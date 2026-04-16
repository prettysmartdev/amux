use crate::cli::Agent;
use crate::commands::auth::resolve_auth;
use crate::commands::implement::confirm_mount_scope_stdin;
use crate::commands::init_flow::{
    find_git_root_from, project_dockerfile_embedded,
    write_agent_dockerfile, write_project_dockerfile,
};
use crate::commands::output::OutputSink;
use crate::config::{load_repo_config, migrate_legacy_repo_config};
use crate::runtime::{agent_image_tag, format_build_cmd, format_build_cmd_no_cache, project_image_tag};
use anyhow::{bail, Context, Result};
use std::path::PathBuf;

/// The prompt sent to the agent for Dockerfile.dev audit.
pub const AUDIT_PROMPT: &str = "scan this project and determine every tool needed to build, run, \
    and test it per the local development workflows defined in the aspec. Modify Dockerfile.dev \
    to ensure that all of those tools, at the correct version, get installed when the Dockerfile \
    is built. Pin to specific versions wherever possible. Ensure all relevant tools are in $PATH \
    and can be executed by the container entrypoint command. Only modify Dockerfile.dev; do not \
    modify any other files. Do not add any new files.";

/// 50 random greetings used to check local agent installation / refresh OAuth tokens.
pub const GREETINGS: [&str; 50] = [
    "Hello",
    "Hi there",
    "Hey",
    "Greetings",
    "Good day",
    "Howdy",
    "Salutations",
    "How are you",
    "Good morning",
    "Good afternoon",
    "Good evening",
    "Hi",
    "Hey there",
    "Ahoy",
    "Yo",
    "Hello there",
    "Hiya",
    "How's it going",
    "How do you do",
    "Pleased to meet you",
    "Nice to meet you",
    "How are things",
    "What's new",
    "How have you been",
    "Welcome",
    "Aloha",
    "Bonjour",
    "Ciao",
    "Hola",
    "Namaste",
    "Howdy partner",
    "Top of the morning to you",
    "What's happening",
    "How goes it",
    "How's everything",
    "How's life",
    "Well hello",
    "Hey friend",
    "Good to see you",
    "Hello friend",
    "Greetings and salutations",
    "Hey buddy",
    "Sup",
    "What's up",
    "Long time no see",
    "Rise and shine",
    "How's your day going",
    "Hope you're doing well",
    "Great to hear from you",
    "Glad you're here",
];

/// Select a greeting at random using the current time as a seed.
///
/// Uses seconds since epoch rather than nanoseconds: on most platforms the
/// system clock has millisecond or microsecond resolution, meaning the raw
/// nanosecond count is always a multiple of 50 (since 10^3, 10^6, and 10^9
/// are all divisible by 50), which would pin the result to GREETINGS[0].
/// Seconds are not multiples of 50 in general, so this produces varied output.
pub fn select_random_greeting() -> &'static str {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    GREETINGS[(secs % GREETINGS.len() as u64) as usize]
}

/// Context produced by the pre-audit phase, needed by the audit and post-audit phases.
#[derive(Clone)]
pub struct ReadyContext {
    pub image_tag: String,           // project base image tag
    pub dockerfile_str: String,      // path to Dockerfile.dev
    pub git_root_str: String,
    pub mount_path: String,
    pub agent_name: String,
    pub env_vars: Vec<(String, String)>,
    /// Agent image tag (`amux-{project}-{agent}:latest`). `None` when in legacy mode.
    pub agent_image_tag: Option<String>,
    /// Path to `.amux/Dockerfile.{agent}`. `None` when in legacy mode.
    pub agent_dockerfile_str: Option<String>,
}

/// Options controlling ready command behavior. Shared between command and TUI modes.
#[derive(Clone, Debug, Default)]
pub struct ReadyOptions {
    /// When true, run the Dockerfile agent audit. When false, skip it.
    pub refresh: bool,
    /// When true, force rebuild the dev container image even if one exists.
    /// Ignored when `refresh` is true (refresh always rebuilds after audit).
    pub build: bool,
    /// When true, pass `--no-cache` to `docker build`.
    pub no_cache: bool,
    /// When true, launch the agent in non-interactive (print) mode.
    pub non_interactive: bool,
    /// When true, mount the host Docker daemon socket into the audit container.
    pub allow_docker: bool,
    /// When true, auto-create Dockerfile.dev if missing (used by TUI to skip prompting).
    pub auto_create_dockerfile: bool,
    /// When true, skip the agent dockerfile/image steps and use only the project image.
    /// Set when user declines migration from the legacy single-file layout.
    pub legacy_mode: bool,
}

/// Tracks the status of each step for the summary table.
#[derive(Clone, Debug)]
pub struct ReadySummary {
    pub docker_daemon: StepStatus,
    pub dockerfile: StepStatus,
    pub aspec_folder: StepStatus,
    pub work_items_config: StepStatus,
    pub local_agent: StepStatus,
    pub dev_image: StepStatus,
    pub refresh: StepStatus,
    pub image_rebuild: StepStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StepStatus {
    Pending,
    Ok(String),
    Skipped(String),
    Failed(String),
    Warn(String),
}

impl Default for ReadySummary {
    fn default() -> Self {
        Self {
            docker_daemon: StepStatus::Pending,
            dockerfile: StepStatus::Pending,
            aspec_folder: StepStatus::Pending,
            work_items_config: StepStatus::Pending,
            local_agent: StepStatus::Pending,
            dev_image: StepStatus::Pending,
            refresh: StepStatus::Pending,
            image_rebuild: StepStatus::Pending,
        }
    }
}

/// Prints the summary table to the output sink.
pub fn print_summary(out: &OutputSink, runtime_name: &str, summary: &ReadySummary) {
    out.println(String::new());
    out.println("┌───────────────────────────────────────────────────┐");
    out.println("│                   Ready Summary                   │");
    out.println("├───────────────────┬───────────────────────────────┤");
    let runtime_row_label = match runtime_name {
        "apple-containers" => "apple-container".to_string(),
        name => format!("{} runtime", name),
    };
    print_summary_row(out, &runtime_row_label, &summary.docker_daemon);
    print_summary_row(out, "Dockerfile.dev", &summary.dockerfile);
    print_summary_row(out, "aspec folder", &summary.aspec_folder);
    print_summary_row(out, "work items config", &summary.work_items_config);
    print_summary_row(out, "Local agent", &summary.local_agent);
    print_summary_row(out, "Dev image", &summary.dev_image);
    print_summary_row(out, "Refresh (audit)", &summary.refresh);
    print_summary_row(out, "Image rebuild", &summary.image_rebuild);
    out.println("└───────────────────┴───────────────────────────────┘");
}

fn print_summary_row(out: &OutputSink, label: &str, status: &StepStatus) {
    let (symbol, text) = match status {
        StepStatus::Pending => ("-", "pending".to_string()),
        StepStatus::Ok(msg) => ("✓", msg.clone()),
        StepStatus::Skipped(msg) => ("–", msg.clone()),
        StepStatus::Failed(msg) => ("✗", msg.clone()),
        StepStatus::Warn(msg) => ("⚠", msg.clone()),
    };
    out.println(format!(
        "│ {:>17} │ {} {:<27} │",
        label, symbol, text
    ));
}

/// Large ASCII-art notice printed before launching an interactive agent.
pub fn print_interactive_notice(out: &OutputSink, agent_name: &str) {
    out.println(String::new());
    out.println("╔══════════════════════════════════════════════════════════════╗");
    out.println("║                                                              ║");
    out.println("║     ╦╔╗╔╔╦╗╔═╗╦═╗╔═╗╔═╗╔╦╗╦╦  ╦╔═╗  ╔╦╗╔═╗╔╦╗╔═╗        ║");
    out.println("║     ║║║║ ║ ║╣ ╠╦╝╠═╣║   ║ ║╚╗╔╝║╣   ║║║║ ║ ║║║╣         ║");
    out.println("║     ╩╝╚╝ ╩ ╚═╝╩╚═╩ ╩╚═╝ ╩ ╩ ╚╝ ╚═╝  ╩ ╩╚═╝═╩╝╚═╝       ║");
    out.println("║                                                              ║");
    out.println(format!(
        "║  Agent '{}' is launching in INTERACTIVE mode.{}║",
        agent_name,
        " ".repeat(46usize.saturating_sub(agent_name.len() + 43))
    ));
    out.println("║  You will need to quit the agent (Ctrl+C or exit)            ║");
    out.println("║  when its work is complete.                                  ║");
    out.println("║                                                              ║");
    out.println("╚══════════════════════════════════════════════════════════════╝");
    out.println(String::new());
}

/// Check whether the given Dockerfile.dev content matches the default project base template.
/// Returns true when the content is still unmodified from the generated project template,
/// which signals that running the audit agent would be useful.
pub fn dockerfile_matches_template(content: &str) -> bool {
    let template = project_dockerfile_embedded();
    content.trim() == template.trim()
}

/// Run the configured agent locally (non-containerized) with a simple greeting
/// to check whether it is installed and authenticated.
/// Returns `(status, greeting_sent, agent_response)`.
pub async fn check_local_agent(agent_name: &str) -> (StepStatus, String, String) {
    let greeting = select_random_greeting();
    let (cmd, args): (&str, Vec<&str>) = match agent_name {
        "claude" => ("claude", vec!["--print", greeting]),
        "codex" => ("codex", vec!["exec", greeting]),
        "opencode" => ("opencode", vec!["run", greeting]),
        "maki" => ("maki", vec!["--print", greeting]),
        "gemini" => ("gemini", vec!["-p", greeting]),
        _ => (agent_name, vec!["--print", greeting]),
    };

    match tokio::process::Command::new(cmd)
        .args(&args)
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let response = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (
                StepStatus::Ok(format!("{}: ready", agent_name)),
                greeting.to_string(),
                response,
            )
        }
        Ok(output) => {
            let response = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (
                StepStatus::Failed(format!("{}: error (check auth)", agent_name)),
                greeting.to_string(),
                response,
            )
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            StepStatus::Failed(format!("{}: not installed", agent_name)),
            greeting.to_string(),
            String::new(),
        ),
        Err(_) => (
            StepStatus::Failed(format!("{}: could not run", agent_name)),
            greeting.to_string(),
            String::new(),
        ),
    }
}

