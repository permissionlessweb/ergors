//! Protobuf definitions for Ergors.
//!
//! This crate only contains the `.proto` files and the Rust types generated
//! from them.  These types only handle parsing the wire format; validation
//! should be performed by converting them into an appropriate domain type, as
//! in the following diagram:
//!
//! ```ascii
//! ┌───────┐          ┌──────────────┐               ┌──────────────┐
//! │encoded│ protobuf │ ho-std TryFrom/Into  │ domain types │
//! │ bytes │<──wire ─>│    types     │<─validation ─>│(other crates)│
//! └───────┘  format  └──────────────┘   boundary    └──────────────┘
//! ```
//!
//! The [`DomainType`] marker trait can be implemented on a domain type to ensure
//! these conversions exist.

// The autogen code is not clippy-clean, so we disable some clippy warnings for this crate.
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::unwrap_used)]
#![allow(non_snake_case)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub use prost::{Message, Name};

pub mod custody {
    pub mod v1 {
        include!("gen/hoe.custody.v1.rs");
    }
}
pub mod keys {
    pub mod v1 {
        include!("gen/hoe.keys.v1.rs");
    }
}
pub mod network {
    pub mod v1 {
        include!("gen/hoe.network.v1.rs");
    }
}

pub mod orchestration {

    pub mod v1 {
        include!("gen/hoe.orchestration.v1.rs");
    }
}
pub mod storage {

    pub mod v1 {
        include!("gen/hoe.storage.v1.rs");
    }
}
pub mod types {
    pub mod v1 {
        include!("gen/hoe.types.v1.rs");
    }
}
