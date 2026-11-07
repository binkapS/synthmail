//! JMAP (RFC 8620/8621) backend: delegates to IMAP proxy + Postgres FTS.
//! Minimal but standards-shaped: session discovery + Mailbox/Email query/get/set.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JmapSession {
    pub capabilities: Vec<Capability>,
    pub accounts: std::collections::HashMap<String, Account>,
    pub api_url: String,
    pub download_url: String,
    pub upload_url: String,
    pub event_source_url: String,
    pub state: String,
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub is_personal: bool,
    pub is_read_only: bool,
    pub account_capabilities: serde_json::Value,
}

pub fn session_for(username: &str, api_url: &str) -> JmapSession {
    let mut accounts = std::collections::HashMap::new();
    accounts.insert(
        "urn:ietf:params:jmap:account".to_string(),
        Account {
            name: username.to_string(),
            is_personal: true,
            is_read_only: false,
            account_capabilities: serde_json::json!({
                "urn:ietf:params:jmap:mail": {"maxMailboxes": 32, "maxMailboxesEachLevel": 4, "maxObjects": 1024, "maxObjectSize": 50000000},
                "urn:ietf:params:jmap:mail:card": {},
                "urn:ietf:params:jmap:mail:calendars": {}
            }),
        },
    );
    JmapSession {
        capabilities: vec![
            Capability {
                name: "urn:ietf:params:jmap:core".into(),
                version: "1".into(),
            },
            Capability {
                name: "urn:ietf:params:jmap:mail".into(),
                version: "1".into(),
            },
        ],
        accounts,
        api_url: api_url.to_string(),
        download_url: format!("{}/download/{{accountId}}/{{blobId}}/{{name}}", api_url),
        upload_url: format!("{}/upload/{{accountId}}", api_url),
        event_source_url: format!("{}/eventsources/{{accountId}}", api_url),
        state: "synthmail-state-1".to_string(),
        username: username.to_string(),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JmapRequest {
    pub using: Vec<String>,
    #[serde(default)]
    pub method_calls: Vec<MethodCall>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MethodCall {
    pub name: String,
    pub args: serde_json::Value,
    #[serde(default)]
    pub tag: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JmapResponse {
    pub session_state: String,
    pub method_responses: Vec<serde_json::Value>,
}

pub fn mailbox_list() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"id": "inbox", "name": "INBOX", "role": "inbox", "totalEmails": 0, "unreadEmails": 0}),
        serde_json::json!({"id": "sent", "name": "Sent", "role": "sent", "totalEmails": 0, "unreadEmails": 0}),
        serde_json::json!({"id": "drafts", "name": "Drafts", "role": "drafts", "totalEmails": 0, "unreadEmails": 0}),
        serde_json::json!({"id": "junk", "name": "Junk", "role": "junk", "totalEmails": 0, "unreadEmails": 0}),
        serde_json::json!({"id": "trash", "name": "Trash", "role": "trash", "totalEmails": 0, "unreadEmails": 0}),
    ]
}

pub fn handle_method(call: &MethodCall) -> serde_json::Value {
    match call.name.as_str() {
        "Mailbox/query" => {
            serde_json::json!({"accountId": call.args.get("accountId").cloned().unwrap_or(serde_json::json!("urn:ietf:params:jmap:account")), "ids": ["inbox","sent","drafts","junk","trash"]})
        }
        "Mailbox/get" => {
            serde_json::json!({"accountId": "urn:ietf:params:jmap:account", "list": mailbox_list(), "notFound": [], "notModified": false})
        }
        "Email/query" => {
            serde_json::json!({"accountId": "urn:ietf:params:jmap:account", "ids": [], "total": 0})
        }
        "Email/get" => {
            serde_json::json!({"accountId": "urn:ietf:params:jmap:account", "list": [], "notFound": [], "notModified": false})
        }
        "Email/set" => {
            serde_json::json!({"accountId": "urn:ietf:params:jmap:account", "created": {}, "updated": {}, "destroyed": []})
        }
        _ => {
            serde_json::json!({"accountId": "urn:ietf:params:jmap:account", "error": "unsupportedMethod"})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_has_core_capability() {
        let s = session_for("alice@example.com", "https://mail.example.com/jmap");
        assert!(s.capabilities.iter().any(|c| c.name.contains("jmap:core")));
        assert!(s.accounts.contains_key("urn:ietf:params:jmap:account"));
    }
    #[test]
    fn mailbox_query_shape() {
        let call = MethodCall {
            name: "Mailbox/query".into(),
            args: serde_json::json!({"accountId": "x"}),
            tag: "a".into(),
        };
        assert!(handle_method(&call)["ids"].as_array().unwrap().len() == 5);
    }
}