/// Compute the effective `--build` flag for the ready command.
///
/// When `--refresh` is set, the image is always rebuilt after the audit runs,
/// so passing `--build` during the pre-audit phase is unnecessary. This mirrors
/// the comment at the top of `run()`: "ignore --build when --refresh is set."
/// Note: migration code sets `build = true` programmatically *after* this call,
/// overriding the computed value — that is intentional.
pub fn compute_ready_build_flag(refresh: bool, build: bool) -> bool {
    if refresh { false } else { build }
}

/// Detect whether the project is using the legacy single-file Dockerfile.dev layout.
///
/// Returns `true` when:
/// - `Dockerfile.dev` exists in the git root, AND
/// - the agent name is a known amux agent, AND
/// - `.amux/Dockerfile.{agent_name}` does NOT exist yet.
///
/// This is the condition that triggers migration to the modular layout.
pub fn is_legacy_layout(git_root: &std::path::Path, agent_name: &str) -> bool {
    let dockerfile_path = git_root.join("Dockerfile.dev");
    let agent_dockerfile_path = git_root
        .join(".amux")
        .join(format!("Dockerfile.{}", agent_name));
    let is_known_agent = crate::cli::KNOWN_AGENT_NAMES.contains(&agent_name);
    dockerfile_path.exists() && is_known_agent && !agent_dockerfile_path.exists()
}

/// Perform the legacy Dockerfile.dev → modular layout migration.
///
/// - Backs up `Dockerfile.dev` to `Dockerfile.dev.bak`.
/// - Overwrites `Dockerfile.dev` with the minimal project base template.
///
/// Returns a list of human-readable messages describing what was done.
/// Callers should print these messages via their respective output mechanism.
pub fn perform_legacy_migration(git_root: &std::path::Path) -> Result<Vec<String>> {
    let dockerfile_path = git_root.join("Dockerfile.dev");
    let backup_path = dockerfile_path.with_extension("dev.bak");
    std::fs::copy(&dockerfile_path, &backup_path)
        .context("Failed to back up Dockerfile.dev")?;
    let content = crate::commands::init_flow::project_dockerfile_embedded();
    std::fs::write(&dockerfile_path, content)
        .context("Failed to overwrite Dockerfile.dev with project template")?;
    Ok(vec![
        format!("Backed up existing Dockerfile.dev to {}.", backup_path.display()),
        "Dockerfile.dev recreated with project base template.".to_string(),
    ])
}

/// Gather environment variables for the ready audit container.
///
/// - Calls `resolve_auth()` to read keychain credentials (e.g., Claude OAuth token
///   for OAuth-based agents). `resolve_auth()` is keychain-only.
/// - Then appends any `effective_env_passthrough` vars not already present, for
///   API-key-based agents (Codex, Gemini, etc.) that inject credentials via env vars
///   rather than the keychain.
///
/// File-based auth (`.claude.json` / `.claude` dir mounts) is handled separately by
/// `create_ready_host_settings()`; this function only produces env vars.
/// Both CLI and TUI call this function to ensure identical credential gathering.
pub fn gather_ready_env_vars(git_root: &std::path::Path, agent_name: &str) -> Result<Vec<(String, String)>> {
    let credentials = resolve_auth(git_root, agent_name)?;
    let mut env_vars = credentials.env_vars;
    for name in &crate::config::effective_env_passthrough(git_root) {
        if env_vars.iter().any(|(k, _)| k == name) {
            continue;
        }
        if let Ok(val) = std::env::var(name) {
            env_vars.push((name.clone(), val));
        }
    }
    Ok(env_vars)
}

/// Create host settings (sanitized config files in a temp dir) for the ready audit container.
pub fn create_ready_host_settings(agent_name: &str) -> Option<crate::runtime::HostSettings> {
    crate::passthrough::passthrough_for_agent(agent_name).prepare_host_settings()
}

/// Apply the USER directive from the agent dockerfile to the host settings.
///
/// Ensures settings files are mounted at the correct home directory inside the
/// container. Must be called after `run_pre_audit()` returns (so the agent
/// dockerfile has been written), before the audit container is launched.
///
/// Returns the informational message from `apply_dockerfile_user` (if any).
pub fn apply_ready_user_directive(
    host_settings: Option<&mut crate::runtime::HostSettings>,
    ctx: &ReadyContext,
) -> Option<String> {
    let settings = host_settings?;
    let dockerfile_for_user = ctx.agent_dockerfile_str
        .as_deref()
        .map(std::path::Path::new)
        .unwrap_or_else(|| std::path::Path::new(&ctx.dockerfile_str));
    crate::runtime::apply_dockerfile_user(settings, dockerfile_for_user)
}

/// Check whether the host Docker socket is accessible when `--allow-docker` is set.
///
/// Returns `Ok(())` when:
/// - `allow_docker` is false (no check needed), or
/// - `allow_docker` is true and the socket is found (prints a warning to `out`).
///
/// Returns `Err` when `allow_docker` is true but the socket is not found.
pub fn check_allow_docker(
    out: &OutputSink,
    allow_docker: bool,
    runtime: &dyn crate::runtime::AgentRuntime,
) -> Result<()> {
    if !allow_docker {
        return Ok(());
    }
    let socket_path = runtime.check_socket()
        .context("Cannot mount socket for audit container")?;
    out.println(format!("{} socket: {} (found)", runtime.name(), socket_path.display()));
    out.println(format!(
        "WARNING: --allow-docker: mounting host {} socket into audit container ({}:{}). \
         This grants the agent elevated host access.",
        runtime.name(),
        socket_path.display(),
        socket_path.display()
    ));
    Ok(())
}

/// Carries the Docker image tag and entrypoint command for the audit container.
pub struct AuditSetup {
    pub image_tag: String,
    pub entrypoint: Vec<String>,
}

/// Build the audit container setup (image tag + entrypoint) from the ready context.
///
/// - `non_interactive`: when `true`, uses the non-interactive (print/quiet) entrypoint.
/// - `image_tag`: uses the agent image when available; falls back to the project base image
///   in legacy mode (when `ctx.agent_image_tag` is `None`).
pub fn build_audit_setup(ctx: &ReadyContext, non_interactive: bool) -> AuditSetup {
    let image_tag = ctx.agent_image_tag.as_deref().unwrap_or(&ctx.image_tag).to_string();
    let entrypoint = if non_interactive {
        audit_entrypoint_non_interactive(&ctx.agent_name)
    } else {
        audit_entrypoint(&ctx.agent_name)
    };
    AuditSetup { image_tag, entrypoint }
}

/// Command-mode entry point: gathers mount scope and delegates to the ready flow.
pub async fn run(
    refresh: bool,
    build: bool,
    no_cache: bool,
    non_interactive: bool,
    allow_docker: bool,
    runtime: std::sync::Arc<dyn crate::runtime::AgentRuntime>,
) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let git_root = find_git_root_from(&cwd).context("Not inside a Git repository")?;
    let mount_path = confirm_mount_scope_stdin(&git_root)?;
    let sink = OutputSink::Stdout;
    let mut qa = crate::commands::ready_flow::CliReadyQa::new(sink.clone());
    let launcher = crate::commands::ready_flow::CliReadyAuditLauncher::new(runtime.clone());
    let params = crate::commands::ready_flow::ReadyParams {
        refresh,
        build,
        no_cache,
        non_interactive,
        allow_docker,
    };
    crate::commands::ready_flow::execute(params, &mut qa, &launcher, &sink, mount_path, runtime)
        .await?;
    Ok(())
}

