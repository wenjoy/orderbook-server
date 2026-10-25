use axum::{http::StatusCode, response::IntoResponse};

#[derive(Debug)]
pub enum EngineError {
    OrderNotFound(uuid::Uuid),
    InvalidOrder(String),
    InSufficientLiquidity,
    OrderAlreadyClosed,
}

pub type EngineResult<T> = std::result::Result<T, EngineError>;

pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Internale(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m).into_response(),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m).into_response(),
            Self::Internale(m) => (StatusCode::INTERNAL_SERVER_ERROR, m).into_response(),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
