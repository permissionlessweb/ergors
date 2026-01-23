use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Address already in whitelist: {address}")]
    AlreadyWhitelisted { address: String },

    #[error("Address not in whitelist: {address}")]
    NotWhitelisted { address: String },

    #[error("Invalid address format: {address}")]
    InvalidAddress { address: String },

    #[error("Batch operation limit exceeded: max {max}, got {got}")]
    BatchLimitExceeded { max: u32, got: u32 },
}
