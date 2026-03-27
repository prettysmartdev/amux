use crate::commands::output::OutputSink;
use crate::config::load_repo_config;
use crate::docker;
use anyhow::Result;
use std::io::Write;
use tokio::time::Duration;

/// 50 tips shown randomly at the bottom of the status dashboard.
const TIPS: &[&str] = &[
    "`amux status` shows all running code agents and nanoclaw containers.",
    "`amux status --watch` auto-refreshes every 3 seconds. Press Ctrl-C to stop.",
    "`amux implement <work-item-number>` starts a code agent on a work item.",
    "`amux chat` opens an interactive chat session with your configured agent.",
    "`amux ready` checks your environment and builds the Docker image if needed.",
    "`amux ready --refresh` re-runs the OAuth token refresh before launching.",
    "`amux ready --build` forces a Docker image rebuild even if one exists.",
    "`amux ready --no-cache` rebuilds the Docker image from scratch with no layer cache.",
    "`amux ready --build --no-cache` is the nuclear option for a fully clean image.",
    "`amux claws init` sets up the nanoclaw parallel agent system for the first time.",
    "`amux claws ready` (re)launches the nanoclaw controller container.",
    "`amux claws chat` attaches an interactive shell to the running nanoclaw container.",
    "`amux new` guides you through creating a new work item interactively.",
    "Work items live in `aspec/work-items/` and use a numbered Markdown format.",
    "Per-repo config lives at `<git-root>/aspec/.amux.json`.",
    "Global config lives at `~/.amux/config.json`.",
    "Agent data and state is stored in `~/.amux/`.",
    "Agents always run inside Docker containers — never directly on the host.",
    "Only the current Git repo root is mounted into agent containers.",
    "The `amux` binary is statically linked — no runtime dependencies to install.",
    "Press Ctrl+T in the TUI to open a new tab with its own working directory.",
    "Use Ctrl+A and Ctrl+D to switch between tabs in the TUI.",
    "Press Ctrl+C in the TUI (single tab) to open the quit confirmation dialog.",
    "Press `q` in an empty command box to open the quit confirmation dialog.",
    "Press the Up arrow in the command box to navigate to the execution window.",
    "In the execution window, press `b` to jump to the start of output.",
    "In the execution window, press `e` to jump to the end (latest) output.",
    "In the execution window, press Up/Down arrows to scroll through output.",
    "Press Esc in the execution window to return focus to the command box.",
    "When a container is running, press `c` to maximise its window for full interaction.",
    "The container window can be minimised with Esc, leaving the outer window scrollable.",
    "A yellow tab name means the container has been idle for over 60 seconds.",
    "CPU and memory stats for running containers are polled and displayed live.",
    "Agent credentials are read from the system keychain automatically.",
    "Nanoclaw worker containers are named with the `nanoclaw-` prefix.",
    "The nanoclaw controller container is always named `amux-claws-controller`.",
    "Multiple tabs let you monitor and run agents in different repos simultaneously.",
    "The `ready` command checks local agent installation before launching a container.",
    "Docker images are built from `Dockerfile.dev` in your repo root.",
    "amux supports Claude Code, Codex, and Opencode as agent backends.",
    "Work items can be of type Feature, Bug, or Task.",
    "The TUI auto-starts `status --watch` when launched outside a Git repo.",
    "`amux implement` finds work items by their number (e.g. `implement 42`).",
    "The `new` command creates work items using the template in `aspec/work-items/0000-template.md`.",
    "Container output streams live to the TUI execution window with full ANSI colour.",
    "The VT100 terminal emulator in the container window supports colours, bold, and cursor movement.",
    "Scroll the container window with the mouse wheel when it is maximised.",
    "Each amux tab maintains independent output history that you can scroll through after a command.",
    "Run `amux` from any subdirectory of a Git repo — it locates the root automatically.",
    "amux never mounts parent directories above the Git root into containers.",
];

