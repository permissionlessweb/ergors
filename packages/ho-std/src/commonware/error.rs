//! Network error types

use thiserror::Error;

use crate::error::HoError;

#[derive(Error, Debug)]
pub enum CommonwareNetworkError {
    #[error("P2P error: {0}")]
    P2P(String), // We'll convert from commonware-p2p errors

    #[error("Broadcast error: {0}")]
    Broadcast(String), // We'll convert from commonware-broadcast errors

    #[error("Collector timeout")]
    CollectorTimeout,

    #[error("ConfigError: {0}")]
    ConfigError(String),

    #[error("No peers with role: {0}")]
    NoPeersForRole(String),

    #[error("HoError: {0}")]
    HoError(#[from] HoError),

    #[error("CommonwareLookupError: {0}")]
    CommonwareLookupError(#[from] commonware_p2p::authenticated::lookup::Error),

    #[error("Message serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid node type: {0}")]
    InvalidNodeType(String),

    #[error("Network not initialized")]
    NotInitialized,

    #[error("NodePrivKeyNotFound")]
    NodePrivKeyNotFound,

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Channel error: {0}")]
    ChannelError(String),
}

pub type CommonwareNetworkResult<T> = std::result::Result<T, CommonwareNetworkError>;
