//! `HeadlessCommandFrontend` impl for the CLI.

use async_trait::async_trait;

use crate::command::commands::headless::HeadlessCommandFrontend;
use crate::command::error::CommandError;
use crate::frontend::cli::command_frontend::CliFrontend;
use crate::frontend::headless::HeadlessServeConfig;

#[async_trait]
impl HeadlessCommandFrontend for CliFrontend {
    async fn serve_until_shutdown(
        &mut self,
        config: HeadlessServeConfig,
    ) -> Result<(), CommandError> {
        crate::frontend::headless::serve(config).await
    }
}
