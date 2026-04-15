use crate::commands::init_flow::find_git_root_from;
use crate::commands::output::OutputSink;
use crate::commands::ready::{
    apply_ready_user_directive, build_audit_setup, check_allow_docker,
    compute_ready_build_flag, create_ready_host_settings, dockerfile_matches_template,
    gather_ready_env_vars, is_legacy_layout, perform_legacy_migration,
    print_interactive_notice, print_summary, run_force_build, run_post_audit, run_pre_audit,
    ReadyContext, ReadyOptions, ReadySummary, StepStatus,
};
use crate::config::load_repo_config;
use crate::runtime::AgentRuntime;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;

// ─── Traits ───────────────────────────────────────────────────────────────────

/// All Q&A interactions the ready flow needs from the caller.
///
/// CLI implements these with `OutputSink` prompts; TUI implements them by
/// returning pre-collected answers from modal dialogs without blocking.
pub trait ReadyQa {
    /// Called when `Dockerfile.dev` is missing.
    ///
    /// Return `true` to auto-create the file from the project template and run
    /// the audit; return `false` to abort (caller handles the failure message).
    fn ask_create_dockerfile(&mut self) -> Result<bool>;

    /// Called when `Dockerfile.dev` exists and its content matches the unmodified
    /// project base template, indicating the audit would be useful.
    ///
    /// Return `true` to run the audit; return `false` to skip it.
    fn ask_run_audit_on_template(&mut self) -> Result<bool>;

    /// Called when the legacy single-file layout is detected.
    ///
    /// Return `true` to migrate to the modular layout (performs file operations
    /// inside `execute()` and forces a rebuild + refresh); return `false` to
    /// keep the existing layout and run in legacy mode.
    fn ask_migrate_legacy(&mut self, agent_name: &str) -> Result<bool>;
}

/// Container audit operation delegated to the caller.
///
/// CLI runs the audit synchronously (inherited stdio for interactive, captured
/// for non-interactive). TUI blocks inside its spawned background task using
/// captured output streamed line-by-line through the `OutputSink`.
pub trait ReadyAuditLauncher {
    fn run_audit(
        &self,
        ctx: &ReadyContext,
        host_settings: Option<&crate::runtime::HostSettings>,
        opts: &ReadyOptions,
        sink: &OutputSink,
    ) -> Result<()>;
}

// ─── Params ───────────────────────────────────────────────────────────────────

/// CLI flags forwarded to the ready flow.
pub struct ReadyParams {
    pub refresh: bool,
    pub build: bool,
    pub no_cache: bool,
    pub non_interactive: bool,
    pub allow_docker: bool,
}

// ─── CLI adapters ─────────────────────────────────────────────────────────────

/// Q&A implementation for CLI mode — uses `OutputSink` for prompts.
pub struct CliReadyQa {
    out: OutputSink,
}

impl CliReadyQa {
    pub fn new(out: OutputSink) -> Self {
        Self { out }
    }
}

impl ReadyQa for CliReadyQa {
    fn ask_create_dockerfile(&mut self) -> Result<bool> {
        self.out.println("\nNo Dockerfile.dev found in the project.");
        self.out.println(
            "Dockerfile.dev defines the container that runs your code agent securely.",
        );
        self.out
            .println("Without it, `amux ready` cannot build the dev container image.");
        Ok(self.out.ask_yes_no(
            "Create a Dockerfile.dev from the default template and run the agent audit?",
        ))
    }

    fn ask_run_audit_on_template(&mut self) -> Result<bool> {
        self.out.println(
            "\nYour Dockerfile.dev matches the default project template — the agent audit can",
        );
        self.out
            .println("scan your project and customize it for your specific toolchain.");
        Ok(self.out.ask_yes_no("Run the agent audit container now?"))
    }

