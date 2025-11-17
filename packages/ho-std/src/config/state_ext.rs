//! ## Storage Key Structure:: Env Variables & Api Keys

//! ### Key Design Rationale

use crate::error::{HoError, HoResult};

use crate::orchestrate::{LlmEntity, LlmRouterConfig};
use crate::traits::StateWrite;
use async_trait::async_trait;
use cnidarium::StateRead;
use futures::StreamExt;

pub mod state_key {}

/// Extension trait for reading LLM configurations from verifiable storage
#[async_trait]
pub trait StateReadExt: StateRead {}

impl<T: StateRead + ?Sized> StateReadExt for T {}

/// Extension trait for writing LLM configurations to verifiable storage
#[async_trait]
pub trait StateWriteExt: StateWrite {}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}
