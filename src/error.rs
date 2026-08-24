use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid protocol message: {0}")]
    Protocol(String),
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("network error: {0}")]
    Network(#[from] std::io::Error),
    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] Box<bincode::ErrorKind>),
    #[error("channel closed")]
    ChannelClosed,
}

pub type AppResult<T> = Result<T, AppError>;