    fn ask_migrate_legacy(&mut self, agent_name: &str) -> Result<bool> {
        self.out.println("");
        self.out
            .println("Detected legacy single-file Dockerfile.dev layout.");
        self.out.println(format!(
            "Would you like to migrate to the modular layout? \
             (agent tools move to .amux/Dockerfile.{})",
            agent_name
        ));
        self.out.println("");
        self.out.println("Migrating will:");
        self.out
            .println("  1. Back up Dockerfile.dev to Dockerfile.dev.bak");
        self.out.println(
            "  2. Recreate Dockerfile.dev with a minimal debian:bookworm-slim base",
        );
        self.out.println(format!(
            "  3. Write .amux/Dockerfile.{} using the agent template",
            agent_name
        ));
        self.out.println("  4. Build both images");
        self.out.println(
            "  5. Run the audit agent to restore project dependencies in Dockerfile.dev",
        );
        self.out.println("");
        Ok(self
            .out
            .ask_yes_no("Migrate to modular Dockerfile layout?"))
    }
}

/// Container launcher for CLI mode — runs the audit synchronously.
///
/// Interactive mode inherits stdio (takes over the terminal). Non-interactive
/// mode captures output and streams it line-by-line through the `OutputSink`.
pub struct CliReadyAuditLauncher {
    runtime: Arc<dyn AgentRuntime>,
}

impl CliReadyAuditLauncher {
    pub fn new(runtime: Arc<dyn AgentRuntime>) -> Self {
        Self { runtime }
    }
}

impl ReadyAuditLauncher for CliReadyAuditLauncher {
    fn run_audit(
        &self,
        ctx: &ReadyContext,
        host_settings: Option<&crate::runtime::HostSettings>,
        opts: &ReadyOptions,
        sink: &OutputSink,
    ) -> Result<()> {
        let audit = build_audit_setup(ctx, opts.non_interactive);
        let entrypoint_refs: Vec<&str> = audit.entrypoint.iter().map(String::as_str).collect();

        if opts.non_interactive {
            let (_cmd, output) = self
                .runtime
                .run_container_captured(
                    &audit.image_tag,
                    &ctx.mount_path,
                    &entrypoint_refs,
                    &ctx.env_vars,
                    host_settings,
                    opts.allow_docker,
                    None,
                    None,
                )
                .context("Dockerfile audit container failed")?;
            for line in output.lines() {
                sink.println(line);
            }
        } else {
            self.runtime
                .run_container(
                    &audit.image_tag,
                    &ctx.mount_path,
                    &entrypoint_refs,
                    &ctx.env_vars,
                    host_settings,
                    opts.allow_docker,
                    None,
                    None,
                )
                .context("Dockerfile audit container failed")?;
        }
        Ok(())
    }
}

// ─── execute() ────────────────────────────────────────────────────────────────

