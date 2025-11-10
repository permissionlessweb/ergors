use anyhow::Result;
use camino::Utf8Path;

#[derive(Debug, clap::Parser)]
pub struct AuthCmd {
    #[clap(subcommand)]
    pub subcmd: AuthTopSubCmd,
    /// base-64 encoded json of authentication structure
    #[clap(display_order = 200)]
    pub auth: String,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum AuthTopSubCmd {
    /// register a user key pair for permissioned api access
    #[clap(display_order = 100)]
    Register {},
    /// revoke a user key pair for permissioned api access
    #[clap(display_order = 200)]
    Revoke {},
}
impl AuthCmd {
    pub fn exec(&self, home_dir: &Utf8Path) -> Result<()> {
        //
        match self.subcmd.clone() {
            AuthTopSubCmd::Register {} => {
                // check fo existing register,
            }
            AuthTopSubCmd::Revoke {} => {
                // check if exists, remove if so
            }
        };
        Ok(())
    }
}
