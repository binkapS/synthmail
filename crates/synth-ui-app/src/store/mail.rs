// MailStore — folders, messages, reading pane, compose/reply/search.
// Fetch via worker threads + channel + slint::Timer drain on main thread.
use crate::api::{ApiClient, ApiError};
use crate::slint_generatedApp::{MailFull, MailRow};
use slint::{Model, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Deserialize, Default)]
struct FolderResp { folders: Vec<String> }

#[derive(Deserialize, Default)]
struct MessagesResp { messages: Vec<MessageDto> }

#[derive(Deserialize, Default)]
struct MessageDto { uid: i32, from: String, subject: String, preview: String,
                     date: String, seen: bool }
impl From<&MessageDto> for MailRow {
    fn from(m: &MessageDto) -> Self {
        MailRow { uid: m.uid, from: m.from.clone().into(), subject: m.subject.clone().into(),
                  preview: m.preview.clone().into(), date: m.date.clone().into(), seen: m.seen }
    }
}

#[derive(Deserialize, Default)]
struct MessageFullDto { uid: i32, from: String, subject: String, date: String,
                        seen: bool, raw: String }
impl From<&MessageFullDto> for MailFull {
    fn from(m: &MessageFullDto) -> Self {
        MailFull { uid: m.uid, from: m.from.clone().into(), subject: m.subject.clone().into(),
                   date: m.date.clone().into(), seen: m.seen, raw: m.raw.clone().into() }
    }
}

use serde::Deserialize;

pub struct MailStore {
    pub folders: Rc<VecModel<SharedString>>,
    pub messages: Rc<VecModel<MailRow>>,
    pub full: Rc<VecModel<MailFull>>,
    pub timers: RefCell<Vec<slint::Timer>>,
}

impl MailStore {
    pub fn new() -> Self {
        Self {
            folders: Rc::new(VecModel::from(vec![])),
            messages: Rc::new(VecModel::from(vec![])),
            full: Rc::new(VecModel::from(vec![])),
            timers: RefCell::new(vec![]),
        }
    }

    fn client() -> ApiClient { crate::store::auth::client() }

    fn drain<M: Clone + 'static>(
        &self, model: &Rc<VecModel<M>>, rx: mpsc::Receiver<Result<Vec<M>, ApiError>>,
    ) {
        let model = model.clone();
        let rx = Arc::new(Mutex::new(rx));
        let timer = slint::Timer::default();
        let mut stopped = false;
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(20), move || {
            if stopped { return; }
            if let Ok(result) = rx.lock().unwrap().try_recv() {
                stopped = true;
                if let Ok(rows) = result {
                    model.clear();
                    model.extend_from_slice(&rows);
                }
            }
        });
        self.timers.borrow_mut().push(timer);
    }

    fn set_single<M: Clone + 'static>(model: &Rc<VecModel<M>>, row: Option<M>) {
        model.clear();
        if let Some(r) = row { model.push(r); }
    }

    pub fn load_folders(&self) {
        let c = Self::client();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || { let _ = tx.send(c.get::<FolderResp>("/api/v1/mail/folders").map(|r| r.folders.iter().map(|s| SharedString::from(s.as_str())).collect())); });
        self.drain(&self.folders, rx);
    }

    pub fn load_messages(&self, folder: &str) {
        let c = Self::client();
        let folder = folder.to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<MessagesResp>(&format!("/api/v1/mail/messages?folder={}&limit=50",
                urlenc(&folder))).map(|r| r.messages.iter().map(MailRow::from).collect()));
        });
        self.drain(&self.messages, rx);
    }

    pub fn load_full(&self, uid: i32) {
        let c = Self::client();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<MessageFullDto>(&format!("/api/v1/mail/messages/{uid}")).map(|m| vec![MailFull::from(&m)]));
        });
        Self::set_single(&self.full, None);
        self.drain(&self.full, rx);
    }

    pub fn send(&self, to: SharedString, subject: SharedString, body: SharedString) {
        let c = Self::client();
        std::thread::spawn(move || {
            let _ = c.post::<serde_json::Value>("/api/v1/mail/send", &serde_json::json!(
                {"from": "", "to": to.to_string(), "subject": subject.to_string(), "body": body.to_string()}));
        });
    }

    pub fn reply(&self, to: SharedString, text: SharedString) {
        let c = Self::client();
        std::thread::spawn(move || {
            let _ = c.post::<serde_json::Value>("/api/v1/mail/send", &serde_json::json!(
                {"from": "", "to": to.to_string(), "subject": text.to_string(), "body": ""}));
        });
    }

    pub fn delete_draft(&self, uid: i32) {
        let c = Self::client();
        std::thread::spawn(move || {
            let _ = c.post::<serde_json::Value>("/api/v1/mail/drafts", &serde_json::json!({"uid": uid}));
        });
    }
}

fn urlenc(s: &str) -> String {
    s.replace(' ', "%20").replace('&', "%26").replace('=', "%3D")
}