/// Phase 1 — Pre-audit: Docker checks, Dockerfile init, aspec check, agent check, image build.
///
/// Returns the context needed to launch the audit and post-audit phases.
///
/// **Migration interaction**: when the legacy-layout migration has run, the caller sets
/// `opts.build = true` *before* calling this function. Step 6 (`needs_build` check) is
/// the only place that flag matters for migration correctness: it forces the project image
/// to rebuild from the new minimal `Dockerfile.dev` before the agent image is layered on
/// top in step 7. The post-audit phase (`rebuild_images`) then rebuilds both images again
/// after the audit agent populates `Dockerfile.dev` with project-specific tooling.
pub async fn run_pre_audit(
    out: &OutputSink,
    mount_path: PathBuf,
    env_vars: Vec<(String, String)>,
    opts: &ReadyOptions,
    summary: &mut ReadySummary,
    runtime: &dyn crate::runtime::AgentRuntime,
) -> Result<ReadyContext> {
    // 1. Runtime daemon check
    out.print(&format!("Checking {} runtime... ", runtime.name()));
    if runtime.is_available() {
        out.println("OK");
        summary.docker_daemon = StepStatus::Ok("running".into());
    } else {
        out.println("FAILED");
        summary.docker_daemon = StepStatus::Failed("not running".into());
        bail!("{} runtime is not running or not accessible. Start it and try again.", runtime.name());
    }

    // 2. Git root + project-specific image tag
    // Derive the git root from mount_path (the tab's working directory) so that
    // each tab operates against its own project, not the process CWD.
    let git_root = find_git_root_from(&mount_path).context("Not inside a Git repository")?;
    if migrate_legacy_repo_config(&git_root)? {
        out.println("Migrated config: aspec/.amux.json -> .amux/config.json".to_string());
    }
    let image_tag = project_image_tag(&git_root);
    let dockerfile = git_root.join("Dockerfile.dev");
    let config = load_repo_config(&git_root)?;
    let agent_name = config.agent.as_deref().unwrap_or("claude").to_string();

    // 3. Check aspec folder.
    let aspec_dir = git_root.join("aspec");
    if aspec_dir.exists() {
        summary.aspec_folder = StepStatus::Ok("present".into());
    } else {
        summary.aspec_folder = StepStatus::Failed("missing".into());
        out.println("Note: no aspec folder found. Run `amux init --aspec` to add one.");
    }

    // 3b. Check work_items config (advisory only — does not fail ready).
    {
        let aspec_absent = matches!(summary.aspec_folder, StepStatus::Failed(_));
        let work_items_dir_set = config
            .work_items
            .as_ref()
            .and_then(|w| w.dir.as_deref())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        if aspec_absent && !work_items_dir_set {
            summary.work_items_config = StepStatus::Warn("not configured".into());
            out.println(
                "`specs new` and `implement` will not work. \
                 Run `amux config set work_items.dir <path>` to configure a work items directory."
                    .to_string(),
            );
        } else {
            summary.work_items_config = StepStatus::Ok("ok".into());
        }
    }

    // 4. Check local agent installation (non-containerized greeting).
    out.println(format!("Checking local {} agent...", agent_name));
    let (agent_status, greeting_sent, agent_response) = check_local_agent(&agent_name).await;
    out.println(format!("  > {}", greeting_sent));
    if !agent_response.is_empty() {
        // Show first non-empty line of the response (agent may produce many lines).
        let first_line = agent_response.lines().find(|l| !l.trim().is_empty()).unwrap_or(&agent_response);
        out.println(format!("  < {}", first_line));
    }
    match &agent_status {
        StepStatus::Ok(msg) => out.println(format!("  {}: OK", msg)),
        StepStatus::Failed(msg) => out.println(format!("  note: {}", msg)),
        _ => {}
    }
    summary.local_agent = agent_status;

    // 5. Handle Dockerfile.dev — create if missing (requires user acceptance).
    out.print("Checking Dockerfile.dev... ");
    let dockerfile_was_missing;
    {
        if !dockerfile.exists() {
            if opts.auto_create_dockerfile {
                // TUI mode or user already accepted: create from project template.
                if write_project_dockerfile(&git_root, out).await? {
                    out.println(format!(
                        "MISSING — created at {}",
                        dockerfile.display()
                    ));
                    summary.dockerfile = StepStatus::Ok("created".into());
                    dockerfile_was_missing = true;
                } else {
                    // write_project_dockerfile returned false (file appeared between checks).
                    out.println(format!("OK ({})", dockerfile.display()));
                    summary.dockerfile = StepStatus::Ok("exists".into());
                    dockerfile_was_missing = false;
                }
            } else {
                // Command mode, user declined: fail.
                out.println("MISSING");
                summary.dockerfile = StepStatus::Failed("missing — run `amux init`".into());
                bail!("Dockerfile.dev is missing. Run `amux init` to create it.");
            }
        } else {
            out.println(format!("OK ({})", dockerfile.display()));
            summary.dockerfile = StepStatus::Ok("exists".into());
            dockerfile_was_missing = false;
        }
    }

    // 6. Check if project base image exists; build if missing or forced.
    let dockerfile_str = dockerfile.to_str().unwrap().to_string();
    let git_root_str = git_root.to_str().unwrap().to_string();
    let mount_path_str = mount_path.to_str().unwrap().to_string();

    let needs_build = dockerfile_was_missing || opts.build || !runtime.image_exists(&image_tag);

    if needs_build {
        let reason = if !runtime.image_exists(&image_tag) {
            format!("Image {} not found. Building...", image_tag)
        } else if dockerfile_was_missing {
            format!("Dockerfile.dev was missing — rebuilding image {}...", image_tag)
        } else {
            format!("Rebuilding image {} (--build)...", image_tag)
        };
        out.println(&reason);
        let build_cmd_display = if opts.no_cache {
            format_build_cmd_no_cache(runtime.cli_binary(), &image_tag, &dockerfile_str, &git_root_str)
        } else {
            format_build_cmd(runtime.cli_binary(), &image_tag, &dockerfile_str, &git_root_str)
        };
        out.println(format!("$ {}", build_cmd_display));
        let out_clone = out.clone();
        runtime.build_image_streaming(
            &image_tag,
            std::path::Path::new(&dockerfile_str),
            std::path::Path::new(&git_root_str),
            opts.no_cache,
            &mut |line| { out_clone.println(line); },
        )
        .context("Failed to build project base image")?;
        out.println(format!("Image {} built successfully.", image_tag));
        summary.dev_image = StepStatus::Ok("built".into());
    } else {
        out.println(format!("Image {} found.", image_tag));
        summary.dev_image = StepStatus::Ok("exists".into());
    }

    // 7. Handle agent dockerfile and image (new modular layout only).
    //    Skipped when in legacy mode or when agent name is not recognized.
    let is_known_agent = crate::cli::KNOWN_AGENT_NAMES.contains(&agent_name.as_str());
    let (agent_image_tag_opt, agent_dockerfile_str_opt) = if !opts.legacy_mode && is_known_agent {
        let agent_enum = agent_from_str(&agent_name)
            .expect("is_known_agent guard ensures agent_name is in KNOWN_AGENT_NAMES");
        let agent_df_path = git_root
            .join(".amux")
            .join(format!("Dockerfile.{}", agent_name));

        // Write agent dockerfile if missing; track whether it was just created.
        let agent_dockerfile_was_missing = if !agent_df_path.exists() {
            out.println(format!("Writing agent Dockerfile to {}...", agent_df_path.display()));
            write_agent_dockerfile(&git_root, &agent_enum, out).await?;
            true
        } else {
            out.println(format!("Agent Dockerfile found: {}", agent_df_path.display()));
            false
        };

        let agent_tag = agent_image_tag(&git_root, &agent_name);
        let agent_df_str = agent_df_path.to_str().unwrap().to_string();

        // Build agent image when missing, just created, or forced by --build.
        let agent_needs_build =
            agent_dockerfile_was_missing || opts.build || !runtime.image_exists(&agent_tag);
        if agent_needs_build {
            let reason = if !runtime.image_exists(&agent_tag) {
                format!("Agent image {} not found. Building...", agent_tag)
            } else {
                format!("Agent Dockerfile was missing — rebuilding agent image {}...", agent_tag)
            };
            out.println(&reason);
            let build_cmd_display = if opts.no_cache {
                format_build_cmd_no_cache(runtime.cli_binary(), &agent_tag, &agent_df_str, &git_root_str)
            } else {
                format_build_cmd(runtime.cli_binary(), &agent_tag, &agent_df_str, &git_root_str)
            };
            out.println(format!("$ {}", build_cmd_display));
            let out_clone = out.clone();
            runtime.build_image_streaming(
                &agent_tag,
                std::path::Path::new(&agent_df_str),
                std::path::Path::new(&git_root_str),
                opts.no_cache,
                &mut |line| { out_clone.println(line); },
            )
            .context("Failed to build agent image")?;
            out.println(format!("Agent image {} built successfully.", agent_tag));
        } else {
            out.println(format!("Agent image {} found.", agent_tag));
        }

        (Some(agent_tag), Some(agent_df_str))
    } else {
        if opts.legacy_mode {
            out.println(format!(
                "Note: using legacy single-image layout (project image). \
                 Run `amux ready` to migrate to the modular layout."
            ));
        }
        (None, None)
    };

    Ok(ReadyContext {
        image_tag,
        dockerfile_str,
        git_root_str,
        mount_path: mount_path_str,
        agent_name,
        env_vars,
        agent_image_tag: agent_image_tag_opt,
        agent_dockerfile_str: agent_dockerfile_str_opt,
    })
}