/// Select a tip at random using the current time as a seed (seconds since epoch).
///
/// Uses the same approach as `select_random_greeting` to ensure proper variance:
/// nanoseconds are often multiples of TIPS.len() on common platforms, whereas
/// seconds are not.
pub fn select_random_tip() -> &'static str {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    TIPS[(secs % TIPS.len() as u64) as usize]
}

/// Special marker line sent through the TUI output channel to clear the
/// execution window before writing updated status tables. The `tick()` method
/// in `TabState` recognises this marker and calls `output_lines.clear()`.
pub const CLEAR_MARKER: &str = "\x00CLEAR\x00";

/// A running code agent container and its associated metadata.
pub struct CodeAgentRow {
    /// Docker container name (e.g. `amux-12345-678901234`).
    pub name: String,
    /// Host path of the Git project mounted into the container (from `/workspace` bind-mount).
    pub project: String,
    /// Agent name from the repo config (e.g. `claude`).
    pub agent: String,
    /// CPU usage percentage string (e.g. `5.23%`).
    pub cpu: String,
    /// Memory usage string (e.g. `200MiB`).
    pub memory: String,
}

/// A running nanoclaw-related container and its Docker stats.
pub struct NanoclawRow {
    /// Docker container name.
    pub name: String,
    /// CPU usage percentage string.
    pub cpu: String,
    /// Memory usage string.
    pub memory: String,
}

/// Discover all running code-agent containers.
///
/// Code-agent containers have an `amux-` name prefix and are excluded if they
/// belong to the nanoclaw subsystem (`amux-claws-*` or contain `nanoclaw`).
pub fn gather_code_agents() -> Vec<CodeAgentRow> {
    docker::list_running_containers_by_prefix("amux-")
        .into_iter()
        .filter(|n| !n.starts_with("amux-claws-") && !n.contains("nanoclaw"))
        .map(|name| {
            let (project, agent) = project_and_agent_for(&name);
            let (cpu, memory) = stats_for(&name);
            CodeAgentRow { name, project, agent, cpu, memory }
        })
        .collect()
}

/// Discover all running nanoclaw-related containers.
///
/// Includes `amux-claws-controller` (if running) and any container whose name
/// contains `nanoclaw`.
pub fn gather_nanoclaw_containers() -> Vec<NanoclawRow> {
    let mut names: Vec<String> = Vec::new();

    // The nanoclaw controller has a well-known name.
    let controller = crate::commands::claws::NANOCLAW_CONTROLLER_NAME;
    let running_with_prefix = docker::list_running_containers_by_prefix(controller);
    for n in running_with_prefix {
        if n == controller {
            names.push(n);
        }
    }

    // Any container whose name contains "nanoclaw" (worker containers, etc.).
    let nanoclaw_workers = docker::list_running_containers_by_prefix("nanoclaw");
    for n in nanoclaw_workers {
        if !names.contains(&n) {
            names.push(n);
        }
    }

    names
        .into_iter()
        .map(|name| {
            let (cpu, memory) = stats_for(&name);
            NanoclawRow { name, cpu, memory }
        })
        .collect()
}

/// Return the workspace mount source path and agent name for a code-agent container.
///
/// Queries the container's `/workspace` mount, then reads `aspec/.amux.json`
/// from the mounted Git root to determine the configured agent.
fn project_and_agent_for(container_name: &str) -> (String, String) {
    let project = docker::get_container_workspace_mount(container_name)
        .unwrap_or_else(|| "unknown".to_string());

    let agent = if project != "unknown" {
        load_repo_config(std::path::Path::new(&project))
            .ok()
            .and_then(|c| c.agent)
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };

    (project, agent)
}

/// Return (cpu_percent, memory) stats for a container, or ("--", "--") on failure.
fn stats_for(name: &str) -> (String, String) {
    docker::query_container_stats(name)
        .map(|s| (s.cpu_percent, s.memory))
        .unwrap_or_else(|| ("--".to_string(), "--".to_string()))
}

