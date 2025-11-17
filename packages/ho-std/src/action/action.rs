use anyhow::anyhow;

use std::convert::{TryFrom, TryInto};

use crate::action::{ActionView, IsAction, TransactionPerspective};
use serde::{Deserialize, Serialize};

/// An action performed by a Penumbra transaction.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::Action", into = "pb::Action")]
#[allow(clippy::large_enum_variant)]
pub enum Action {}

impl EffectingData for Action {
    fn effect_hash(&self) -> EffectHash {
        match self {}
    }
}

impl Action {
    /// Create a tracing span to track execution related to this action.
    ///
    /// The `idx` parameter is the index of this action in the transaction.
    pub fn create_span(&self, idx: usize) -> tracing::Span {
        match self {}
    }

    /// Canonical action ordering according to protobuf definitions
    pub fn variant_index(&self) -> usize {
        match self {}
    }
}

impl IsAction for Action {
    fn balance_commitment(&self) -> balance::Commitment {
        match self {}
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        match self {}
    }
}

impl DomainType for Action {
    type Proto = pb::Action;
}

impl From<Action> for pb::Action {
    fn from(msg: Action) -> Self {
        match msg {}
    }
}

impl TryFrom<pb::Action> for Action {
    type Error = anyhow::Error;
    fn try_from(proto: pb::Action) -> anyhow::Result<Self, Self::Error> {
        if proto.action.is_none() {
            anyhow::bail!("missing action content");
        }
        match proto
            .action
            .ok_or_else(|| anyhow!("missing action in Action protobuf"))?
        {
            pb::action::Action::Output(inner) => Ok(Action::Output(inner.try_into()?)),
        }
    }
}
