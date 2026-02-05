#[derive(Debug, Clone, PartialEq)]
pub enum MarketEvent {
    Tick {
        timestamp: i64,
        price: f64,
    },
    Trade {
        timestamp: i64,
        price: f64,
        quantity: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamError {
    Disconnected(String),
    Protocol(String),
    InvalidData(String),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Disconnected(msg) => write!(f, "disconnected: {msg}"),
            StreamError::Protocol(msg) => write!(f, "protocol: {msg}"),
            StreamError::InvalidData(msg) => write!(f, "invalid data: {msg}"),
        }
    }
}

pub trait MarketStream {
    fn next_event(&mut self) -> Result<MarketEvent, StreamError>;
}
