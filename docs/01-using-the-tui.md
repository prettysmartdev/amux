# Using the TUI

amux has two execution modes:

- **TUI mode** — run `amux` with no arguments to open the interactive terminal UI. This is the primary interface for ongoing agent work: it supports multiple simultaneous sessions, live tab state, and a full in-process terminal emulator for agent output.
- **Command mode** — run `amux <subcommand>` directly from your shell. It executes the command and exits. Useful for scripting, CI, or quick one-off actions.

This guide covers TUI mode.

---

## Layout

```
┌─ Tab 1: myproject ─────────┬─ Tab 2: myproject ──────────┐
│  implement 0001             │  chat                        │
└─────────────────────────────┴──────────────────────────────┘
┌─── ● running: implement 0001 ──────────────────────────────┐
│ $ docker run --rm -it ...                                   │
│                                                             │
│  ╭─ 🔒 Claude Code (containerized) ── myproj | 5% | 200mb ─╮│
│  │                                                          ││
│  │  [agent output here]                                     ││
│  │                                                          ││
│  ╰──────────────────────────────────────────────────────────╯│
│                                                             │
│  Press Esc to minimize the container window                 │
└─────────────────────────────────────────────────────────────┘
┌─── command ──────────────────────────────────────────────────┐
│ > _                                                           │
└───────────────────────────────────────────────────────────────┘
  init  ·  ready  ·  implement  ·  chat  ·  specs
```

The TUI is composed of three areas:

- **Tab bar** (top) — one entry per open session, with colour-coded state
- **Execution window** (middle) — shows command output; overlaid by the container window when an agent is running
- **Command box** (bottom) — where you type subcommands

---

## The command box

The command box is where you interact with amux. Type any subcommand and press **Enter**.

| Key | Action |
|-----|--------|
| Type | Update input; suggestions appear below |
| **Enter** | Execute command |
| **Shift+Enter** | Insert a newline (multi-line input) |
| **← / →** | Move cursor within input |
| **↑** | Focus the execution window (for scrolling) |
| **Backspace / Delete** | Edit input |
| **q** (empty input) | Open quit confirmation |
| **Ctrl+C** | Open quit confirmation |

### Autocomplete

As you type, amux shows matching suggestions below the command box:

```
implement --
  implement <NNNN>  e.g. implement 0001
  implement <NNNN> --agent <NAME>  — override configured agent
  implement <NNNN> --non-interactive  — run without interactive prompt
  implement <NNNN> --plan  — plan mode
  implement <NNNN> --worktree  — use git worktree
  implement <NNNN> --yolo  — skip confirmation prompts
  implement <NNNN> --yolo --workflow <FILE>  — workflow file path
```

Every flag available in `amux implement` and `amux chat` is also available in
the TUI command box and appears in autocomplete. Both `--flag value` and
`--flag=value` forms are accepted. For example:

```
chat --agent codex
chat --agent=codex
implement 0042 --agent opencode --plan
```

If you type an unrecognised command, amux suggests the closest known one:

```
'implemnt' is not an amux command.  Did you mean: implement
```

### Quitting

Press **q** or **Ctrl+C** from the command box to open the quit confirmation:

```
╭─── Quit amux? ───────────────────╮
│  Are you sure you want to quit?   │
│  [y/n]                            │
╰───────────────────────────────────╯
```

Press **y** to quit, **n** or **Esc** to cancel.

---

## The execution window

The execution window shows plain-text streaming output from commands — Docker build logs, status messages, error output. It is separate from the container window (see below).

### Scrolling

When the window is selected (press **↑** from the command box to select it):

| Key / Action | Effect |
|---|---|
| **↑ / ↓** | Scroll line by line |
| **b / e** | Jump to beginning / end |
| Mouse scroll | Scroll at any time |
| **Esc** | Return focus to command box |

### Border colours

| Colour | Meaning |
|--------|---------|
| Blue | Running (selected) |
| Grey | Running (unselected) or idle |
| Green | Completed successfully |
| Red | Completed with error |

---

## The container window

