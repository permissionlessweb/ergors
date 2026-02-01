use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Address already authorized: {address}")]
    AlreadyAuthorized { address: String },

    #[error("Address not found in authorized list: {address}")]
    NotAuthorized { address: String },

    #[error("Cannot remove admin from authorized list")]
    CannotRemoveAdmin {},

    #[error("Invalid address format: {address}")]
    InvalidAddress { address: String },
}
