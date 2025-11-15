use anyhow::Result;
use camino::Utf8Path;

#[derive(Debug, clap::Parser)]
pub struct CallCmd {
    #[clap(subcommand)]
    pub subcmd: CallTopSubCmd,
    /// base-64 encoded json of authentication structure
    #[clap(display_order = 200)]
    pub auth: String,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum CallTopSubCmd {
    /// register a user key pair for permissioned api access
    #[clap(display_order = 100)]
    Prompt {},
}
impl CallTopSubCmd {
    pub fn exec(&self, home_dir: &Utf8Path) -> Result<()> {
        //
        match self {
            CallTopSubCmd::Prompt {} => {
                // form prompt request,
            }
        };
        Ok(())
    }
}
