use serde::Serialize;

use crate::Action;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pbt::ActionView", into = "pbt::ActionView")]
#[allow(clippy::large_enum_variant)]
pub enum ActionView {
    // Action types with encrypted contents
    Prompt(SpendView),
    // Orchestrate(SpendView),
    // Authorize(SpendView),
}

impl DomainType for ActionView {
    type Proto = pbt::ActionView;
}

impl TryFrom<pbt::ActionView> for ActionView {
    type Error = anyhow::Error;

    fn try_from(v: pbt::ActionView) -> Result<Self, Self::Error> {
        use pbt::action_view::ActionView as AV;
        Ok(
            match v
                .action_view
                .ok_or_else(|| anyhow::anyhow!("missing action_view"))?
            {
                AV::Prompt(x) => ActionView::Prompt(x.try_into()?),
            },
        )
    }
}

impl From<ActionView> for pbt::ActionView {
    fn from(v: ActionView) -> Self {
        use pbt::action_view::ActionView as AV;
        Self {
            action_view: Some(match v {
                ActionView::Prompt(x) => AV::Prompt(x.into()),
            }),
        }
    }
}

impl From<ActionView> for Action {
    fn from(action_view: ActionView) -> Action {
        match action_view {
            ActionView::Swap(x) => Action::Swap(x.into()),
        }
    }
}