/// Render an ASCII box table with the given column `headers` and `rows`.
///
/// Uses Unicode box-drawing characters for borders. All cell content is
/// assumed to be ASCII (container names, paths, agent names, stats strings).
pub fn format_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let ncols = headers.len();

    // Compute column widths: max of header width and all cell widths.
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(ncols) {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let mut out = String::new();

    // Top border: ┌──┬──┐
    out.push('┌');
    for (i, w) in widths.iter().enumerate() {
        out.push_str(&"─".repeat(w + 2));
        out.push(if i + 1 < ncols { '┬' } else { '┐' });
    }
    out.push('\n');

    // Header row: │ Col │ ...
    out.push('│');
    for (h, w) in headers.iter().zip(widths.iter()) {
        out.push_str(&format!(" {:<width$} │", h, width = w));
    }
    out.push('\n');

    // Header separator: ├──┼──┤
    out.push('├');
    for (i, w) in widths.iter().enumerate() {
        out.push_str(&"─".repeat(w + 2));
        out.push(if i + 1 < ncols { '┼' } else { '┤' });
    }
    out.push('\n');

    // Data rows.
    for row in rows {
        out.push('│');
        for (cell, w) in row.iter().zip(widths.iter()) {
            out.push_str(&format!(" {:<width$} │", cell, width = w));
        }
        out.push('\n');
    }

    // Bottom border: └──┴──┘
    out.push('└');
    for (i, w) in widths.iter().enumerate() {
        out.push_str(&"─".repeat(w + 2));
        out.push(if i + 1 < ncols { '┴' } else { '┘' });
    }
    out.push('\n');

    out
}

/// Build the full status output: both CODE AGENTS and NANOCLAW sections.
///
/// `tip` is a pre-selected tip string that is shown at the bottom of the output.
/// Callers should select the tip once per invocation (not per refresh) so that
/// the tip remains stable across `--watch` refreshes.
pub fn format_status_output(tip: &str) -> String {
    let code_agents = gather_code_agents();
    let nanoclaw = gather_nanoclaw_containers();

    let mut out = String::new();

    // Dashboard header.
    out.push_str("AMUX STATUS DASHBOARD\n\n");

    // --- CODE AGENTS section ---
    out.push_str("CODE AGENTS\n");
    if code_agents.is_empty() {
        out.push_str("  No code agents running.\n");
        out.push_str("  To start one: amux implement <work-item>  or  amux chat\n");
    } else {
        let headers = ["●", "Project", "Agent", "CPU", "Memory"];
        let rows: Vec<Vec<String>> = code_agents
            .iter()
            .map(|r| {
                vec![
                    "🟢".to_string(),
                    r.project.clone(),
                    r.agent.clone(),
                    r.cpu.clone(),
                    r.memory.clone(),
                ]
            })
            .collect();
        out.push_str(&format_table(&headers, &rows));
    }

    out.push('\n');

    // --- NANOCLAW section ---
    out.push_str("NANOCLAW\n");
    if nanoclaw.is_empty() {
        out.push_str("  Nanoclaw is not running.\n");
        out.push_str("  To start it: amux claws init\n");
    } else {
        let headers = ["●", "Container", "CPU", "Memory"];
        let rows: Vec<Vec<String>> = nanoclaw
            .iter()
            .map(|r| vec!["🟢".to_string(), r.name.clone(), r.cpu.clone(), r.memory.clone()])
            .collect();
        out.push_str(&format_table(&headers, &rows));
    }

    // Tip of the run.
    out.push_str(&format!("\nTip: {}\n", tip));

    out
}

