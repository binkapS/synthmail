// reqwest-based API client. Error strings mirror the old web UI ("HTTP 401: …").
use super::models::{LoginResp, MeResp};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug)]
pub struct ApiError {
    pub status: Option<u16>,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(status) => write!(f, "HTTP {status}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ApiError {}

pub struct ApiClient {
    base_url: String,
    webmail_url: String,
    http: reqwest::blocking::Client,
    token: Option<String>,
}

pub const DEFAULT_CONTROL_URL: &str = "http://synth-control:8000";
pub const DEFAULT_WEBMAIL_URL: &str = "http://synth-webmail:3000";

fn parse_json<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, ApiError> {
    serde_json::from_str(text.trim())
        .map_err(|e| ApiError { status: None, message: format!("invalid response: {e}") })
}

impl ApiClient {
    pub fn new(base_url: &str, webmail_url: &str, token: Option<String>) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self { base_url: base_url.trim_end_matches('/').to_string(),
               webmail_url: webmail_url.trim_end_matches('/').to_string(),
               http, token }
    }

    pub fn with_token(&mut self, token: Option<String>) { self.token = token; }

    fn call(&self, method: reqwest::Method, url: &str, body: Option<&impl Serialize>)
            -> Result<String, ApiError> {
        let url = format!("{url}");
        let mut req = self.http.request(method, &url);
        if let Some(t) = &self.token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().map_err(|e| ApiError { status: None, message: e.to_string() })?;
        let status = resp.status().as_u16();
        let text = resp.text().map_err(|e| ApiError { status: Some(status), message: e.to_string() })?;
        if !(200..300).contains(&status) {
            let detail = parse_json::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("detail").and_then(|d| d.as_str()).map(str::to_string))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "request failed".into());
            return Err(ApiError { status: Some(status), message: detail });
        }
        Ok(text)
    }

    pub fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ApiError> {
        let text = self.call(reqwest::Method::GET, &format!("{}{path}", self.base_url), None::<&serde_json::Value>)?;
        parse_json(&text)
    }

    pub fn post<T: for<'de> Deserialize<'de>, B: Serialize>(&self, path: &str, body: &B) -> Result<T, ApiError> {
        let text = self.call(reqwest::Method::POST, &format!("{}{path}", self.base_url), Some(body))?;
        parse_json(&text)
    }

    pub fn put<T: for<'de> Deserialize<'de>, B: Serialize>(&self, path: &str, body: &B) -> Result<T, ApiError> {
        let text = self.call(reqwest::Method::PUT, &format!("{}{path}", self.base_url), Some(body))?;
        parse_json(&text)
    }

    pub fn delete<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ApiError> {
        let text = self.call(reqwest::Method::DELETE, &format!("{}{path}", self.base_url), None::<&serde_json::Value>)?;
        parse_json(&text)
    }

    // ---- auth (control plane) ----
    pub fn login(&self, email: &str, password: &str, totp_code: Option<&str>) -> Result<LoginResp, ApiError> {
        let mut body = serde_json::json!({ "email": email, "password": password });
        if let Some(t) = totp_code.filter(|t| !t.is_empty()) {
            body["totp_code"] = serde_json::json!(t);
        }
        self.post("/api/v1/auth/login", &body)
    }

    pub fn me(&self) -> Result<MeResp, ApiError> {
        self.get("/api/v1/auth/me")
    }

    // ---- webmail ----
    pub fn mail_folders(&self) -> Result<serde_json::Value, ApiError> {
        self.get(&format!("{}/api/v1/mail/folders", self.webmail_url))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn api_error_display() {
        let e = super::ApiError { status: Some(401), message: "expired".into() };
        assert_eq!(e.to_string(), "HTTP 401: expired");
    }
}
