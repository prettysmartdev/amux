use crate::cli::Agent;
use crate::commands::init_flow::{find_git_root_from, CliContainerLauncher, CliInitQa, InitParams, execute};
use crate::commands::output::OutputSink;
use crate::runtime::AgentRuntime;
use anyhow::{Context, Result};
use std::sync::Arc;

// ─── CLI entry point ──────────────────────────────────────────────────────────

/// Command-mode entry point: creates CLI adapters and runs the init flow.
pub async fn run(agent: Agent, aspec: bool, runtime: Arc<dyn AgentRuntime>) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let git_root = find_git_root_from(&cwd).context("Not inside a Git repository")?;

    let sink = OutputSink::Stdout;
    let mut qa = CliInitQa::new(&git_root, sink.clone());
    let launcher = CliContainerLauncher::new(runtime.clone());
    let params = InitParams { agent, aspec, git_root };
    execute(params, &mut qa, &launcher, &sink, runtime).await?;
    Ok(())
}

// ─── Utilities ────────────────────────────────────────────────────────────────

/// Prompt the user with a yes/no question via stdin. Returns true for yes.
///
/// Used by `ready` and other CLI commands that need a quick yes/no without an
/// `OutputSink`.
pub fn ask_yes_no_stdin(prompt: &str) -> bool {
    use std::io::{BufRead, Write};
    print!("{} [y/N]: ", prompt);
    let _ = std::io::stdout().flush();
    let stdin = std::io::stdin();
    let answer = stdin
        .lock()
        .lines()
        .next()
        .unwrap_or(Ok(String::new()))
        .unwrap_or_default();
    matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_yes_no_stdin_is_callable() {
        // Compile-time check that the function exists with the expected signature.
        let _f: fn(&str) -> bool = ask_yes_no_stdin;
    }
}