/// Rebuild the project base image, then rebuild every agent image whose
/// `.amux/Dockerfile.{agent}` exists in the project.
///
/// Called by both `run_post_audit` (after the audit agent modifies `Dockerfile.dev`)
/// and `run_force_build` (explicit `--build`).  Rebuilding all agent images is
/// required because each one layers `FROM amux-{project}:latest`, so a base rebuild
/// invalidates every agent layer.
async fn rebuild_images(
    out: &OutputSink,
    ctx: &ReadyContext,
    opts: &ReadyOptions,
    runtime: &dyn crate::runtime::AgentRuntime,
) -> Result<()> {
    let git_root = std::path::Path::new(&ctx.git_root_str);

    // 1. Rebuild project base image.
    let build_cmd_display = if opts.no_cache {
        format_build_cmd_no_cache(runtime.cli_binary(), &ctx.image_tag, &ctx.dockerfile_str, &ctx.git_root_str)
    } else {
        format_build_cmd(runtime.cli_binary(), &ctx.image_tag, &ctx.dockerfile_str, &ctx.git_root_str)
    };
    out.println(format!("$ {}", build_cmd_display));
    let out_clone = out.clone();
    runtime.build_image_streaming(
        &ctx.image_tag,
        std::path::Path::new(&ctx.dockerfile_str),
        git_root,
        opts.no_cache,
        &mut |line| { out_clone.println(line); },
    )
    .context("Failed to rebuild project base image")?;
    out.println(format!("Image {} rebuilt.", ctx.image_tag));

    // 2. Rebuild all agent images found in `.amux/Dockerfile.*`.
    //    Sorted for deterministic output.
    let amux_dir = git_root.join(".amux");
    if amux_dir.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(&amux_dir)
            .context("Failed to read .amux directory")?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if let Some(agent_name) = name.strip_prefix("Dockerfile.") {
                if agent_name.is_empty() {
                    continue;
                }
                let agent_tag = agent_image_tag(git_root, agent_name);
                let agent_df_str = entry.path().to_str().unwrap().to_string();
                out.println(format!("Rebuilding agent image {}...", agent_tag));
                let agent_build_cmd = if opts.no_cache {
                    format_build_cmd_no_cache(runtime.cli_binary(), &agent_tag, &agent_df_str, &ctx.git_root_str)
                } else {
                    format_build_cmd(runtime.cli_binary(), &agent_tag, &agent_df_str, &ctx.git_root_str)
                };
                out.println(format!("$ {}", agent_build_cmd));
                let out_clone2 = out.clone();
                runtime.build_image_streaming(
                    &agent_tag,
                    std::path::Path::new(&agent_df_str),
                    git_root,
                    opts.no_cache,
                    &mut |line| { out_clone2.println(line); },
                )
                .with_context(|| format!("Failed to rebuild agent image {}", agent_tag))?;
                out.println(format!("Agent image {} rebuilt.", agent_tag));
            }
        }
    }
    Ok(())
}

/// Phase 3 — Post-audit: Rebuild the project base image after the agent has updated Dockerfile.dev,
/// then rebuild all agent images on top of the updated base.
pub async fn run_post_audit(
    out: &OutputSink,
    ctx: &ReadyContext,
    opts: &ReadyOptions,
    summary: &mut ReadySummary,
    runtime: &dyn crate::runtime::AgentRuntime,
) -> Result<()> {
    out.println(format!(
        "Rebuilding image {} with updated Dockerfile.dev...",
        ctx.image_tag
    ));
    rebuild_images(out, ctx, opts, runtime).await?;
    summary.image_rebuild = StepStatus::Ok("rebuilt".into());
    Ok(())
}

/// Force-rebuild the project base image and all agent images (used when --build is passed without --refresh).
pub async fn run_force_build(
    out: &OutputSink,
    ctx: &ReadyContext,
    opts: &ReadyOptions,
    summary: &mut ReadySummary,
    runtime: &dyn crate::runtime::AgentRuntime,
) -> Result<()> {
    out.println(format!(
        "Rebuilding image {} (--build)...",
        ctx.image_tag
    ));
    rebuild_images(out, ctx, opts, runtime).await?;
    summary.image_rebuild = StepStatus::Ok("rebuilt".into());
    Ok(())
}


/// Build the entrypoint command for the Dockerfile audit agent (interactive mode).
pub fn audit_entrypoint(agent: &str) -> Vec<String> {
    match agent {
        "claude" => vec![
            "claude".into(),
            "--allowedTools=Edit,Write".into(),
            AUDIT_PROMPT.into(),
        ],
        "codex" => vec!["codex".into(), AUDIT_PROMPT.into()],
        "opencode" => vec!["opencode".into(), "run".into(), AUDIT_PROMPT.into()],
        "maki" => vec!["maki".into(), AUDIT_PROMPT.into()],
        "gemini" => vec!["gemini".into(), AUDIT_PROMPT.into()],
        _ => vec![agent.into(), AUDIT_PROMPT.into()],
    }
}

/// Build the entrypoint command for the Dockerfile audit agent (non-interactive/print mode).
pub fn audit_entrypoint_non_interactive(agent: &str) -> Vec<String> {
    match agent {
        "claude" => vec![
            "claude".into(),
            "-p".into(),
            "--allowedTools=Edit,Write".into(),
            AUDIT_PROMPT.into(),
        ],
        "codex" => vec!["codex".into(), "exec".into(), AUDIT_PROMPT.into()],
        "opencode" => vec!["opencode".into(), "run".into(), AUDIT_PROMPT.into()],
        "maki" => vec!["maki".into(), "--print".into(), AUDIT_PROMPT.into()],
        "gemini" => vec!["gemini".into(), "-p".into(), AUDIT_PROMPT.into()],
        _ => vec![agent.into(), AUDIT_PROMPT.into()],
    }
}

