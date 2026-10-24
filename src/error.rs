#[derive(Debug)]
pub enum EngineError {
    OrderNotFound(uuid::Uuid),
    InvalidOrder(String),
    InSufficientLiquidity,
    OrderAlreadyClosed,
}

pub type EngineResult<T> = std::result::Result<T, EngineError>;
