pub mod auth;
pub mod config;
pub mod dmarc;
pub mod dns_check;
pub mod error;
pub mod hibp;
pub mod metrics;
pub mod policy;
pub mod rate_limit;
pub mod secrets;
pub mod telemetry;
pub mod totp;

pub use auth::{create_session_token, decode_session_token, hash_password, verify_password};
pub use config::Config;
pub use error::{AppError, AppResult, ErrorResponse};
pub use policy::{check_password_allowed, validate_password_strength};
pub use telemetry::init_telemetry;
