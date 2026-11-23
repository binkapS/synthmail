// REST API client (control plane + webmail) built on reqwest.
mod client;
mod models;

pub use client::{ApiClient, ApiError, DEFAULT_CONTROL_URL, DEFAULT_WEBMAIL_URL};
