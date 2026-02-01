pub mod contract;
pub mod error;
pub mod msg;
pub mod state;

pub use crate::error::ContractError;
pub use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};

// Re-export entry points when not used as a library
#[cfg(not(feature = "library"))]
pub use crate::contract::{execute, instantiate, query};
