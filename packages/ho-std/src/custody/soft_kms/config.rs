use crate::custody::policy::AuthPolicy;
use crate::types::keys::v1::SpendKey;
use ed25519_consensus::SigningKey;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Configuration data for the [`SoftKms`](super::SoftKms).
///
/// Contains both transaction signing keys (spend_key) and API keys for LLM providers.
/// All keys are stored in memory and accessed via gRPC custody service.

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub spend_key: SpendKey,
    #[serde(default, skip_serializing_if = "is_default")]
    pub auth_policy: Vec<AuthPolicy>,
    /// Node's identity signing key (for API key encryption)
    #[serde(skip)]
    pub node_key: Option<SigningKey>,
    /// Decrypted API keys stored in memory (provider_name -> api_key)
    #[serde(skip)]
    pub api_keys: HashMap<String, String>,
}

impl From<SpendKey> for Config {
    fn from(spend_key: SpendKey) -> Self {
        Self {
            spend_key,
            auth_policy: Default::default(),
            node_key: None,
            api_keys: HashMap::new(),
        }
    }
}

/// Helper function for Serde serialization, allowing us to skip serialization
/// of default config values.  Rationale: if we don't skip serialization of
/// defaults, if someone serializes a config with some default values, they're
/// "pinning" the current defaults as their choices for all time, and we have no
/// way to distinguish between fields they configured explicitly and ones they
/// passed through from the defaults. If we skip serializing default values,
/// then we know every value in the config was explicitly set.
fn is_default<T: Default + Eq>(value: &T) -> bool {
    *value == T::default()
}

// #[cfg(test)]
// mod tests {
//     use ho_std_keys::keys::{Bip44Path, SeedPhrase};

//     use crate::policy::PreAuthorizationPolicy;

//     use super::*;

//     #[test]
//     fn toml_config_round_trip() {
//         let seed_phrase = SeedPhrase::generate(rand_core::OsRng);
//         let spend_key = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));

//         let pak = ed25519_consensus::SigningKey::new(rand_core::OsRng);
//         let pvk = pak.verification_key();

//         let auth_policy = vec![
//             AuthPolicy::OnlyIbcRelay,
//             AuthPolicy::DestinationAllowList {
//                 allowed_destination_addresses: vec![
//                     spend_key
//                         .incoming_viewing_key()
//                         .payment_address(Default::default())
//                         .0,
//                 ],
//             },
//             AuthPolicy::PreAuthorization(PreAuthorizationPolicy::Ed25519 {
//                 required_signatures: 1,
//                 allowed_signers: vec![pvk],
//             }),
//         ];

//         let example = Config {
//             spend_key: spend_key.clone(),
//             auth_policy,
//         };

//         let encoded = toml::to_string_pretty(&example).unwrap();
//         println!("{encoded}");
//         let example2 = toml::from_str(&encoded).unwrap();
//         assert_eq!(example, example2);

//         println!("---");

//         let example3 = Config::from(spend_key);
//         println!("{}", toml::to_string_pretty(&example3).unwrap());
//     }
// }
