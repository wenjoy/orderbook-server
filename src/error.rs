pub enum EngineError {
    OrderNotFound(uuid::Uuid),
    InvalidOrder(String),
    InSufficientLiquidity,
    OrderAlreadyClosed,
}

pub type Result<T> = std::result::Result<T, EngineError>;
