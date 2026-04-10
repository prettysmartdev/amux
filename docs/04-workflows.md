# Workflows

A workflow breaks a large implementation task into discrete phases — for example: plan → implement → review → docs. Each phase runs as its own agent session. You review the output between phases and decide whether to advance, retry, or redirect.

Workflows are plain Markdown files that you write and commit to your repo. amux parses them into an execution plan and runs them inside Docker containers, pausing between steps for your input.

---

## When to use workflows

Single-step `implement` works well for focused, well-specified tasks. Workflows are better when:

- The task is complex enough that you want the agent to plan before coding
- You want multiple review checkpoints (e.g. review the plan before implementation starts)
- You want documentation generated as a separate step after implementation
- You're running in `--yolo` mode and want structured auto-advancement instead of a single long session

---

## Quick start

```sh
# Run the bundled example workflow against work item 0027
amux implement 0027 --workflow aspec/workflows/implement-feature.md
```

The TUI shows a **workflow status strip** between the execution window and the command box, with one coloured box per step. After each step completes, a confirmation dialog appears — press **Enter** to advance, **q** to pause. State is saved to disk so you can resume later.

---

## Workflow file format

A workflow is a plain Markdown file. amux looks for:

| Element | Description |
|---------|-------------|
| `# Title` | Optional heading used for display only |
| `## Step: <name>` | Defines a step; names must be unique within the file |
| `Depends-on: <step>[, <step>…]` | Declares upstream dependencies (omit for root steps) |
| `Prompt:` | Everything after this keyword (until the next heading) is the step's prompt template |

### Example

```markdown
# Implement Feature Workflow

## Step: plan
Prompt: Read the following work item and produce an implementation plan.

{{work_item_content}}

## Step: implement
Depends-on: plan
Prompt: Implement work item {{work_item_number}} according to the plan.

Follow the spec: {{work_item_section:[Implementation Details]}}

## Step: review
Depends-on: implement
Prompt: Review the changes from the implement step for correctness and style.

## Step: docs
Depends-on: implement
Prompt: Write documentation for work item {{work_item_number}}.
```

In this example, `review` and `docs` both depend on `implement` — they form a **parallel group** and are executed sequentially in file order. In the TUI they are rendered stacked vertically with slight indentation.

### Prompt template variables

| Variable | Replaced with |
|----------|--------------|
| `{{work_item_number}}` | Zero-padded four-digit work item number (e.g. `0027`) |
| `{{work_item_content}}` | Full text of the work item Markdown file |
| `{{work_item_section:[Name]}}` | Content of the named section from the work item file (case-insensitive) |

Unknown variables or missing sections are left in place with a warning.

---

## Running a workflow

### In the TUI

```
implement 0027 --workflow=aspec/workflows/implement-feature.md
```

A **workflow status strip** appears, showing each step as a coloured box:

| Colour | Status |
|--------|--------|
| Grey / dim | Pending |
| Blue / bold | Running |
| Green | Done |
| Red / bold | Error |
| Yellow / bold | Stuck (idle for >10 s) |

When a step completes, a confirmation dialog appears. Press **Enter** or **y** to advance, **q** or **Esc** to pause.

### In command mode

```sh
amux implement 0027 --workflow aspec/workflows/implement-feature.md
```

Between steps, amux prints the step summary and prompts:

```
Step 'plan' completed.
Next step(s): implement
Press [Enter] to advance, or [q] to abort:
```

On agent failure:

```
Step 'implement' failed: Container exited with code 1
Press [r] to retry, or any other key to abort:
```

### Flags

All flags available on `implement` work with `--workflow`:

| Flag | Description |
|------|-------------|
| `--non-interactive` | Run each step's agent in print/batch mode |
| `--plan` | Run each step in read-only mode |
| `--allow-docker` | Mount Docker socket into each step's container |
| `--worktree` | Run all steps in an isolated Git worktree |
| `--mount-ssh` | Mount `~/.ssh` into each step's container |
| `--yolo` | Fully autonomous mode; implies `--worktree`; auto-advances stuck steps |

---

## Workflow control board (TUI only)

While a workflow step is **running**, press **Ctrl+W** (with the container window minimized) to open the **workflow control board** — a popup that lets you redirect execution without waiting for the current step to finish.

```
╭──────── Workflow Control ────────╮
│ Step: implement                  │
│                                  │
│         ↑ Restart current step   │
│                                  │
│ ← Cancel to prev   → Next: new   │
│                                  │
│         ↓ Next: same container   │
│                                  │
│  [Arrow] select  [d]isable  [Esc] dismiss  │
╰──────────────────────────────────╯
```

