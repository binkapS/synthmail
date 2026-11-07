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
            AppError::Database(_) | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn to_problem_json(&self) -> Value {
        json!({
            "type": "about:blank",
            "title": self.status().canonical_reason().unwrap_or("error"),
            "status": self.status().as_u16(),
            "detail": self.to_string(),
        })
    }
}

#[derive(Debug)]
pub struct ErrorResponse(pub AppError);

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let err = self.0;
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