/// Run the full ready flow.
///
/// All business logic lives here; CLI and TUI differ only through their `qa`
/// and `launcher` implementations. `mount_path` is either the process CWD (CLI)
/// or the tab's working directory (TUI); the git root is derived from it.
pub async fn execute<Q, L>(
    params: ReadyParams,
    qa: &mut Q,
    launcher: &L,
    sink: &OutputSink,
    mount_path: PathBuf,
    runtime: Arc<dyn AgentRuntime>,
) -> Result<ReadySummary>
where
    Q: ReadyQa,
    L: ReadyAuditLauncher,
{
    // ── Pre-Q&A setup: resolve config to get agent name ───────────────────────
    let git_root = find_git_root_from(&mount_path).context("Not inside a Git repository")?;
    let config = load_repo_config(&git_root).unwrap_or_default();
    let agent_name = config.agent.as_deref().unwrap_or("claude").to_string();

    // ── Q&A: Dockerfile.dev creation / audit offer ────────────────────────────
    let mut effective_refresh = params.refresh;
    let auto_create_dockerfile;
    let dockerfile_path = git_root.join("Dockerfile.dev");

    if !dockerfile_path.exists() {
        if qa.ask_create_dockerfile()? {
            auto_create_dockerfile = true;
            // User accepted: create Dockerfile and run audit automatically.
            effective_refresh = true;
        } else {
            // User declined: proceed so run_pre_audit can record the failure.
            sink.println("Dockerfile.dev is required. Run `amux init` to set it up.");
            auto_create_dockerfile = false;
        }
    } else if !params.refresh {
        // Dockerfile.dev exists; offer audit when it still matches the default template.
        let content = std::fs::read_to_string(&dockerfile_path).unwrap_or_default();
        if dockerfile_matches_template(&content) && qa.ask_run_audit_on_template()? {
            effective_refresh = true;
        }
        auto_create_dockerfile = true; // file already exists, no creation needed
    } else {
        // --refresh was explicitly set and Dockerfile.dev exists.
        effective_refresh = true;
        auto_create_dockerfile = true;
    }

    // ── Q&A: legacy layout migration ─────────────────────────────────────────
    //
    // compute_ready_build_flag uses *effective_refresh* (which may have been
    // set to true above), but migration overrides effective_build afterward —
    // that is intentional: migration forces a project image rebuild regardless
    // of what the user passed on the command line.
    let mut effective_build = compute_ready_build_flag(effective_refresh, params.build);
    let legacy_mode = if is_legacy_layout(&git_root, &agent_name) {
        if qa.ask_migrate_legacy(&agent_name)? {
            let messages = perform_legacy_migration(&git_root)?;
            for msg in &messages {
                sink.println(msg.as_str());
            }
            // Force project image rebuild from the new minimal Dockerfile.dev (DIV-4).
            effective_build = true;
            effective_refresh = true;
            false // migrated — proceed with the new modular layout
        } else {
            sink.println("Keeping existing layout. Use the project image for this session.");
            sink.println(
                "DEPRECATION WARNING: Run `amux ready` to migrate to the modular layout.",
            );
            true // stay in legacy mode
        }
    } else {
        false // no legacy layout detected
    };

    // ── Gather credentials ────────────────────────────────────────────────────
    let env_vars = gather_ready_env_vars(&git_root, &agent_name)?;
    let mut host_settings = create_ready_host_settings(&agent_name);

    // ── Build ReadyOptions ────────────────────────────────────────────────────
    let opts = ReadyOptions {
        refresh: effective_refresh,
        build: effective_build,
        no_cache: params.no_cache,
        non_interactive: params.non_interactive,
        allow_docker: params.allow_docker,
        auto_create_dockerfile,
        legacy_mode,
    };

    // ── Phase 1: Pre-audit ────────────────────────────────────────────────────
    let mut summary = ReadySummary::default();
    let ctx = run_pre_audit(sink, mount_path, env_vars, &opts, &mut summary, &*runtime).await?;

    // ── Phase 2: Audit ────────────────────────────────────────────────────────
    if opts.refresh {
        // Apply the USER directive from the agent dockerfile now that run_pre_audit()
        // has written it. Must happen before the audit container is launched so that
        // settings files are mounted at the correct in-container home directory.
        if let Some(msg) = apply_ready_user_directive(host_settings.as_mut(), &ctx) {
            sink.println(msg);
        }

        if !opts.non_interactive {
            print_interactive_notice(sink, &ctx.agent_name);
        }

        check_allow_docker(sink, opts.allow_docker, &*runtime)?;

        launcher.run_audit(&ctx, host_settings.as_ref(), &opts, sink)?;
        summary.refresh = StepStatus::Ok("completed".into());

        // ── Phase 3: Post-audit rebuild ───────────────────────────────────────
        run_post_audit(sink, &ctx, &opts, &mut summary, &*runtime).await?;
    } else {
        sink.println("Skipping Dockerfile audit (use --refresh to run it).");
        summary.refresh = StepStatus::Skipped("use --refresh to run".into());
        if opts.build {
            run_force_build(sink, &ctx, &opts, &mut summary, &*runtime).await?;
        } else {
            summary.image_rebuild = StepStatus::Skipped("no refresh".into());
        }
    }

    // ── Summary and tips ──────────────────────────────────────────────────────
    print_summary(sink, runtime.name(), &summary);

    if !opts.refresh {
        sink.println("");
        sink.println("Tip: use `amux ready --refresh` to run the Dockerfile audit agent.");
    }
    if matches!(summary.aspec_folder, StepStatus::Failed(_)) {
        sink.println("");
        sink.println("Tip: run `amux init --aspec` to add an aspec folder to this project.");
    }
    if matches!(summary.work_items_config, StepStatus::Warn(_)) {
        sink.println("");
        sink.println(
            "Tip: run `amux config set work_items.dir <path>` to configure a work items directory.",
        );
    }

    sink.println("");
    sink.println("amux is ready.");

    Ok(summary)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    // ── Minimal mock stubs ────────────────────────────────────────────────────

    struct MockReadyQa {
        create_dockerfile: bool,
        run_audit_on_template: bool,
        migrate_legacy: bool,
    }

    impl ReadyQa for MockReadyQa {
        fn ask_create_dockerfile(&mut self) -> Result<bool> {
            Ok(self.create_dockerfile)
        }
        fn ask_run_audit_on_template(&mut self) -> Result<bool> {
            Ok(self.run_audit_on_template)
        }
        fn ask_migrate_legacy(&mut self, _agent_name: &str) -> Result<bool> {
            Ok(self.migrate_legacy)
        }
    }

    struct MockReadyAuditLauncher {
        should_fail: bool,
    }

    impl ReadyAuditLauncher for MockReadyAuditLauncher {
        fn run_audit(
            &self,
            _ctx: &ReadyContext,
            _host_settings: Option<&crate::runtime::HostSettings>,
            _opts: &ReadyOptions,
            _sink: &OutputSink,
        ) -> Result<()> {
            if self.should_fail {
                anyhow::bail!("mock audit failure");
            }
            Ok(())
        }
    }

    // ── CliReadyQa ────────────────────────────────────────────────────────────

    #[test]
    fn cli_ready_qa_ask_create_dockerfile_yes() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["y"]);
        let mut qa = CliReadyQa::new(sink);
        assert!(qa.ask_create_dockerfile().unwrap());
    }

    #[test]
    fn cli_ready_qa_ask_create_dockerfile_no() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["n"]);
        let mut qa = CliReadyQa::new(sink);
        assert!(!qa.ask_create_dockerfile().unwrap());
    }

    #[test]
    fn cli_ready_qa_ask_run_audit_on_template_yes() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["y"]);
        let mut qa = CliReadyQa::new(sink);
        assert!(qa.ask_run_audit_on_template().unwrap());
    }

    #[test]
    fn cli_ready_qa_ask_run_audit_on_template_no() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["n"]);
        let mut qa = CliReadyQa::new(sink);
        assert!(!qa.ask_run_audit_on_template().unwrap());
    }

    #[test]
    fn cli_ready_qa_ask_migrate_legacy_yes() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["y"]);
        let mut qa = CliReadyQa::new(sink);
        assert!(qa.ask_migrate_legacy("claude").unwrap());
    }

    #[test]
    fn cli_ready_qa_ask_migrate_legacy_no() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["n"]);
        let mut qa = CliReadyQa::new(sink);
        assert!(!qa.ask_migrate_legacy("claude").unwrap());
    }

    #[test]
    fn cli_ready_qa_ask_migrate_legacy_prints_agent_name() {
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["n"]);
        let mut qa = CliReadyQa::new(sink);
        let _ = qa.ask_migrate_legacy("codex");
        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let all = messages.join("\n");
        assert!(
            all.contains("codex"),
            "Expected agent name 'codex' in migration dialog output"
        );
    }

    // ── execute() — early failure path (Docker not running) ───────────────────

    #[tokio::test]
    async fn execute_fails_gracefully_without_docker() {
        let runtime = crate::runtime::DockerRuntime::new();
        if runtime.is_available() {
            return; // skip when Docker is running
        }
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        let cwd = std::env::current_dir().unwrap();
        let git_root = match find_git_root_from(&cwd) {
            Some(r) => r,
            None => return,
        };
        if !git_root.join("Dockerfile.dev").exists() {
            return;
        }

        let mut qa = MockReadyQa {
            create_dockerfile: true,
            run_audit_on_template: false,
            migrate_legacy: false,
        };
        let launcher = MockReadyAuditLauncher { should_fail: false };
        let params = ReadyParams {
            refresh: false,
            build: false,
            no_cache: false,
            non_interactive: false,
            allow_docker: false,
        };
        let result = execute(
            params,
            &mut qa,
            &launcher,
            &sink,
            git_root,
            Arc::new(runtime),
        )
        .await;
        assert!(result.is_err());
        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("FAILED") || m.contains("Checking")),
            "Expected Docker check message. Got: {:?}",
            messages
        );
    }

    // ── execute() — routes output through sink ────────────────────────────────

    #[tokio::test]
    async fn execute_routes_output_through_sink() {
        let runtime = crate::runtime::DockerRuntime::new();
        if !runtime.is_available() {
            return;
        }
        let cwd = std::env::current_dir().unwrap();
        let git_root = match find_git_root_from(&cwd) {
            Some(r) => r,
            None => return,
        };
        if !git_root.join("Dockerfile.dev").exists() {
            return;
        }

        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        let mut qa = MockReadyQa {
            create_dockerfile: true,
            run_audit_on_template: false,
            migrate_legacy: false,
        };
        let launcher = MockReadyAuditLauncher { should_fail: false };
        let params = ReadyParams {
            refresh: false,
            build: false,
            no_cache: false,
            non_interactive: false,
            allow_docker: false,
        };
        let _ = execute(
            params,
            &mut qa,
            &launcher,
            &sink,
            git_root,
            Arc::new(runtime),
        )
        .await;

        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let has_runtime_check = messages
            .iter()
            .any(|m| m.contains("Checking") && m.contains("runtime"));
        assert!(
            has_runtime_check,
            "Expected runtime check message. Got: {:?}",
            messages
        );
    }

    // ── ReadyParams defaults ──────────────────────────────────────────────────

    #[test]
    fn ready_params_no_refresh_by_default() {
        let params = ReadyParams {
            refresh: false,
            build: false,
            no_cache: false,
            non_interactive: false,
            allow_docker: false,
        };
        assert!(!params.refresh);
        assert!(!params.build);
        assert!(!params.no_cache);
        assert!(!params.non_interactive);
        assert!(!params.allow_docker);
    }

    // ── MockReadyQa ───────────────────────────────────────────────────────────

    #[test]
    fn mock_ready_qa_returns_preset_answers() {
        let mut qa = MockReadyQa {
            create_dockerfile: true,
            run_audit_on_template: false,
            migrate_legacy: true,
        };
        assert!(qa.ask_create_dockerfile().unwrap());
        assert!(!qa.ask_run_audit_on_template().unwrap());
        assert!(qa.ask_migrate_legacy("claude").unwrap());
    }

    // ── MockReadyAuditLauncher ────────────────────────────────────────────────

    #[test]
    fn mock_audit_launcher_ok() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        let ctx = ReadyContext {
            image_tag: "test:latest".into(),
            dockerfile_str: "/tmp/Dockerfile.dev".into(),
            git_root_str: "/tmp".into(),
            mount_path: "/tmp".into(),
            agent_name: "claude".into(),
            env_vars: vec![],
            agent_image_tag: None,
            agent_dockerfile_str: None,
        };
        let opts = ReadyOptions { ..Default::default() };
        let launcher = MockReadyAuditLauncher { should_fail: false };
        assert!(launcher.run_audit(&ctx, None, &opts, &sink).is_ok());
    }

    #[test]
    fn mock_audit_launcher_fail() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        let ctx = ReadyContext {
            image_tag: "test:latest".into(),
            dockerfile_str: "/tmp/Dockerfile.dev".into(),
            git_root_str: "/tmp".into(),
            mount_path: "/tmp".into(),
            agent_name: "claude".into(),
            env_vars: vec![],
            agent_image_tag: None,
            agent_dockerfile_str: None,
        };
        let opts = ReadyOptions { ..Default::default() };
        let launcher = MockReadyAuditLauncher { should_fail: true };
        assert!(launcher.run_audit(&ctx, None, &opts, &sink).is_err());
    }
}