| Key | Action |
|-----|--------|
| **↑** | Restart current step — reset to Pending and relaunch in a fresh container |
| **←** | Cancel to previous step — mark current step Pending and re-run the most recently completed step |
| **→** | Next step: new container — mark current step Done and advance to the next step in a new container |
| **↓** | Next step: same container — mark current step Done and send the next step's prompt to the existing container via PTY |
| **d** | Disable auto-popup for this step — dismiss and suppress auto-open for the remainder of this step |
| **Esc** | Dismiss without changing anything |

Each action persists workflow state before launching any new execution, so an unexpected exit leaves state consistent.

### Next step: same container

The **↓** action reuses the already-running container — the next step's prompt is written directly to its PTY stdin. Useful when the container has already installed dependencies or built artifacts that the next step needs. If the PTY session has closed, amux falls back to a new container and shows a status message.

### Manual vs. automatic opening

Ctrl+W requires:
- A workflow active in the current tab
- A step currently running
- Container window **minimized**
- No other dialog open

When the container is maximized, the hint changes to remind you to minimize first:

```
Press Esc to minimize container, then Ctrl+w for workflow controls
```

---

## Auto-advance when stuck

If a running workflow step produces no output for **10 seconds**, amux automatically opens the workflow control board so you can decide what to do without having to notice the yellow indicator yourself.

The auto-open fires only when:
- The stuck tab is the currently active tab (background tabs are deferred until you switch to them)
- No other dialog is already open
- Auto-open has not been disabled for this step via the **d** key

After you dismiss with **Esc**, the stuck timer resets. If the container stays silent for another 10 seconds, the dialog re-opens. The auto-open works even when the container window is maximized — the dialog appears over the full-screen terminal view.

---

## Workflow state persistence

amux saves workflow state to:

```
$GITROOT/.amux/workflows/<repo-hash8>-<work-item>-<workflow-name>.json
```

The file records the status of every step, the container ID used for each step, and a SHA-256 hash of the workflow file.

### Resuming

If a saved state file exists when you run `implement --workflow`, amux offers to resume:

```
Found a saved workflow state for 'implement-feature' (work item 0027).
  1) Resume from where you left off
  2) Restart from the beginning
  [1/2]:
```

### Workflow file changed

If the workflow file has been modified since the state was saved, amux warns you:

```
WARNING: The workflow file has changed since the last run.
  1) Restart from the beginning
  2) Continue anyway (could be dangerous)
  [1/2]:
```

If you choose `2`, amux verifies that step names and `Depends-on` values are identical. If they differ, it forces a restart.

### Interrupted steps

If a step was running when amux last exited:

```
Step 'implement' was running when the previous session ended.
Start it over (s) or skip to next step (n)? [s/n]:
```

---

## Parallel groups

Steps that share the same `Depends-on` set form a **parallel group**. amux executes them sequentially in file order (true parallel container execution is a future enhancement). In the TUI they are rendered stacked vertically. If a group has more than two steps, the third box shows `+ N more…`.

---

## Bundled example

`aspec/workflows/implement-feature.md` defines a four-step plan → implement → review + docs workflow suitable for most feature work items. Copy and customise it for your project.

---

## Edge cases

| Situation | Behaviour |
|-----------|-----------|
| Cycle in `Depends-on` graph | Error before any agent runs |
| Unknown `Depends-on` step name | Error at parse time |
| Empty workflow file | Rejected with a helpful message |
| Work item file not found | Error before loading the workflow |
| Workflow file not found / unreadable | Clear error with the file path |
| Agent failure mid-workflow | Step marked Error; user prompted to retry or abort |
| Very long step names | Truncated to 12 characters with `…` in the TUI strip |
| Large number of parallel steps | Capped at 3 visible rows; extra shown as `+ N more…` |
| Large number of sequential steps | `+ N more…` box at the far right of the strip |
| **d** pressed; auto-popup suppressed | Auto-open skipped until workflow advances; Ctrl+W still works |
| Container window maximized (manual Ctrl+W) | Ctrl+W suppressed; hint guides you to minimize first |
| Container window maximized (auto-open) | Dialog opens over the maximized terminal; input routes to dialog |
| Another dialog already open | Both Ctrl+W and auto-open suppressed until open dialog is dismissed |
| Step silent on a background tab | Auto-open deferred; control board appears when you switch to that tab |
| Esc dismissed; container still silent | Timer resets; dialog re-opens after another 10 s |
| Output resumes before 10 s threshold | Stuck state clears; auto-open does not trigger |

### Limitations (v0.3)

- **Sequential only**: parallel groups run one step at a time. True concurrent container execution is not yet supported.
- **TUI resume dialogs**: hash-mismatch and resume prompts use auto-restart behaviour rather than a full dialog.

---

[← Security & Isolation](03-security-and-isolation.md) · [Next: Yolo Mode →](05-yolo-mode.md)
