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

// Re-export akash modules at the expected locations for generated code
pub mod market {
    pub use crate::types::ergors::akash::market::*;
}
pub mod manifest {
    pub use crate::types::ergors::akash::manifest::*;
}

pub mod akash {

    pub mod base {
        pub mod attributes {
            pub mod v1 {
                include!("gen/akash.base.attributes.v1.rs");
            }
        }
        pub mod deposit {
            pub mod v1 {
                include!("gen/akash.base.deposit.v1.rs");
            }
        }
        pub mod resources {
            pub mod v1beta4 {
                include!("gen/akash.base.resources.v1beta4.rs");
            }
        }
        pub mod v1beta3 {
            include!("gen/akash.base.v1beta3.rs");
        }
    }

    pub mod cert {
        pub mod v1 {
            include!("gen/akash.cert.v1.rs");
        }
    }

    pub mod deployment {
        pub mod v1 {
            include!("gen/akash.deployment.v1.rs");
        }
        pub mod v1beta3 {
            include!("gen/akash.deployment.v1beta3.rs");
        }
        pub mod v1beta4 {
            include!("gen/akash.deployment.v1beta4.rs");
        }
        pub mod v1beta5 {
            include!("gen/akash.deployment.v1beta5.rs");
        }
    }

    pub mod discovery {
        pub mod v1 {
            include!("gen/akash.discovery.v1.rs");
        }
    }

    pub mod escrow {
        pub mod id {
            pub mod v1 {
                include!("gen/akash.escrow.id.v1.rs");
            }
        }
        pub mod types {
            pub mod v1 {
                include!("gen/akash.escrow.types.v1.rs");
            }
        }
        pub mod v1 {
            include!("gen/akash.escrow.v1.rs");
        }
    }

    pub mod manifest {
        pub mod v2beta2 {
            include!("gen/akash.manifest.v2beta2.rs");
        }
    }

    pub mod market {
        pub mod v1 {
            include!("gen/akash.market.v1.rs");
        }
        pub mod v1beta4 {
            include!("gen/akash.market.v1beta4.rs");
        }
        pub mod v2beta1 {
            include!("gen/akash.market.v2beta1.rs");
        }
    }

    pub mod provider {
        pub mod lease {
            pub mod v1 {
                include!("gen/akash.provider.lease.v1.rs");
            }
        }
        pub mod v1beta3 {
            include!("gen/akash.provider.v1beta3.rs");
        }
        pub mod v1beta4 {
            include!("gen/akash.provider.v1beta4.rs");
        }
    }
}

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

pub mod cosmos {
    pub mod base {
        pub mod v1beta1 {
            include!("gen/cosmos.base.v1beta1.rs");
        }
        pub mod query {
            pub mod v1beta1 {
                include!("gen/cosmos.base.query.v1beta1.rs");
            }
        }
    }
    pub mod v1 {
        include!("gen/cosmos_proto.rs");
        // include!("gen/cosmos.ics23.v1.rs");
    }
}

pub mod cosmwasm {
    pub mod wasm {
        pub mod v1 {
            include!("gen/cosmwasm.wasm.v1.rs");
        }
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

pub mod proxy {
    pub mod v1 {
        include!("gen/ergors.proxy.v1.rs");
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

// pub mod ibc {
//     pub mod v1 {
//         include!("gen/ergors.ibc.v1.rs");
//         // include!("gen/ergors.ibc.v1.serde.rs");
//     }
// }

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

pub mod git {
    pub mod v1 {
        include!("gen/ergors.git.v1.rs");
    }
}

pub mod management {
    pub mod v1 {
        include!("gen/ergors.management.v1.rs");
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
