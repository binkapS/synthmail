use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("validation error: {0}")]
    Validation(String),
    #[error("forbidden")]
    Forbidden,
    #[error("too many requests")]
    TooManyRequests,
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Forbidden => StatusCode::FORBIDDEN,
            AppError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            AppError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            AppError::Database(_) | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Server-side detail for logging. Internal errors are only logged, never
    /// returned to clients (M-07: error responses must not leak SQL/schema).
    fn server_detail(&self) -> String {
        self.to_string()
    }

    /// Client-safe detail. 5xx classes return a generic message (M-07).
    pub fn to_problem_json(&self) -> Value {
        let detail = match self {
            AppError::Database(_) | AppError::Internal(_) => {
                "an internal error occurred".to_string()
            }
            other => other.server_detail(),
        };
        json!({
            "type": "about:blank",
            "title": self.status().canonical_reason().unwrap_or("error"),
            "status": self.status().as_u16(),
            "detail": detail,
        })
    }
}

#[derive(Debug)]
pub struct ErrorResponse(pub AppError);

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let err = self.0;
        // Log full detail server-side; the response body is redacted for 5xx.
        match &err {
            AppError::Database(_) | AppError::Internal(_) => {
                tracing::error!(detail = %err.server_detail(), "returning 5xx to client");
                let resp = (err.status(), Json(err.to_problem_json())).into_response();
                return resp;
            }
            _ => {}
        }
        let status = err.status();
        let body = err.to_problem_json();
        (status, Json(body)).into_response()
    }
}

impl From<AppError> for ErrorResponse {
    fn from(e: AppError) -> Self {
        ErrorResponse(e)
    }
}

pub type AppResult<T> = Result<T, AppError>;
