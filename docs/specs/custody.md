# Answer

The custody module in Penumbra is a modular system for managing cryptographic keys and signing transactions [1](#0-0) . It provides multiple backend implementations for different security and operational requirements [2](#0-1) .

## Core Architecture

The custody module exposes several key components [3](#0-2) :

- **Client interface** (`CustodyClient`) - provides the API for interacting with custody services
- **Request types** - `AuthorizeRequest`, `AuthorizeValidatorDefinitionRequest`, `AuthorizeValidatorVoteRequest`
- **Pre-authorization** - `PreAuthorization` for policy-based signing

## Backend Implementations

The module supports five custody backends [4](#0-3) :

### 1. ViewOnly (`null_kms`)
A read-only backend that cannot sign transactions [5](#0-4) . Used for monitoring wallets without spending authority.

### 2. SoftKms (`soft_kms`)
A software-based key management system that stores the spend key in memory [6](#0-5) . It implements the `CustodyService` trait with methods for:
- `authorize` - signs transaction plans [7](#0-6) 
- `authorize_validator_definition` - signs validator definitions [8](#0-7) 
- `authorize_validator_vote` - signs validator votes [9](#0-8) 
- `export_full_viewing_key` - exports the FVK [10](#0-9) 
- `confirm_address` - verifies addresses [11](#0-10) 

### 3. Threshold (`threshold`)
A threshold signing backend where the spend key is split into shares [12](#0-11) . Multiple participants must cooperate to produce signatures through a multi-round protocol [13](#0-12) . This backend implements the same `CustodyService` interface [14](#0-13) .

### 4. Encrypted (`encrypted`)
Wraps other backends with password-based encryption [15](#0-14) . Can encrypt either SoftKms or Threshold configurations [16](#0-15) .

### 5. Ledger (optional)
Hardware wallet integration for external signing devices [17](#0-16) .

## Integration Points

The custody module integrates with the broader system through:

1. **gRPC Service** - All backends implement `custody_service_server::CustodyService` and are exposed via gRPC [18](#0-17) 

2. **Local Service Pattern** - In `pcli`, custody services run in-process using `box_grpc_svc::local()` to create local gRPC clients [19](#0-18) 

3. **Remote Service Pattern** - In `pclientd`, the custody service can be exposed over the network [20](#0-19) 

4. **Governance Separation** - Supports separate custody backends for validator governance operations [21](#0-20) 

## Dependencies

The module depends on cryptographic primitives including [22](#0-21) :
- `decaf377` and `decaf377-rdsa` for signatures
- `decaf377-frost` for threshold signing
- `chacha20poly1305` and `argon2` for encryption
- Penumbra SDK components for keys, transactions, and governance

# Notes

The custody module is designed with modularity as a core principle, allowing users to choose the security model that fits their needs. The threshold backend is particularly notable for enabling multi-party custody without any single party having full spending authority. All backends share a common gRPC interface defined in the proto module [23](#0-22) , ensuring consistent integration across different custody strategies.

Wiki pages you might want to explore:
- [Overview (penumbra-zone/penumbra)](/wiki/penumbra-zone/penumbra#1)

### Citations

**File:** crates/custody/Cargo.toml (L5-5)
```text
description = "Modular custody for Penumbra"
```

**File:** crates/custody/Cargo.toml (L10-39)
```text
[dependencies]
anyhow = {workspace = true}
argon2 = "0.5"
ark-ff = {workspace = true}
ark-serialize = {workspace = true}
base64 = {workspace = true}
blake2b_simd = {workspace = true}
bytes = {workspace = true, features = ["serde"]}
chacha20poly1305 = {workspace = true}
decaf377 = {workspace = true}
decaf377-frost = { version = "2.1.0", path = "../crypto/decaf377-frost" }
decaf377-ka = {workspace = true}
decaf377-rdsa = {workspace = true}
ed25519-consensus = {workspace = true}
futures = {workspace = true}
hex = {workspace = true}
penumbra-sdk-governance = {workspace = true, default-features = false}
penumbra-sdk-keys = {workspace = true, default-features = true}
penumbra-sdk-proto = {workspace = true, features = ["rpc"], default-features = true}
penumbra-sdk-stake = {workspace = true, default-features = false}
penumbra-sdk-transaction = {workspace = true, default-features = true}
penumbra-sdk-txhash = {workspace = true, default-features = true}
prost = {workspace = true}
rand_core = {workspace = true}
serde = {workspace = true, features = ["derive"]}
serde_json = {workspace = true}
serde_with = {workspace = true, features = ["hex"]}
tokio = {workspace = true, features = ["full"]}
tonic = {workspace = true}
tracing = {workspace = true}
```

**File:** crates/custody/src/lib.rs (L1-5)
```rust
//! Implementations of custody services responsible for signing transactions.
//!
//! This crate currently focuses on the [`soft_kms`] implementation, a basic
//! software key management system that can perform basic policy-based
//! authorization or blind signing.
```

**File:** crates/custody/src/lib.rs (L13-28)
```rust
mod client;
mod pre_auth;
mod request;
mod terminal;

pub mod encrypted;
pub mod null_kms;
pub mod policy;
pub mod soft_kms;
pub mod threshold;

pub use client::CustodyClient;
pub use pre_auth::PreAuthorization;
pub use request::{
    AuthorizeRequest, AuthorizeValidatorDefinitionRequest, AuthorizeValidatorVoteRequest,
};
```

**File:** crates/bin/pcli/src/config.rs (L65-81)
```rust
/// The custody backend to use.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(tag = "backend")]
#[allow(clippy::large_enum_variant)]
pub enum CustodyConfig {
    /// A view-only client that can't sign transactions.
    ViewOnly,
    /// A software key management service.
    SoftKms(SoftKmsConfig),
    /// A manual threshold custody service.
    Threshold(ThresholdConfig),
    /// An encrypted custody service.
    Encrypted(EncryptedConfig),
    /// A custody service using an external ledger device.
    #[cfg(feature = "ledger")]
    Ledger(LedgerConfig),
}
```

**File:** crates/bin/pcli/src/opt.rs (L69-112)
```rust
        // Build the custody service...
        let custody = match &config.custody {
            CustodyConfig::ViewOnly => {
                tracing::info!("using view-only custody service");
                let null_kms = NullKms::default();
                let custody_svc = CustodyServiceServer::new(null_kms);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
            CustodyConfig::SoftKms(config) => {
                tracing::info!("using software KMS custody service");
                let soft_kms = SoftKms::new(config.clone());
                let custody_svc = CustodyServiceServer::new(soft_kms);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
            CustodyConfig::Threshold(config) => {
                tracing::info!("using manual threshold custody service");
                let threshold_kms = penumbra_sdk_custody::threshold::Threshold::new(
                    config.clone(),
                    ActualTerminal {
                        fvk: Some(fvk.clone()),
                    },
                );
                let custody_svc = CustodyServiceServer::new(threshold_kms);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
            CustodyConfig::Encrypted(config) => {
                tracing::info!("using encrypted custody service");
                let encrypted_kms = penumbra_sdk_custody::encrypted::Encrypted::new(
                    config.clone(),
                    ActualTerminal {
                        fvk: Some(fvk.clone()),
                    },
                );
                let custody_svc = CustodyServiceServer::new(encrypted_kms);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
            #[cfg(feature = "ledger")]
            CustodyConfig::Ledger(config) => {
                tracing::info!("using ledger custody service");
                let service = penumbra_sdk_custody_ledger_usb::Service::new(config.clone());
                let custody_svc = CustodyServiceServer::new(service);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
        };
```

**File:** crates/bin/pcli/src/opt.rs (L114-147)
```rust
        // Build the governance custody service...
        let governance_custody = match &config.governance_custody {
            Some(separate_governance_custody) => match separate_governance_custody {
                GovernanceCustodyConfig::SoftKms(config) => {
                    tracing::info!(
                        "using separate software KMS custody service for validator voting"
                    );
                    let soft_kms = SoftKms::new(config.clone());
                    let custody_svc = CustodyServiceServer::new(soft_kms);
                    CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
                }
                GovernanceCustodyConfig::Threshold(config) => {
                    tracing::info!(
                        "using separate manual threshold custody service for validator voting"
                    );
                    let threshold_kms = penumbra_sdk_custody::threshold::Threshold::new(
                        config.clone(),
                        ActualTerminal { fvk: Some(fvk) },
                    );
                    let custody_svc = CustodyServiceServer::new(threshold_kms);
                    CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
                }
                GovernanceCustodyConfig::Encrypted { config, .. } => {
                    tracing::info!("using separate encrypted custody service for validator voting");
                    let encrypted_kms = penumbra_sdk_custody::encrypted::Encrypted::new(
                        config.clone(),
                        ActualTerminal { fvk: Some(fvk) },
                    );
                    let custody_svc = CustodyServiceServer::new(encrypted_kms);
                    CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
                }
            },
            None => custody.clone(), // If no separate custody for validator voting, use the same one
        };
```

**File:** crates/custody/src/soft_kms.rs (L97-115)
```rust
    async fn authorize(
        &self,
        request: Request<pb::AuthorizeRequest>,
    ) -> Result<Response<AuthorizeResponse>, Status> {
        let request = request
            .into_inner()
            .try_into()
            .map_err(|e: anyhow::Error| Status::invalid_argument(e.to_string()))?;

        let authorization_data = self
            .sign(&request)
            .map_err(|e| Status::unauthenticated(format!("{e:#}")))?;

        let authorization_response = AuthorizeResponse {
            data: Some(authorization_data.into()),
        };

        Ok(Response::new(authorization_response))
    }
```

**File:** crates/custody/src/soft_kms.rs (L117-135)
```rust
    async fn authorize_validator_definition(
        &self,
        request: Request<pb::AuthorizeValidatorDefinitionRequest>,
    ) -> Result<Response<pb::AuthorizeValidatorDefinitionResponse>, Status> {
        let request = request
            .into_inner()
            .try_into()
            .map_err(|e: anyhow::Error| Status::invalid_argument(e.to_string()))?;

        let validator_definition_auth = self
            .sign_validator_definition(&request)
            .map_err(|e| Status::unauthenticated(format!("{e:#}")))?;

        let authorization_response = pb::AuthorizeValidatorDefinitionResponse {
            validator_definition_auth: Some(validator_definition_auth.into()),
        };

        Ok(Response::new(authorization_response))
    }
```

**File:** crates/custody/src/soft_kms.rs (L137-155)
```rust
    async fn authorize_validator_vote(
        &self,
        request: Request<pb::AuthorizeValidatorVoteRequest>,
    ) -> Result<Response<pb::AuthorizeValidatorVoteResponse>, Status> {
        let request = request
            .into_inner()
            .try_into()
            .map_err(|e: anyhow::Error| Status::invalid_argument(e.to_string()))?;

        let validator_vote_auth = self
            .sign_validator_vote(&request)
            .map_err(|e| Status::unauthenticated(format!("{e:#}")))?;

        let authorization_response = pb::AuthorizeValidatorVoteResponse {
            validator_vote_auth: Some(validator_vote_auth.into()),
        };

        Ok(Response::new(authorization_response))
    }
```

**File:** crates/custody/src/soft_kms.rs (L157-164)
```rust
    async fn export_full_viewing_key(
        &self,
        _request: Request<pb::ExportFullViewingKeyRequest>,
    ) -> Result<Response<pb::ExportFullViewingKeyResponse>, Status> {
        Ok(Response::new(pb::ExportFullViewingKeyResponse {
            full_viewing_key: Some(self.config.spend_key.full_viewing_key().clone().into()),
        }))
    }
```

**File:** crates/custody/src/soft_kms.rs (L166-192)
```rust
    async fn confirm_address(
        &self,
        request: Request<pb::ConfirmAddressRequest>,
    ) -> Result<Response<pb::ConfirmAddressResponse>, Status> {
        let address_index = request
            .into_inner()
            .address_index
            .ok_or_else(|| {
                Status::invalid_argument("missing address index in confirm address request")
            })?
            .try_into()
            .map_err(|e| {
                Status::invalid_argument(format!(
                    "invalid address index in confirm address request: {e:#}"
                ))
            })?;

        let (address, _dtk) = self
            .config
            .spend_key
            .full_viewing_key()
            .payment_address(address_index);

        Ok(Response::new(pb::ConfirmAddressResponse {
            address: Some(address.into()),
        }))
    }
```

**File:** crates/custody/src/threshold.rs (L173-188)
```rust
/// A custody backend using threshold signing.
///
/// This backend is initialized with a full viewing key, but only a share
/// of the spend key, which is not enough to sign on its own. Instead,
/// other signers with the same type of configuration need to cooperate
/// to help produce a signature.
pub struct Threshold<T> {
    config: Config,
    terminal: T,
}

impl<T> Threshold<T> {
    pub fn new(config: Config, terminal: T) -> Self {
        Threshold { config, terminal }
    }
}
```

**File:** crates/custody/src/threshold.rs (L191-234)
```rust
    /// Try and create the necessary signatures to authorize the transaction plan.
    async fn authorize(&self, request: SigningRequest) -> Result<SigningResponse> {
        // Some requests will have no signatures to gather, so there's no need
        // to send around empty threshold signature requests.
        if let Some(out) = no_signature_response(self.config.fvk(), &request)? {
            return Ok(out);
        }
        // Round 1
        let (round1_message, state1) = sign::coordinator_round1(&mut OsRng, &self.config, request)?;
        self.terminal
            .explain("Send this message to the other signers:")?;
        self.terminal.broadcast(&to_json(&round1_message)?).await?;
        self.terminal.explain(&format!(
            "Now, gather at least {} replies from the other signers, and paste them below:",
            self.config.threshold() - 1
        ))?;
        let round1_replies = {
            let mut acc = Vec::<sign::FollowerRound1>::new();
            // We need 1 less, since we've already included ourselves.
            for _ in 1..self.config.threshold() {
                acc.push(self.terminal.next_response().await?);
            }
            acc
        };
        // Round 2
        let (round2_message, state2) =
            sign::coordinator_round2(&self.config, state1, &round1_replies)?;
        self.terminal
            .explain("Send this message to the other signers:")?;
        self.terminal.broadcast(&to_json(&round2_message)?).await?;
        self.terminal.explain(
            "Now, gather the replies from the *same* signers as Round 1, and paste them below:",
        )?;
        let round2_replies = {
            let mut acc = Vec::<sign::FollowerRound2>::new();
            // We need 1 less, since we've already included ourselves.
            for _ in 1..self.config.threshold() {
                acc.push(self.terminal.next_response().await?);
            }
            acc
        };
        // Round 3
        sign::coordinator_round3(&self.config, state2, &round2_replies)
    }
```

**File:** crates/custody/src/threshold.rs (L249-359)
```rust
#[async_trait]
impl<T: Terminal + Sync + Send + 'static> pb::custody_service_server::CustodyService
    for Threshold<T>
{
    async fn authorize(
        &self,
        request: Request<pb::AuthorizeRequest>,
    ) -> Result<Response<pb::AuthorizeResponse>, Status> {
        let request: AuthorizeRequest = request
            .into_inner()
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("{e}")))?;
        let data = self
            .authorize(SigningRequest::TransactionPlan(request.plan))
            .await
            .map_err(|e| {
                Status::internal(format!(
                    "Failed to process transaction authorization request: {e}"
                ))
            })?;
        let SigningResponse::Transaction(data) = data else {
            return Err(Status::internal(
                "expected transaction authorization but custody service returned another kind of authorization data"
                    .to_string()
            ));
        };
        Ok(Response::new(pb::AuthorizeResponse {
            data: Some(data.into()),
        }))
    }

    async fn authorize_validator_definition(
        &self,
        request: Request<pb::AuthorizeValidatorDefinitionRequest>,
    ) -> Result<Response<pb::AuthorizeValidatorDefinitionResponse>, Status> {
        let request: AuthorizeValidatorDefinitionRequest = request
            .into_inner()
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("{e}")))?;
        let data = self
            .authorize(SigningRequest::ValidatorDefinition(
                request.validator_definition,
            ))
            .await
            .map_err(|e| {
                Status::internal(format!(
                    "Failed to process validator definition authorization request: {e}"
                ))
            })?;
        let SigningResponse::ValidatorDefinition(validator_definition_auth) = data else {
            return Err(Status::internal(
                "expected validator definition authorization but custody service returned another kind of authorization data".to_string()
            ));
        };
        Ok(Response::new(pb::AuthorizeValidatorDefinitionResponse {
            validator_definition_auth: Some(validator_definition_auth.into()),
        }))
    }

    async fn authorize_validator_vote(
        &self,
        request: Request<pb::AuthorizeValidatorVoteRequest>,
    ) -> Result<Response<pb::AuthorizeValidatorVoteResponse>, Status> {
        let request: AuthorizeValidatorVoteRequest = request
            .into_inner()
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("{e}")))?;
        let data = self
            .authorize(SigningRequest::ValidatorVote(request.validator_vote))
            .await
            .map_err(|e| {
                Status::internal(format!(
                    "Failed to process validator vote authorization request: {e}"
                ))
            })?;
        let SigningResponse::ValidatorVote(validator_vote_auth) = data else {
            return Err(Status::internal(
                "expected validator vote authorization but custody service returned another kind of authorization data".to_string()
            ));
        };
        Ok(Response::new(pb::AuthorizeValidatorVoteResponse {
            validator_vote_auth: Some(validator_vote_auth.into()),
        }))
    }

    async fn export_full_viewing_key(
        &self,
        _request: Request<pb::ExportFullViewingKeyRequest>,
    ) -> Result<Response<pb::ExportFullViewingKeyResponse>, Status> {
        let fvk = self.export_full_viewing_key();
        Ok(Response::new(pb::ExportFullViewingKeyResponse {
            full_viewing_key: Some(fvk.into()),
        }))
    }

    async fn confirm_address(
        &self,
        request: Request<pb::ConfirmAddressRequest>,
    ) -> Result<Response<pb::ConfirmAddressResponse>, Status> {
        let index = request
            .into_inner()
            .address_index
            .ok_or(anyhow!("ConfirmAddressRequest missing address_index"))
            .and_then(|x| x.try_into())
            .map_err(|e| Status::invalid_argument(format!("{e}")))?;
        let address = self.confirm_address(index);
        Ok(Response::new(pb::ConfirmAddressResponse {
            address: Some(address.into()),
        }))
    }
}
```

**File:** crates/bin/pcli/src/command/init.rs (L316-325)
```rust
                    if self.encrypted {
                        let password = ActualTerminal::get_confirmed_password().await?;
                        CustodyConfig::Encrypted(penumbra_sdk_custody::encrypted::Config::create(
                            &password,
                            penumbra_sdk_custody::encrypted::InnerConfig::SoftKms(spend_key.into()),
                        )?)
                    } else {
                        CustodyConfig::SoftKms(spend_key.into())
                    },
                )
```

**File:** crates/bin/pclientd/src/lib.rs (L345-352)
```rust
                let custody_service = config.kms_config.as_ref().map(|kms_config| {
                    CustodyServiceServer::new(SoftKms::new(kms_config.spend_key.clone().into()))
                });

                let server = Server::builder()
                    .accept_http1(true)
                    .add_service(tonic_web::enable(view_service))
                    .add_optional_service(custody_service.map(tonic_web::enable))
```

**File:** crates/proto/src/lib.rs (L213-226)
```rust
    /// Custody protocol structures.
    pub mod custody {
        pub mod threshold {
            pub mod v1 {
                include!("gen/penumbra.custody.threshold.v1.rs");
                include!("gen/penumbra.custody.threshold.v1.serde.rs");
            }
        }

        pub mod v1 {
            include!("gen/penumbra.custody.v1.rs");
            include!("gen/penumbra.custody.v1.serde.rs");
        }
    }
```
