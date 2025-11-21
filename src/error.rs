use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrderBookError {
    #[error("Order not found: {0}")]
    OrderNotFound(u32),

    #[error("Trade buffer full, cannot record more trades")]
    TradeBufferFull,

    #[error("Failed to persist event: {0}")]
    PersistenceFailed(String),

    #[error("Invalid Error: {0}")]
    SerializationFailed(String),

    #[error("Broadcast Failed: {0}")]
    InvalidOrder(String),

    #[error("Channel Closed")]
    ChannelClosed,
}

pub type Result<T> = std::result::Result<T, OrderBookError>;

#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    #[error("Price cannot be zero")]
    ZeroPrice,

    #[error("Quantity cannot be zero")]
    ZeroQuantity,

    #[error("Price exceeds maximum: {0}")]
    PriceOverflow(u32),

    #[error("Invalid order ID: {0}")]
    InvalidOrder(u32),
}

#[derive(Error, Debug, Clone)]
pub enum PersistenceError {
    #[error("Failed to send to persistence channel: {0}")]
    ChannelSendFailed(String),

    #[error("Database connection lost")]
    ConnectionLost,

    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
}

#[derive(Error, Debug, Clone)]
pub enum BroadcastError {
    #[error("No active subscribers")]
    NoSubcribers,

    #[error("Broadcast channel full")]
    ChannelFull,

    #[error("Failed to encode message: {0}")]
    EncodingFailed(String),
}