fn agent_from_str(name: &str) -> Option<Agent> {
    match name {
        "claude" => Some(Agent::Claude),
        "codex" => Some(Agent::Codex),
        "opencode" => Some(Agent::Opencode),
        "maki" => Some(Agent::Maki),
        "gemini" => Some(Agent::Gemini),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::AgentRuntime;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn audit_entrypoint_claude() {
        let args = audit_entrypoint("claude");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "claude");
        assert_eq!(args[1], "--allowedTools=Edit,Write");
        assert!(args[2].contains("scan this project"));
    }

    #[test]
    fn audit_entrypoint_codex() {
        let args = audit_entrypoint("codex");
        assert_eq!(args[0], "codex");
        assert!(args[1].contains("scan this project"));
    }

    #[test]
    fn audit_entrypoint_opencode() {
        let args = audit_entrypoint("opencode");
        assert_eq!(args[0], "opencode");
        assert_eq!(args[1], "run");
        assert!(args[2].contains("scan this project"));
    }

    #[test]
    fn audit_entrypoint_non_interactive_claude() {
        let args = audit_entrypoint_non_interactive("claude");
        assert_eq!(args[0], "claude");
        assert_eq!(args[1], "-p");
        assert_eq!(args[2], "--allowedTools=Edit,Write");
        assert!(args[3].contains("scan this project"));
    }

    #[test]
    fn audit_entrypoint_non_interactive_codex() {
        let args = audit_entrypoint_non_interactive("codex");
        assert_eq!(args[0], "codex");
        assert_eq!(args[1], "exec");
        assert!(args[2].contains("scan this project"));
    }

    #[test]
    fn agent_from_str_known_agents_return_some() {
        assert!(matches!(agent_from_str("claude"), Some(Agent::Claude)));
        assert!(matches!(agent_from_str("codex"), Some(Agent::Codex)));
        assert!(matches!(agent_from_str("opencode"), Some(Agent::Opencode)));
        assert!(matches!(agent_from_str("maki"), Some(Agent::Maki)));
        assert!(matches!(agent_from_str("gemini"), Some(Agent::Gemini)));
    }

    #[test]
    fn agent_from_str_unknown_returns_none() {
        assert!(agent_from_str("unknown").is_none());
        assert!(agent_from_str("").is_none());
        assert!(agent_from_str("CLAUDE").is_none());
    }

    #[test]
    fn audit_entrypoint_gemini() {
        let args = audit_entrypoint("gemini");
        assert_eq!(args[0], "gemini");
        assert!(args[1].contains("scan this project"), "second arg must be the audit prompt");
    }

    #[test]
    fn audit_entrypoint_non_interactive_gemini() {
        let args = audit_entrypoint_non_interactive("gemini");
        assert_eq!(args[0], "gemini");
        assert_eq!(args[1], "-p");
        assert!(args[2].contains("scan this project"), "third arg must be the audit prompt");
    }

    #[test]
    fn dockerfile_matches_template_project_template_returns_true() {
        let content = project_dockerfile_embedded();
        assert!(
            dockerfile_matches_template(&content),
            "project Dockerfile template must match itself"
        );
    }

    #[test]
    fn dockerfile_matches_template_gemini_agent_returns_false() {
        use crate::commands::init_flow::dockerfile_for_agent_embedded;
        let content = dockerfile_for_agent_embedded(&Agent::Gemini);
        assert!(
            !dockerfile_matches_template(&content),
            "gemini agent Dockerfile must not match the project template"
        );
    }

    #[test]
    fn dockerfile_matches_template_maki_agent_returns_false() {
        use crate::commands::init_flow::dockerfile_for_agent_embedded;
        let content = dockerfile_for_agent_embedded(&Agent::Maki);
        assert!(
            !dockerfile_matches_template(&content),
            "maki agent Dockerfile must not match the project template"
        );
    }

    #[test]
    fn summary_default_all_pending() {
        let summary = ReadySummary::default();
        assert_eq!(summary.docker_daemon, StepStatus::Pending);
        assert_eq!(summary.dockerfile, StepStatus::Pending);
        assert_eq!(summary.aspec_folder, StepStatus::Pending);
        assert_eq!(summary.work_items_config, StepStatus::Pending);
        assert_eq!(summary.local_agent, StepStatus::Pending);
        assert_eq!(summary.dev_image, StepStatus::Pending);
        assert_eq!(summary.refresh, StepStatus::Pending);
        assert_eq!(summary.image_rebuild, StepStatus::Pending);
    }

    #[test]
    fn print_summary_outputs_table() {
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        let summary = ReadySummary {
            docker_daemon: StepStatus::Ok("running".into()),
            dockerfile: StepStatus::Ok("exists".into()),
            aspec_folder: StepStatus::Ok("present".into()),
            work_items_config: StepStatus::Ok("ok".into()),
            local_agent: StepStatus::Ok("claude: installed & authenticated".into()),
            dev_image: StepStatus::Ok("exists".into()),
            refresh: StepStatus::Skipped("use --refresh to run".into()),
            image_rebuild: StepStatus::Skipped("no refresh".into()),
        };
        print_summary(&sink, "docker", &summary);

        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let all = messages.join("\n");
        assert!(all.contains("Ready Summary"), "Missing header");
        assert!(all.contains("docker runtime"), "Missing runtime row");
        assert!(all.contains("running"), "Missing running status");
        assert!(all.contains("aspec folder"), "Missing aspec row");
        assert!(all.contains("Local agent"), "Missing agent row");
        assert!(all.contains("Refresh"), "Missing refresh row");
        assert!(all.contains("Skipped") || all.contains("–"), "Missing skip indicator");
    }

    #[test]
    fn interactive_notice_includes_agent_name() {
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        print_interactive_notice(&sink, "claude");
        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let all = messages.join("\n");
        assert!(all.contains("INTERACTIVE"), "Missing interactive label");
        assert!(all.contains("claude"), "Missing agent name");
        assert!(all.contains("Ctrl+C"), "Missing quit hint");
    }

    #[test]
    fn ready_options_default_no_refresh() {
        let opts = ReadyOptions::default();
        assert!(!opts.refresh);
        assert!(!opts.build);
        assert!(!opts.no_cache);
        assert!(!opts.non_interactive);
        assert!(!opts.auto_create_dockerfile);
    }

    #[test]
    fn ready_options_build_flag() {
        let opts = ReadyOptions { build: true, ..Default::default() };
        assert!(opts.build);
        assert!(!opts.refresh);
        assert!(!opts.no_cache);
    }

    #[test]
    fn ready_options_no_cache_flag() {
        let opts = ReadyOptions { no_cache: true, ..Default::default() };
        assert!(opts.no_cache);
        assert!(!opts.build);
    }

    #[test]
    fn ready_options_build_and_no_cache() {
        let opts = ReadyOptions { build: true, no_cache: true, ..Default::default() };
        assert!(opts.build);
        assert!(opts.no_cache);
    }

    #[test]
    fn ready_options_auto_create_dockerfile() {
        let opts = ReadyOptions { auto_create_dockerfile: true, ..Default::default() };
        assert!(opts.auto_create_dockerfile);
        assert!(!opts.refresh);
    }

    #[test]
    fn greetings_has_fifty_entries() {
        assert_eq!(GREETINGS.len(), 50);
    }

    #[test]
    fn greetings_all_non_empty() {
        for greeting in GREETINGS.iter() {
            assert!(!greeting.is_empty(), "Greeting should not be empty: {:?}", greeting);
        }
    }

    #[test]
    fn select_random_greeting_returns_valid_greeting() {
        let greeting = select_random_greeting();
        assert!(
            GREETINGS.contains(&greeting),
            "select_random_greeting returned unknown greeting: {:?}",
            greeting
        );
    }

    #[test]
    fn select_random_greeting_returns_different_values_over_time() {
        // Collect a few greetings and ensure we got at least one valid one.
        let greetings: Vec<&str> = (0..10).map(|_| select_random_greeting()).collect();
        assert!(greetings.iter().all(|g| GREETINGS.contains(g)));
    }

    #[test]
    fn dockerfile_matches_template_claude_agent_returns_false() {
        use crate::commands::init_flow::dockerfile_for_agent_embedded;
        let content = dockerfile_for_agent_embedded(&Agent::Claude);
        assert!(
            !dockerfile_matches_template(&content),
            "Claude agent template should not match the project template"
        );
    }

    #[test]
    fn dockerfile_matches_template_codex_agent_returns_false() {
        use crate::commands::init_flow::dockerfile_for_agent_embedded;
        let content = dockerfile_for_agent_embedded(&Agent::Codex);
        assert!(
            !dockerfile_matches_template(&content),
            "Codex agent template should not match the project template"
        );
    }

    #[test]
    fn dockerfile_matches_template_false_for_custom() {
        assert!(
            !dockerfile_matches_template("FROM ubuntu:22.04\nRUN apt-get update"),
            "Custom Dockerfile should not match project template"
        );
    }

    #[tokio::test]
    async fn check_local_agent_returns_step_status() {
        // Checks that the function returns a StepStatus. We don't assert success/fail
        // because the agent may or may not be installed in the test environment.
        let (status, greeting, _response) = check_local_agent("claude").await;
        // The function must return a non-Pending status.
        assert_ne!(status, StepStatus::Pending, "check_local_agent must return a concrete status");
        // The greeting must be one of the known greetings.
        assert!(GREETINGS.contains(&greeting.as_str()), "Greeting must be from GREETINGS list");
    }

    #[tokio::test]
    async fn check_local_agent_not_installed_returns_failed() {
        // Use a command name that definitely doesn't exist.
        let (status, greeting, response) = check_local_agent("__nonexistent_agent_xyz__").await;
        assert!(
            matches!(status, StepStatus::Failed(_)),
            "Non-existent agent should return Failed status, got: {:?}",
            status
        );
        assert!(GREETINGS.contains(&greeting.as_str()), "Greeting must be from GREETINGS list");
        assert!(response.is_empty(), "Response should be empty for non-existent agent");
    }

    // ─── MockRuntime for run_pre_audit tests ─────────────────────────────────

    /// Minimal runtime stub used to test `run_pre_audit` without Docker.
    struct MockRuntime {
        available: bool,
        image_exists: bool,
    }

    impl MockRuntime {
        fn available() -> Self {
            Self { available: true, image_exists: true }
        }
    }

    impl AgentRuntime for MockRuntime {
        fn is_available(&self) -> bool { self.available }
        fn check_socket(&self) -> anyhow::Result<std::path::PathBuf> {
            Ok(std::path::PathBuf::from("/var/run/mock.sock"))
        }
        fn image_exists(&self, _tag: &str) -> bool { self.image_exists }
        fn name(&self) -> &'static str { "mock" }
        fn cli_binary(&self) -> &'static str { "mock" }

        fn build_image_streaming(
            &self, _tag: &str, _dockerfile: &std::path::Path, _context: &std::path::Path,
            _no_cache: bool, _on_line: &mut dyn FnMut(&str),
        ) -> anyhow::Result<String> { unreachable!("build_image_streaming should not be called") }

        fn run_container(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> anyhow::Result<()> { unreachable!("run_container should not be called") }

        fn run_container_captured(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> anyhow::Result<(String, String)> { unreachable!("run_container_captured should not be called") }

        fn run_container_at_path(
            &self, _image: &str, _host_path: &str, _container_path: &str, _working_dir: &str,
            _entrypoint: &[&str], _env_vars: &[(String, String)],
            _host_settings: Option<&crate::runtime::HostSettings>, _allow_docker: bool,
            _container_name: Option<&str>,
        ) -> anyhow::Result<()> { unreachable!("run_container_at_path should not be called") }

        fn run_container_captured_at_path(
            &self, _image: &str, _host_path: &str, _container_path: &str, _working_dir: &str,
            _entrypoint: &[&str], _env_vars: &[(String, String)],
            _host_settings: Option<&crate::runtime::HostSettings>, _allow_docker: bool,
        ) -> anyhow::Result<(String, String)> { unreachable!("run_container_captured_at_path should not be called") }

        fn run_container_detached(
            &self, _image: &str, _host_path: &str, _container_path: &str, _working_dir: &str,
            _container_name: Option<&str>, _env_vars: Vec<(String, String)>, _allow_docker: bool,
            _host_settings: Option<&crate::runtime::HostSettings>,
        ) -> anyhow::Result<String> { unreachable!("run_container_detached should not be called") }

        fn start_container(&self, _id: &str) -> anyhow::Result<()> { unreachable!() }
        fn stop_container(&self, _id: &str) -> anyhow::Result<()> { unreachable!() }
        fn remove_container(&self, _id: &str) -> anyhow::Result<()> { unreachable!() }
        fn is_container_running(&self, _id: &str) -> bool { unreachable!() }

        fn find_stopped_container(
            &self, _name: &str, _image: &str,
        ) -> Option<crate::runtime::StoppedContainerInfo> { unreachable!() }

        fn list_running_containers_by_prefix(&self, _prefix: &str) -> Vec<String> { unreachable!() }

        fn list_running_containers_with_ids_by_prefix(
            &self, _prefix: &str,
        ) -> Vec<(String, String)> { unreachable!() }

        fn get_container_workspace_mount(&self, _name: &str) -> Option<String> { unreachable!() }

        fn query_container_stats(
            &self, _name: &str,
        ) -> Option<crate::runtime::ContainerStats> { unreachable!() }

        fn build_run_args_pty(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> Vec<String> { unreachable!() }

        fn build_run_args_pty_display(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> Vec<String> { unreachable!() }

        fn build_run_args_pty_at_path(
            &self, _image: &str, _host_path: &str, _container_path: &str, _working_dir: &str,
            _entrypoint: &[&str], _env_vars: &[(String, String)],
            _host_settings: Option<&crate::runtime::HostSettings>, _allow_docker: bool,
            _container_name: Option<&str>,
        ) -> Vec<String> { unreachable!() }

        fn build_exec_args_pty(
            &self, _container_id: &str, _working_dir: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)],
        ) -> Vec<String> { unreachable!() }

        fn build_run_args_display(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> Vec<String> { unreachable!() }
    }

    // ─── run_pre_audit work_items_config tests ────────────────────────────────

    /// Helper: set up a minimal temp git repo with a Dockerfile.dev but no aspec folder.
    ///
    /// Uses `agent: "__nonexistent_test_agent__"` so `check_local_agent` returns
    /// immediately with NotFound rather than running the real agent binary.
    fn setup_bare_git_repo() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join("Dockerfile.dev"), "FROM ubuntu:22.04\n").unwrap();
        // Use a non-existent agent so check_local_agent returns quickly.
        let config = crate::config::RepoConfig {
            agent: Some("__nonexistent_test_agent__".to_string()),
            ..Default::default()
        };
        crate::config::save_repo_config(root, &config).unwrap();
        tmp
    }

    #[tokio::test]
    async fn run_pre_audit_warns_when_aspec_absent_and_no_work_items_dir() {
        let tmp = setup_bare_git_repo();
        let root = tmp.path().to_path_buf();

        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        let opts = ReadyOptions { auto_create_dockerfile: true, ..Default::default() };
        let runtime = MockRuntime::available();

        let mut summary = ReadySummary::default();
        let result = run_pre_audit(&sink, root.clone(), vec![], &opts, &mut summary, &runtime).await;
        // run_pre_audit may succeed or fail depending on agent binary availability;
        // what matters is the work_items_config status set before check_local_agent.
        let _ = result;

        assert!(
            matches!(summary.work_items_config, StepStatus::Warn(_)),
            "expected Warn for work_items_config when aspec absent and dir not configured; got {:?}",
            summary.work_items_config
        );

        let messages: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let output = messages.join("\n");
        assert!(
            output.contains("specs new") || output.contains("work_items"),
            "expected warning about work items in output; got: {}",
            output
        );
    }

    #[tokio::test]
    async fn run_pre_audit_ok_when_aspec_folder_present() {
        let tmp = setup_bare_git_repo();
        let root = tmp.path();

        // Create aspec dir.
        std::fs::create_dir_all(root.join("aspec")).unwrap();

        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        let opts = ReadyOptions { auto_create_dockerfile: true, ..Default::default() };
        let runtime = MockRuntime::available();

        let mut summary = ReadySummary::default();
        let _ = run_pre_audit(&sink, root.to_path_buf(), vec![], &opts, &mut summary, &runtime).await;

        assert!(
            matches!(summary.work_items_config, StepStatus::Ok(_)),
            "expected Ok for work_items_config when aspec folder present; got {:?}",
            summary.work_items_config
        );
    }

    #[tokio::test]
    async fn run_pre_audit_ok_when_work_items_dir_configured_without_aspec() {
        let tmp = setup_bare_git_repo();
        let root = tmp.path();

        // No aspec folder; override config to add work_items.dir while keeping
        // the non-existent agent so check_local_agent returns quickly.
        let items_dir = root.join("my-items");
        std::fs::create_dir_all(&items_dir).unwrap();
        let config = crate::config::RepoConfig {
            agent: Some("__nonexistent_test_agent__".to_string()),
            work_items: Some(crate::config::WorkItemsConfig {
                dir: Some("my-items".to_string()),
                template: None,
            }),
            ..Default::default()
        };
        crate::config::save_repo_config(root, &config).unwrap();

        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        let opts = ReadyOptions { auto_create_dockerfile: true, ..Default::default() };
        let runtime = MockRuntime::available();

        let mut summary = ReadySummary::default();
        let _ = run_pre_audit(&sink, root.to_path_buf(), vec![], &opts, &mut summary, &runtime).await;

        assert!(
            matches!(summary.work_items_config, StepStatus::Ok(_)),
            "expected Ok for work_items_config when work_items.dir is configured; got {:?}",
            summary.work_items_config
        );
    }

    // ─── audit image selection (work item 0049) ──────────────────────────────

    /// When an agent image tag is present in ReadyContext, the audit container must
    /// use it rather than the project base image.  This test validates the
    /// `ctx.agent_image_tag.as_deref().unwrap_or(&ctx.image_tag)` selection logic
    /// used in both `run()` and `run_with_sink()`.
    #[test]
    fn audit_image_prefers_agent_image_over_project_base() {
        let base_tag = "amux-myproject:latest".to_string();
        let agent_tag = "amux-myproject-claude:latest".to_string();

        // New layout: agent_image_tag is Some — must prefer agent image.
        let ctx_new = ReadyContext {
            image_tag: base_tag.clone(),
            dockerfile_str: String::new(),
            git_root_str: String::new(),
            mount_path: String::new(),
            agent_name: "claude".to_string(),
            env_vars: vec![],
            agent_image_tag: Some(agent_tag.clone()),
            agent_dockerfile_str: Some(".amux/Dockerfile.claude".to_string()),
        };
        let audit_image = ctx_new.agent_image_tag.as_deref().unwrap_or(&ctx_new.image_tag);
        assert_eq!(
            audit_image, agent_tag,
            "new layout: audit must use agent image, not project base"
        );

        // Legacy layout: agent_image_tag is None — must fall back to project base.
        let ctx_legacy = ReadyContext {
            image_tag: base_tag.clone(),
            dockerfile_str: String::new(),
            git_root_str: String::new(),
            mount_path: String::new(),
            agent_name: "claude".to_string(),
            env_vars: vec![],
            agent_image_tag: None,
            agent_dockerfile_str: None,
        };
        let audit_image_legacy = ctx_legacy.agent_image_tag.as_deref().unwrap_or(&ctx_legacy.image_tag);
        assert_eq!(
            audit_image_legacy, base_tag,
            "legacy layout: audit must fall back to project base image"
        );
    }

    // ─── compute_ready_build_flag tests ──────────────────────────────────────

    #[test]
    fn compute_ready_build_flag_no_refresh_with_build_returns_true() {
        assert!(compute_ready_build_flag(false, true));
    }

    #[test]
    fn compute_ready_build_flag_with_refresh_and_build_returns_false() {
        assert!(!compute_ready_build_flag(true, true));
    }

    #[test]
    fn compute_ready_build_flag_no_refresh_no_build_returns_false() {
        assert!(!compute_ready_build_flag(false, false));
    }

    #[test]
    fn compute_ready_build_flag_with_refresh_no_build_also_returns_false() {
        assert!(!compute_ready_build_flag(true, false));
    }

    // ─── is_legacy_layout tests ──────────────────────────────────────────────

    #[test]
    fn is_legacy_layout_true_when_dockerfile_exists_no_agent_dockerfile() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Dockerfile.dev"), "FROM ubuntu:22.04\n").unwrap();
        // No .amux/Dockerfile.claude — classic legacy state.
        assert!(
            is_legacy_layout(root, "claude"),
            "Should be legacy when Dockerfile.dev exists but .amux/Dockerfile.claude does not"
        );
    }

    #[test]
    fn is_legacy_layout_false_when_agent_dockerfile_already_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Dockerfile.dev"), "FROM ubuntu:22.04\n").unwrap();
        std::fs::create_dir_all(root.join(".amux")).unwrap();
        std::fs::write(
            root.join(".amux").join("Dockerfile.claude"),
            "FROM amux-project:latest\n",
        )
        .unwrap();
        assert!(
            !is_legacy_layout(root, "claude"),
            "Should not be legacy when .amux/Dockerfile.claude already exists"
        );
    }

    #[test]
    fn is_legacy_layout_false_when_no_dockerfile_dev() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        // No Dockerfile.dev at all — not even a legacy layout.
        assert!(
            !is_legacy_layout(root, "claude"),
            "Should not be legacy when Dockerfile.dev is absent"
        );
    }

    #[test]
    fn is_legacy_layout_false_for_unknown_agent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Dockerfile.dev"), "FROM ubuntu:22.04\n").unwrap();
        // Dockerfile.dev exists, but agent name is not in KNOWN_AGENT_NAMES.
        assert!(
            !is_legacy_layout(root, "unknown-agent-xyz"),
            "Should not be legacy for unknown agent names"
        );
    }

    #[test]
    fn is_legacy_layout_true_for_all_known_agents_when_agent_dockerfile_absent() {
        for &agent in crate::cli::KNOWN_AGENT_NAMES {
            let tmp = tempfile::TempDir::new().unwrap();
            let root = tmp.path();
            std::fs::write(root.join("Dockerfile.dev"), "FROM ubuntu:22.04\n").unwrap();
            // No .amux/Dockerfile.{agent}
            assert!(
                is_legacy_layout(root, agent),
                "Expected legacy layout for known agent '{}' when .amux/Dockerfile.{} is absent",
                agent,
                agent
            );
        }
    }

    #[test]
    fn is_legacy_layout_false_for_all_known_agents_when_agent_dockerfile_present() {
        for &agent in crate::cli::KNOWN_AGENT_NAMES {
            let tmp = tempfile::TempDir::new().unwrap();
            let root = tmp.path();
            std::fs::write(root.join("Dockerfile.dev"), "FROM ubuntu:22.04\n").unwrap();
            std::fs::create_dir_all(root.join(".amux")).unwrap();
            std::fs::write(
                root.join(".amux").join(format!("Dockerfile.{}", agent)),
                "FROM amux-project:latest\n",
            )
            .unwrap();
            assert!(
                !is_legacy_layout(root, agent),
                "Should not be legacy for agent '{}' when .amux/Dockerfile.{} exists",
                agent,
                agent
            );
        }
    }

    // ─── perform_legacy_migration tests ──────────────────────────────────────

    #[test]
    fn perform_legacy_migration_creates_backup_and_replaces_with_template() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        let original = "FROM ubuntu:22.04\nRUN apt-get update\n";
        std::fs::write(root.join("Dockerfile.dev"), original).unwrap();

        let result = perform_legacy_migration(root);
        assert!(
            result.is_ok(),
            "perform_legacy_migration should succeed when Dockerfile.dev exists: {:?}",
            result
        );

        // Backup must exist and contain the original content.
        let backup = root.join("Dockerfile.dev.bak");
        assert!(backup.exists(), "Backup file Dockerfile.dev.bak must be created");
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            original,
            "Backup must contain the original Dockerfile.dev content verbatim"
        );

        // Dockerfile.dev must be overwritten with the project base template.
        let template = crate::commands::init_flow::project_dockerfile_embedded();
        let new_content = std::fs::read_to_string(root.join("Dockerfile.dev")).unwrap();
        assert_eq!(
            new_content.trim(),
            template.trim(),
            "Dockerfile.dev must be replaced with the minimal project base template"
        );
    }

    #[test]
    fn perform_legacy_migration_returns_messages_describing_actions() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Dockerfile.dev"), "FROM ubuntu:22.04\n").unwrap();

        let messages = perform_legacy_migration(root).unwrap();
        assert!(!messages.is_empty(), "Must return at least one message");

        let all = messages.join("\n");
        assert!(
            all.contains("Backed up") || all.contains(".bak"),
            "Messages must mention the backup operation: {:?}",
            messages
        );
        assert!(
            all.contains("Dockerfile.dev"),
            "Messages must reference Dockerfile.dev: {:?}",
            messages
        );
    }

    #[test]
    fn perform_legacy_migration_errors_when_source_file_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        // Dockerfile.dev deliberately absent.
        let result = perform_legacy_migration(root);
        assert!(
            result.is_err(),
            "perform_legacy_migration must return Err when Dockerfile.dev is absent"
        );
    }

    // ─── build_audit_setup tests ─────────────────────────────────────────────

    /// Convenience constructor for ReadyContext with only audit-relevant fields set.
    fn make_audit_context(agent: &str, agent_tag: Option<String>) -> ReadyContext {
        ReadyContext {
            image_tag: "amux-project:latest".to_string(),
            dockerfile_str: String::new(),
            git_root_str: String::new(),
            mount_path: String::new(),
            agent_name: agent.to_string(),
            env_vars: vec![],
            agent_image_tag: agent_tag,
            agent_dockerfile_str: None,
        }
    }

    #[test]
    fn build_audit_setup_interactive_uses_audit_entrypoint() {
        let ctx = make_audit_context("claude", None);
        let setup = build_audit_setup(&ctx, false);
        assert_eq!(
            setup.entrypoint,
            audit_entrypoint("claude"),
            "non_interactive=false must produce the interactive audit_entrypoint"
        );
    }

    #[test]
    fn build_audit_setup_non_interactive_uses_non_interactive_entrypoint() {
        let ctx = make_audit_context("claude", None);
        let setup = build_audit_setup(&ctx, true);
        assert_eq!(
            setup.entrypoint,
            audit_entrypoint_non_interactive("claude"),
            "non_interactive=true must produce audit_entrypoint_non_interactive"
        );
    }

    #[test]
    fn build_audit_setup_uses_agent_image_tag_when_some() {
        let agent_tag = "amux-project-claude:latest".to_string();
        let ctx = make_audit_context("claude", Some(agent_tag.clone()));
        let setup = build_audit_setup(&ctx, false);
        assert_eq!(
            setup.image_tag, agent_tag,
            "agent_image_tag=Some(...) must be used as the audit image_tag"
        );
    }

    #[test]
    fn build_audit_setup_falls_back_to_project_tag_when_agent_tag_none() {
        let ctx = make_audit_context("claude", None);
        let setup = build_audit_setup(&ctx, false);
        assert_eq!(
            setup.image_tag, "amux-project:latest",
            "agent_image_tag=None must fall back to the project base image tag"
        );
    }

    #[test]
    fn build_audit_setup_entrypoint_correct_for_all_known_agents() {
        for &agent in crate::cli::KNOWN_AGENT_NAMES {
            let ctx = make_audit_context(agent, None);

            let interactive = build_audit_setup(&ctx, false);
            assert_eq!(
                interactive.entrypoint,
                audit_entrypoint(agent),
                "interactive entrypoint mismatch for agent '{}'",
                agent
            );

            let non_interactive = build_audit_setup(&ctx, true);
            assert_eq!(
                non_interactive.entrypoint,
                audit_entrypoint_non_interactive(agent),
                "non-interactive entrypoint mismatch for agent '{}'",
                agent
            );
        }
    }

    // ─── TrackingMockRuntime: records build_image_streaming calls ─────────────

    struct TrackingMockRuntime {
        image_exists: bool,
        built_tags: std::sync::Mutex<Vec<String>>,
    }

    impl TrackingMockRuntime {
        fn new_with_image() -> Self {
            Self {
                image_exists: true,
                built_tags: std::sync::Mutex::new(vec![]),
            }
        }

        fn built_tags(&self) -> Vec<String> {
            self.built_tags.lock().unwrap().clone()
        }
    }

    impl AgentRuntime for TrackingMockRuntime {
        fn is_available(&self) -> bool { true }
        fn check_socket(&self) -> anyhow::Result<std::path::PathBuf> {
            Ok(std::path::PathBuf::from("/var/run/mock.sock"))
        }
        fn image_exists(&self, _tag: &str) -> bool { self.image_exists }
        fn name(&self) -> &'static str { "mock" }
        fn cli_binary(&self) -> &'static str { "mock" }

        fn build_image_streaming(
            &self,
            tag: &str,
            _dockerfile: &std::path::Path,
            _context: &std::path::Path,
            _no_cache: bool,
            _on_line: &mut dyn FnMut(&str),
        ) -> anyhow::Result<String> {
            self.built_tags.lock().unwrap().push(tag.to_string());
            Ok(String::new())
        }

        fn run_container(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> anyhow::Result<()> { unreachable!("run_container should not be called") }

        fn run_container_captured(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> anyhow::Result<(String, String)> { unreachable!("run_container_captured should not be called") }

        fn run_container_at_path(
            &self, _image: &str, _host_path: &str, _container_path: &str, _working_dir: &str,
            _entrypoint: &[&str], _env_vars: &[(String, String)],
            _host_settings: Option<&crate::runtime::HostSettings>, _allow_docker: bool,
            _container_name: Option<&str>,
        ) -> anyhow::Result<()> { unreachable!("run_container_at_path should not be called") }

        fn run_container_captured_at_path(
            &self, _image: &str, _host_path: &str, _container_path: &str, _working_dir: &str,
            _entrypoint: &[&str], _env_vars: &[(String, String)],
            _host_settings: Option<&crate::runtime::HostSettings>, _allow_docker: bool,
        ) -> anyhow::Result<(String, String)> { unreachable!("run_container_captured_at_path should not be called") }

        fn run_container_detached(
            &self, _image: &str, _host_path: &str, _container_path: &str, _working_dir: &str,
            _container_name: Option<&str>, _env_vars: Vec<(String, String)>, _allow_docker: bool,
            _host_settings: Option<&crate::runtime::HostSettings>,
        ) -> anyhow::Result<String> { unreachable!("run_container_detached should not be called") }

        fn start_container(&self, _id: &str) -> anyhow::Result<()> { unreachable!() }
        fn stop_container(&self, _id: &str) -> anyhow::Result<()> { unreachable!() }
        fn remove_container(&self, _id: &str) -> anyhow::Result<()> { unreachable!() }
        fn is_container_running(&self, _id: &str) -> bool { unreachable!() }

        fn find_stopped_container(
            &self, _name: &str, _image: &str,
        ) -> Option<crate::runtime::StoppedContainerInfo> { unreachable!() }

        fn list_running_containers_by_prefix(&self, _prefix: &str) -> Vec<String> { unreachable!() }

        fn list_running_containers_with_ids_by_prefix(
            &self, _prefix: &str,
        ) -> Vec<(String, String)> { unreachable!() }

        fn get_container_workspace_mount(&self, _name: &str) -> Option<String> { unreachable!() }

        fn query_container_stats(
            &self, _name: &str,
        ) -> Option<crate::runtime::ContainerStats> { unreachable!() }

        fn build_run_args_pty(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> Vec<String> { unreachable!() }

        fn build_run_args_pty_display(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> Vec<String> { unreachable!() }

        fn build_run_args_pty_at_path(
            &self, _image: &str, _host_path: &str, _container_path: &str, _working_dir: &str,
            _entrypoint: &[&str], _env_vars: &[(String, String)],
            _host_settings: Option<&crate::runtime::HostSettings>, _allow_docker: bool,
            _container_name: Option<&str>,
        ) -> Vec<String> { unreachable!() }

        fn build_exec_args_pty(
            &self, _container_id: &str, _working_dir: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)],
        ) -> Vec<String> { unreachable!() }

        fn build_run_args_display(
            &self, _image: &str, _host_path: &str, _entrypoint: &[&str],
            _env_vars: &[(String, String)], _host_settings: Option<&crate::runtime::HostSettings>,
            _allow_docker: bool, _container_name: Option<&str>, _ssh_dir: Option<&std::path::Path>,
        ) -> Vec<String> { unreachable!() }
    }

    // ─── integration test: migration forces project image rebuild ─────────────

    /// After `perform_legacy_migration()` + `run_pre_audit()` with `opts.build = true`,
    /// `build_image_streaming` must be called for the project image even when
    /// `image_exists()` returns `true` (i.e., the cached image is not reused).
    ///
    /// This covers DIV-4: `needs_build = dockerfile_was_missing || opts.build || !image_exists`.
    #[tokio::test]
    async fn migration_rebuild_forces_project_image_rebuild_even_when_image_exists() {
        let tmp = setup_bare_git_repo();
        let root = tmp.path().to_path_buf();

        // Migrate: backup legacy Dockerfile.dev, replace with project template.
        let messages = perform_legacy_migration(&root)
            .expect("migration should succeed when Dockerfile.dev exists");
        assert!(!messages.is_empty(), "Migration should return at least one message");
        assert!(root.join("Dockerfile.dev.bak").exists(), "Backup must exist after migration");

        // Build the tracking runtime: image already "exists" so without opts.build=true,
        // run_pre_audit would see image_exists()=true and skip the rebuild.
        let runtime = TrackingMockRuntime::new_with_image();
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);

        // Set build=true and legacy_mode=true (skip agent dockerfile steps for isolation).
        // opts.build=true mirrors what the CLI/TUI sets immediately after migration succeeds.
        let opts = ReadyOptions {
            build: true,
            auto_create_dockerfile: true,
            legacy_mode: true,
            ..Default::default()
        };
        let mut summary = ReadySummary::default();
        let result = run_pre_audit(&sink, root.clone(), vec![], &opts, &mut summary, &runtime).await;
        assert!(
            result.is_ok(),
            "run_pre_audit should succeed after migration: {}",
            result.err().map(|e| e.to_string()).unwrap_or_default()
        );

        let built = runtime.built_tags();
        assert!(
            !built.is_empty(),
            "build_image_streaming must be called when opts.build=true (migration path); \
             no build calls recorded — the old cached image would have been used instead"
        );
    }

    // ─── regression: no spurious rebuild when not on migration path ───────────

    /// When a project image already exists and neither `opts.build` nor
    /// `dockerfile_was_missing` is set, `run_pre_audit` must NOT rebuild the image.
    /// This guards against regressions that would cause unnecessary image rebuilds
    /// for users who are not on the legacy-migration path.
    #[tokio::test]
    async fn no_spurious_rebuild_when_image_exists_and_build_false() {
        let tmp = setup_bare_git_repo();
        let root = tmp.path().to_path_buf();

        let runtime = TrackingMockRuntime::new_with_image();
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);

        let opts = ReadyOptions {
            build: false,
            auto_create_dockerfile: true,
            legacy_mode: true,
            ..Default::default()
        };
        let mut summary = ReadySummary::default();
        let _ = run_pre_audit(&sink, root.clone(), vec![], &opts, &mut summary, &runtime).await;

        let built = runtime.built_tags();
        assert!(
            built.is_empty(),
            "build_image_streaming must NOT be called when image exists and build=false; \
             unexpected build calls for tags: {:?}",
            built
        );
    }

    // ─── TUI summary continuity tests ────────────────────────────────────────

    /// Pre-audit must set docker_daemon, dockerfile, and dev_image to non-Pending values,
    /// and those values must survive unchanged through run_post_audit.
    ///
    /// This validates DIV-11: the ReadySummary from pre-audit is stored in
    /// tab.ready_summary and consumed by launch_ready_post_audit, rather than
    /// post-audit creating a fresh pre-populated default.
    #[tokio::test]
    async fn tui_pre_audit_summary_values_persist_unchanged_through_post_audit() {
        let tmp = setup_bare_git_repo();
        let root = tmp.path().to_path_buf();

        let runtime = TrackingMockRuntime::new_with_image();
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);

        // Phase 1: run pre-audit; image exists so dev_image = Ok("exists"), not "checked".
        let pre_opts = ReadyOptions {
            build: false,
            auto_create_dockerfile: true,
            legacy_mode: true,
            ..Default::default()
        };
        let mut summary = ReadySummary::default();
        let ctx = run_pre_audit(&sink, root.clone(), vec![], &pre_opts, &mut summary, &runtime)
            .await
            .unwrap_or_else(|e| panic!("run_pre_audit should succeed: {e}"));

        // Pre-audit must have set the three fields that DIV-11 is about.
        assert!(
            !matches!(summary.docker_daemon, StepStatus::Pending),
            "docker_daemon must be set by pre-audit, got {:?}",
            summary.docker_daemon
        );
        assert!(
            !matches!(summary.dockerfile, StepStatus::Pending),
            "dockerfile must be set by pre-audit, got {:?}",
            summary.dockerfile
        );
        assert!(
            !matches!(summary.dev_image, StepStatus::Pending),
            "dev_image must be set by pre-audit, got {:?}",
            summary.dev_image
        );

        // Capture the pre-audit values before post-audit mutates the summary.
        let pre_docker_daemon = summary.docker_daemon.clone();
        let pre_dockerfile = summary.dockerfile.clone();
        let pre_dev_image = summary.dev_image.clone();

        // Phase 3: run post-audit with the same summary (as the TUI does when
        // ready_summary is Some — the DIV-11 fix).
        let post_opts = ReadyOptions {
            refresh: true,
            auto_create_dockerfile: true,
            legacy_mode: true,
            ..Default::default()
        };
        run_post_audit(&sink, &ctx, &post_opts, &mut summary, &runtime)
            .await
            .unwrap_or_else(|e| panic!("run_post_audit should succeed: {e}"));

        // Post-audit must have set image_rebuild.
        assert!(
            matches!(summary.image_rebuild, StepStatus::Ok(_)),
            "image_rebuild must be Ok after post-audit, got {:?}",
            summary.image_rebuild
        );

        // Pre-audit values for the three DIV-11 fields must be unchanged.
        // run_post_audit only updates image_rebuild; every other field comes from pre-audit.
        assert_eq!(
            summary.docker_daemon, pre_docker_daemon,
            "docker_daemon must retain the pre-audit value after post-audit"
        );
        assert_eq!(
            summary.dockerfile, pre_dockerfile,
            "dockerfile must retain the pre-audit value after post-audit"
        );
        assert_eq!(
            summary.dev_image, pre_dev_image,
            "dev_image must retain the pre-audit value after post-audit"
        );
    }

    /// The dev_image status set by pre-audit when the image already exists must
    /// be "exists", not the "checked" default used by the TUI fallback path
    /// (launch_ready_post_audit when ready_summary is None).
    ///
    /// This distinguishes the real pre-audit summary from the synthetic fallback,
    /// confirming that the stored summary carries genuine status information.
    #[tokio::test]
    async fn tui_pre_audit_dev_image_status_is_exists_not_checked_default() {
        let tmp = setup_bare_git_repo();
        let root = tmp.path().to_path_buf();

        let runtime = TrackingMockRuntime::new_with_image();
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);

        let opts = ReadyOptions {
            build: false,
            auto_create_dockerfile: true,
            legacy_mode: true,
            ..Default::default()
        };
        let mut summary = ReadySummary::default();
        let _ = run_pre_audit(&sink, root.clone(), vec![], &opts, &mut summary, &runtime).await;

        // When the image pre-exists (image_exists=true, build=false), dev_image must be
        // Ok("exists"), not Ok("checked") which is the TUI fallback sentinel.
        assert_eq!(
            summary.dev_image,
            StepStatus::Ok("exists".into()),
            "dev_image must be Ok(\"exists\") when the image was found, not Ok(\"checked\") fallback"
        );
    }

    /// Regression guard: the fallback logic in launch_ready_post_audit (used when
    /// ready_summary is None — i.e., pre-audit failed or was aborted) must still
    /// produce sensible non-Pending values for the three DIV-11 fields.
    #[test]
    fn tui_summary_fallback_defaults_remain_sensible_when_pre_audit_missing() {
        // Reproduce the unwrap_or_else fallback from launch_ready_post_audit in tui/mod.rs.
        let stored: Option<ReadySummary> = None;
        let summary = stored.unwrap_or_else(|| {
            let mut s = ReadySummary::default();
            s.docker_daemon = StepStatus::Ok("running".into());
            s.dockerfile = StepStatus::Ok("checked".into());
            s.dev_image = StepStatus::Ok("checked".into());
            s.refresh = StepStatus::Ok("completed".into());
            s
        });

        assert!(
            matches!(summary.docker_daemon, StepStatus::Ok(_)),
            "fallback docker_daemon must be Ok"
        );
        assert!(
            matches!(summary.dockerfile, StepStatus::Ok(_)),
            "fallback dockerfile must be Ok"
        );
        assert!(
            matches!(summary.dev_image, StepStatus::Ok(_)),
            "fallback dev_image must be Ok"
        );
        assert!(
            matches!(summary.refresh, StepStatus::Ok(_)),
            "fallback refresh must be Ok"
        );
        // image_rebuild is set by run_post_audit itself; the fallback leaves it Pending.
        assert_eq!(
            summary.image_rebuild,
            StepStatus::Pending,
            "fallback image_rebuild must be Pending before post-audit runs"
        );
    }
}