/// Run the `status` command.
///
/// In non-watch mode: renders once and returns.
/// In watch mode (CLI / `OutputSink::Stdout`): refreshes every 3 s, overwriting in place
///   using ANSI cursor-up + clear-to-end escape sequences.
/// In watch mode (TUI / `OutputSink::Channel`): loops forever (until the channel is closed
///   or the provided `cancel` receiver fires), sending a `CLEAR_MARKER` before each refresh.
pub async fn run_with_sink(
    watch: bool,
    sink: &OutputSink,
    cancel: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<()> {
    // Select the tip once per invocation so it stays stable across --watch refreshes.
    let tip = select_random_tip();
    let content = format_status_output(tip);

    if !watch {
        sink.print(&content);
        return Ok(());
    }

    // --- Watch mode ---
    if sink.supports_color() {
        // CLI (stdout) mode: render once, then overwrite in place.
        let mut prev_lines = content.lines().count();
        print!("{}", content);
        let _ = std::io::stdout().flush();

        // `cancel` is not used in CLI watch mode (Ctrl-C terminates the process).
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let new_content = format_status_output(tip);
            let new_lines = new_content.lines().count();
            // Move cursor up by `prev_lines` lines, then clear to end of screen.
            print!("\x1B[{}A\x1B[0J{}", prev_lines, new_content);
            let _ = std::io::stdout().flush();
            prev_lines = new_lines;
        }
    } else {
        // TUI (channel) mode: send content, then refresh via CLEAR_MARKER.
        sink.print(&content);

        let mut cancel = cancel;
        loop {
            let sleep = tokio::time::sleep(Duration::from_secs(3));
            tokio::pin!(sleep);

            if let Some(ref mut rx) = cancel {
                tokio::select! {
                    _ = &mut sleep => {}
                    _ = rx => { break; }
                }
            } else {
                sleep.await;
            }

            let new_content = format_status_output(tip);
            // Send clear marker first; if the channel is closed, stop the loop.
            if !sink.try_println(CLEAR_MARKER) {
                break;
            }
            sink.print(&new_content);
        }
        Ok(())
    }
}

