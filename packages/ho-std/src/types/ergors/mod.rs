//! Protobuf definitions for Ergors.
//!
//! This crate only contains the `.proto` files and the Rust types generated
//! from them.  These types only handle parsing the wire format; validation
//! should be performed by converting them into an appropriate domain type, as
//! in the following diagram:
//!
//! ```ascii
//! ┌───────┐          ┌──────────────┐               ┌──────────────┐
//! │encoded│ protobuf │ ergors TryFrom/Into  │ domain types │
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

pub mod actions {
    pub mod v1 {
        include!("gen/ergors.actions.v1.rs");
    }
}
pub mod asset {
    pub mod v1 {
        include!("gen/ergors.asset.v1.rs");
    }
}
pub mod custody {
    pub mod v1 {
        include!("gen/ergors.custody.v1.rs");
    }
}
pub mod keys {
    pub mod v1 {
        include!("gen/ergors.keys.v1.rs");
    }
}
pub mod decaf377_rdsa {
    pub mod v1 {
        include!("gen/ergors.decaf377_rdsa.v1.rs");
    }
}
pub mod decaf377_fmd {
    pub mod v1 {
        include!("gen/ergors.decaf377_fmd.v1.rs");
    }
}
pub mod decaf377_frost {
    pub mod v1 {
        include!("gen/ergors.decaf377_frost.v1.rs");
    }
}
pub mod network {
    pub mod v1 {
        include!("gen/ergors.network.v1.rs");
    }
}
pub mod view {
    pub mod v1 {
        include!("gen/ergors.view.v1.rs");
    }
}

pub mod orch {

    pub mod v1 {
        include!("gen/ergors.orch.v1.rs");
    }
}
pub mod storage {

    pub mod v1 {
        include!("gen/ergors.storage.v1.rs");
    }
}

pub mod sct {
    pub mod v1 {
        include!("gen/ergors.sct.v1.rs");
    }
}
pub mod tct {
    pub mod v1 {
        include!("gen/ergors.tct.v1.rs");
    }
}

pub mod num {
    pub mod v1 {
        include!("gen/ergors.num.v1.rs");
    }
}

pub mod ibc {
    pub mod v1 {
        include!("gen/ergors.ibc.v1.rs");
        // include!("gen/ergors.ibc.v1.serde.rs");
    }
}

pub mod txhash {
    pub mod v1 {
        include!("gen/ergors.txhash.v1.rs");
    }
}
pub mod types {
    pub mod v1 {
        include!("gen/ergors.types.v1.rs");
    }
}

pub mod tendermint {
    pub mod crypto {
        include!("gen/tendermint.crypto.rs");
    }

    #[allow(clippy::large_enum_variant)]
    pub mod types {
        include!("gen/tendermint.types.rs");
    }

    pub mod version {
        include!("gen/tendermint.version.rs");
    }

    pub mod p2p {
        include!("gen/tendermint.p2p.rs");
    }

    pub mod abci {
        include!("gen/tendermint.abci.rs");
    }
}
