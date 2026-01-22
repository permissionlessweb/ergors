use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid SDL template: {reason}")]
    InvalidTemplate { reason: String },

    #[error("Variable not found: {key}")]
    VariableNotFound { key: String },

    #[error("Invalid JSON: {0}")]
    InvalidJson(String),

    #[error("Admin not set")]
    AdminNotSet {},

    #[error("Missing variable defaults: {variables:?}")]
    MissingVariableDefaults { variables: Vec<String> },
}