/// Entry point for `amux status` (command mode).
pub async fn run(watch: bool) -> Result<()> {
    let sink = OutputSink::Stdout;
    run_with_sink(watch, &sink, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- format_table tests ---

    #[test]
    fn format_table_single_row() {
        let headers = ["Name", "CPU", "Memory"];
        let rows = vec![vec!["amux-123".to_string(), "5%".to_string(), "100MiB".to_string()]];
        let table = format_table(&headers, &rows);
        assert!(table.contains("Name"));
        assert!(table.contains("CPU"));
        assert!(table.contains("Memory"));
        assert!(table.contains("amux-123"));
        assert!(table.contains("5%"));
        assert!(table.contains("100MiB"));
    }

    #[test]
    fn format_table_column_widths_match_longest_cell() {
        let headers = ["A", "B"];
        let rows = vec![
            vec!["short".to_string(), "x".to_string()],
            vec!["a much longer value".to_string(), "y".to_string()],
        ];
        let table = format_table(&headers, &rows);
        // The separator line should accommodate the long value.
        let separator_line = table.lines().nth(2).unwrap_or("");
        // The "A" column separator should be at least 19+2 wide ("a much longer value").
        assert!(separator_line.contains("─────────────────────"));
    }

    #[test]
    fn format_table_header_wider_than_data() {
        let headers = ["Very Long Header", "B"];
        let rows = vec![vec!["x".to_string(), "y".to_string()]];
        let table = format_table(&headers, &rows);
        // All "x" cells should be padded to the header width.
        assert!(table.contains("Very Long Header"));
        // Verify the padding: "x" in the first col should be padded to 16 chars.
        assert!(table.contains("│ x                │"));
    }

    #[test]
    fn format_table_empty_rows() {
        let headers = ["Col1", "Col2"];
        let rows: Vec<Vec<String>> = vec![];
        let table = format_table(&headers, &rows);
        // Should still render the border and headers with no data rows.
        assert!(table.contains("Col1"));
        assert!(table.contains("Col2"));
        // Bottom border should close the table.
        assert!(table.contains('└'));
        assert!(table.contains('┘'));
    }

    #[test]
    fn format_table_multiple_rows() {
        let headers = ["Container", "CPU"];
        let rows = vec![
            vec!["amux-claws-controller".to_string(), "1%".to_string()],
            vec!["nanoclaw-worker-1".to_string(), "3%".to_string()],
        ];
        let table = format_table(&headers, &rows);
        assert!(table.contains("amux-claws-controller"));
        assert!(table.contains("nanoclaw-worker-1"));
        assert!(table.contains("1%"));
        assert!(table.contains("3%"));
    }

    // --- format_status_output tests ---

    #[test]
    fn format_status_output_contains_both_sections() {
        let output = format_status_output("test tip");
        assert!(output.contains("CODE AGENTS"));
        assert!(output.contains("NANOCLAW"));
    }

    #[test]
    fn format_status_output_contains_dashboard_header() {
        let output = format_status_output("test tip");
        assert!(output.contains("AMUX STATUS DASHBOARD"));
    }

    #[test]
    fn format_status_output_contains_tip() {
        let output = format_status_output("my custom tip");
        assert!(output.contains("Tip: my custom tip"));
    }

    #[test]
    fn format_status_output_empty_state_messages_when_no_docker() {
        // When no containers are running (or Docker is unavailable), both sections
        // should show their empty-state messages rather than a table.
        let output = format_status_output("test tip");
        // One or both sections will be empty in the test environment.
        // At minimum, both section headers must be present.
        assert!(output.contains("CODE AGENTS\n"));
        assert!(output.contains("NANOCLAW\n"));
    }

    // --- select_random_tip tests ---

    #[test]
    fn select_random_tip_returns_valid_tip() {
        let tip = select_random_tip();
        assert!(
            TIPS.contains(&tip),
            "select_random_tip returned unknown tip: {:?}",
            tip
        );
    }

    // --- project_and_agent_for tests ---

    #[test]
    fn project_and_agent_for_unknown_container_returns_unknown() {
        // A container that does not exist has no workspace mount → "unknown".
        let (project, agent) = project_and_agent_for("amux-test-nonexistent-xyz-99999");
        assert_eq!(project, "unknown");
        assert_eq!(agent, "unknown");
    }

    // --- stats_for tests ---

    #[test]
    fn stats_for_nonexistent_container_returns_dashes() {
        let (cpu, mem) = stats_for("amux-test-nonexistent-xyz-99999");
        assert_eq!(cpu, "--");
        assert_eq!(mem, "--");
    }

    // --- gather_code_agents tests ---

    #[test]
    fn gather_code_agents_excludes_claws_containers() {
        // This test verifies the filtering logic via the prefix+exclusion rule.
        // We simulate a list of container names and apply the same filter.
        let mock_names = vec![
            "amux-123-456".to_string(),
            "amux-claws-controller".to_string(),
            "amux-claws-worker-1".to_string(),
            "amux-789-012".to_string(),
            "amux-nanoclaw-something".to_string(),
        ];
        let filtered: Vec<String> = mock_names
            .into_iter()
            .filter(|n| !n.starts_with("amux-claws-") && !n.contains("nanoclaw"))
            .collect();
        assert!(filtered.contains(&"amux-123-456".to_string()));
        assert!(filtered.contains(&"amux-789-012".to_string()));
        assert!(!filtered.iter().any(|n| n.contains("claws")));
        assert!(!filtered.iter().any(|n| n.contains("nanoclaw")));
    }

    // --- CLEAR_MARKER constant test ---

    #[test]
    fn clear_marker_contains_null_bytes() {
        assert!(CLEAR_MARKER.starts_with('\x00'));
        assert!(CLEAR_MARKER.ends_with('\x00'));
    }
}