Whenever amux launches a container to run a code agent, a **container window** appears overlaying the execution window. This window contains a full terminal emulator — all keyboard input, ANSI colour codes, cursor movement, and interactive TUI apps (like Claude Code's own UI) work exactly as they would in a real terminal.

```
╭─ 🔒 Claude Code (containerized) ── myproject | 5% | 200mb ──╮
│                                                               │
│  [agent output — full terminal emulation]                    │
│                                                               │
╰───────────────────────────────────────────────────────────────╯
  Esc minimize  ·  scroll ↕ history  ·  drag select  ·  Ctrl+Y copy
```

The title bar shows the container name, live CPU usage, memory, and total runtime. Stats are polled from the container runtime every 5 seconds.

### Keyboard and mouse

When the container window is visible and maximized, all keyboard input is forwarded to the agent:

| Key / Action | Effect |
|---|---|
| Type | Sent directly to the agent |
| **Esc** | Minimize the container window (agent keeps running) |
| Mouse scroll | Scroll terminal scrollback (5 lines per tick) |
| Mouse drag | Select text (highlighted with inverted colours) |
| **Ctrl+Y** | Copy the current selection to clipboard (ANSI stripped) |

Scrollback holds up to 10,000 lines by default. While scrolled, the title bar shows `↑ scrollback (N / M lines)` where `N` is your current offset and `M` is the total depth. Scroll back to the bottom to return to the live view.

**Ctrl+Y** with no active selection forwards the key to the agent instead of copying.

### Minimizing and restoring

Press **Esc** to minimize the container window. The agent keeps running. The window collapses to a 1-line status bar:

```
─ 🔒 claude | myproject | 5% | 200mb | 1m 23s ─────────────────
```

From the minimized state:

| Key | Effect |
|-----|--------|
| **c** | Restore the container window |
| **↑ / ↓** | Scroll the execution window (behind the status bar) |
| **b / e** | Jump to beginning / end of execution window |
| **Esc** | Return focus to command box |

### When the container exits

The container window closes and a summary bar appears:

```
── claude · myproject-12345 · avg CPU 4.2% · 210MiB · 1m 47s · exit 0 ──
```

This summary persists until a new container is launched.

---

## Config dialog

Type `config show` in the command box and press **Enter** to open the config dialog — a large centered modal overlay that lets you view and edit all configuration fields without leaving the TUI.

```
╭─── Configuration ────────────────────────────────────────────────────────╮
│                                                                            │
│  Field                       Global              Repo        Effective     │
│ ─────────────────────────────────────────────────────────────────────────  │
│  default_agent               claude (built-in)   N/A         claude        │
│  runtime                     docker (built-in)   N/A         docker        │
│▶ terminal_scrollback_lines   10000 (built-in)    5000        5000          │
│  yolo_disallowed_tools       (empty)             (not set)   (empty)       │
│  env_passthrough             (empty)             (not set)   (empty)       │
│  agent                       N/A                 codex       codex         │
│  auto_agent_auth_accepted    N/A                 true        true          │
│                                                                            │
│  Accepted values: positive integer                                         │
│                                                                            │
│  ↑↓ navigate · e edit · Ctrl+Enter save · Esc close                       │
╰────────────────────────────────────────────────────────────────────────────╯
```

### Navigation and editing

| Key | Action |
|-----|--------|
| **↑ / ↓** | Move between rows |
| **← / →** | Move between columns (Global, Repo, Effective) |
| **e** | Enter edit mode for the selected field |
| **Enter** (edit mode) | Confirm the new value and exit edit mode |
| **Esc** (edit mode) | Cancel edit without saving |
| **Ctrl+Enter** | Save all pending changes to the appropriate config files |
| **Esc** (navigation) | Close the dialog and return to the previous view |

When a row is selected, a hint line below the table shows the accepted values for that field (e.g. `claude | codex | opencode | maki | gemini`).

Fields marked `(read-only)` — such as `auto_agent_auth_accepted` — are skipped during navigation for edit purposes. Their values are shown but cannot be changed from this dialog.

### Scope and saving

The dialog loads both config files when it opens. Each edit targets the repo config by default; global-only fields (like `runtime` and `default_agent`) write to the global config. Changes are not written to disk until you press **Ctrl+Enter**. Pressing **Esc** without saving discards all edits made in this session.

---

## Multi-tab support

Press **Ctrl+T** to open a new tab. Each tab has its own working directory, execution window, and container session. Tabs run independently in the background when you switch away.

```
Ctrl+T          open a new tab (prompts for working directory)
Ctrl+A          switch to the previous tab
Ctrl+D          switch to the next tab
Ctrl+C, Ctrl+T  (multiple tabs open) close current tab
```

The tab bar shows each tab's project name, current or last command, and an arrow (`➡`) on the active tab. The active tab's bottom border is suppressed so it visually opens into the content area.

### Tab colours

| Colour | Meaning |
|--------|---------|
| Grey | Idle or completed |
| Blue | Running (no container) |
| Green | Running with active container |
| Purple / Magenta | Running a claws (nanoclaw) session |
| Red | Exited with error |
| Yellow | Container silent for >10 seconds (stuck warning) |
| Alternating Yellow / Purple | Background yolo countdown in progress (see [Yolo Mode](05-yolo-mode.md#background-yolo-countdown)) |

### Stuck detection

If a running container produces no output for more than 10 seconds, the tab turns yellow and the subcommand label gains a `⚠️` prefix (e.g. `⚠️ implement 0001`). The warning clears automatically when you:

- Switch to the yellow tab
- Press any key while the tab is active
- Scroll with the mouse wheel

**Active-tab suppression:** On the currently active tab, any keypress or mouse scroll also resets the stuck timer directly. If you are actively reading or scrolling through output, the tab will not turn yellow or show any stuck indicator — the timer only starts when both the container and the user have been idle for 10 seconds. Background tabs are not affected by this; they use output time alone to determine stuck state.

For workflow tabs, amux goes further: the [workflow control board](04-workflows.md#workflow-control-board) opens automatically so you can act without having to notice the yellow indicator. In yolo mode, background tabs show a live countdown directly in the tab bar instead of a dialog. See [Workflows](04-workflows.md) and [Yolo Mode](05-yolo-mode.md) for details.

---

## Reference: all keyboard shortcuts

| Key | Context | Action |
|-----|---------|--------|
| **Ctrl+T** | Anywhere | Open new tab |
| **Ctrl+A** | Anywhere | Switch to previous tab |
| **Ctrl+D** | Anywhere | Switch to next tab |
| **Ctrl+A / Ctrl+D** | Yolo countdown dialog | Close dialog and continue countdown in background |
| **Ctrl+C** | Command box, multiple tabs | Close current tab |
| **Ctrl+W** | Workflow running, container minimized | Open workflow control board |
| **Enter** | Command box | Execute command |
| **Shift+Enter** | Command box | Insert newline |
| **↑** | Command box | Focus execution window |
| **q** | Command box (empty) | Quit confirmation |
| **Esc** | Container window maximized | Minimize container window |
| **c** | Container minimized | Restore container window |
| **↑ / ↓** | Execution window selected | Scroll output |
| **b / e** | Execution window selected | Jump to beginning / end |
| **Ctrl+Y** | Container window, text selected | Copy selection to clipboard |
| Mouse scroll | Container window | Scroll scrollback history |
| Mouse drag | Container window | Select text |
| **y / n / Esc** | Quit dialog | Confirm / cancel quit |
| **↑ / ↓** | Config dialog | Navigate between fields |
| **← / →** | Config dialog | Navigate between columns |
| **e** | Config dialog | Enter edit mode for selected field |
| **Enter** | Config dialog (edit mode) | Confirm value and exit edit mode |
| **Esc** | Config dialog (edit mode) | Cancel edit without saving |
| **Ctrl+Enter** | Config dialog | Save all changes to config files |
| **Esc** | Config dialog (navigation) | Close dialog |

---

[← Getting Started](00-getting-started.md) · [Next: Agent Sessions →](02-agent-sessions.md)
