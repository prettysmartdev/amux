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
| `Agent: <name>` | Optional. Run this step with a specific agent instead of the default. Valid values: `claude`, `codex`, `opencode`, `maki`, `gemini` |
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

## Multi-agent workflows

Each step in a workflow can run in a different agent's container by adding an `Agent:` field to the step header block — between `Depends-on:` and `Prompt:`:

```markdown
## Step: plan
Prompt: Produce an implementation plan.

## Step: implement
Depends-on: plan
Agent: codex
Prompt: Implement the plan from the previous step.

## Step: review
Depends-on: implement
Agent: claude
Prompt: Review the implementation for correctness and style.
```

Steps without an `Agent:` field use the workflow default agent — the value from repo config (or global config), overridden by the `--agent` flag if one was passed at the command line. The `--agent` flag sets the **default** for steps that do not name an agent; it does **not** override steps that explicitly specify one.

### Agent pre-flight check

Before the first step runs, amux collects every distinct agent name required across all steps and checks that the corresponding image exists. If an image is missing, amux prompts:

```
Agent 'codex' has no Dockerfile. Download and build it? [y/N]:
```

**Accept** — amux downloads the agent Dockerfile template, builds the project base image (if needed), then builds the agent image. If your repo has multiple agents to set up, each is prompted in turn before the workflow begins.

**Decline** — amux asks whether to substitute the default agent for that step instead:

```
Use the default agent (claude) for steps that specify 'codex'? [y/N]:
```

- Accept the fallback: those steps run with the default agent. The workflow starts normally.
- Decline the fallback: the workflow does not start.

If all required images are already available, the pre-flight check completes silently and the first step launches immediately.

### Field placement

`Agent:` must appear in the step header block — after any `Depends-on:` line and before the `Prompt:` line. An `Agent:` that appears after `Prompt:` is treated as prompt text, not as a directive.

An unknown agent name in an `Agent:` field is caught at parse time, before any container runs, and exits with a list of valid options.

### Resuming workflows with per-step agents

When resuming a saved workflow, the per-step agent assignments from the original run are preserved in the state file. If you pass a different `--agent` flag on resume, amux warns you; the persisted assignments still take precedence.

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
| `--agent=<name>` | Default agent for steps that do not specify an `Agent:` field. Does not override steps with an explicit `Agent:` directive |
| `--non-interactive` | Run each step's agent in print/batch mode |
| `--plan` | Run each step in read-only mode |
| `--allow-docker` | Mount Docker socket into each step's container |
| `--worktree` | Run all steps in an isolated Git worktree |
| `--mount-ssh` | Mount `~/.ssh` into each step's container |
| `--yolo` | Fully autonomous mode; implies `--worktree`; auto-advances stuck steps |

---

## Workflow control board (TUI only)

While a workflow step is **running**, press **Ctrl+W** to open the **workflow control board** — a popup that lets you redirect execution without waiting for the current step to finish. Ctrl+W works regardless of whether the container window is maximized or minimized.

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

If the next step requires a **different agent** than the current step, the **↓** option is unavailable. In the TUI it renders greyed out with the message:

```
Next step uses agent 'codex'; cannot reuse current 'claude' container.
```

In command mode, the "same container" prompt is skipped entirely and the explanation is printed instead. Use **→** (new container) to advance, which always works regardless of agent.

### Manual vs. automatic opening

Ctrl+W requires:
- A workflow active in the current tab
- A step currently running
- No other dialog open

---

## Auto-advance when stuck

If a running workflow step produces no output for **10 seconds**, amux automatically opens the workflow control board so you can decide what to do without having to notice the yellow indicator yourself.

The auto-open fires only when:
- The stuck tab is the currently active tab (background tabs are deferred until you switch to them)
- No other dialog is already open
- Auto-open has not been disabled for this step via the **d** key
- The user has also been idle for 10 seconds on the active tab (see below)

**Active-tab suppression:** If you are actively pressing keys or scrolling on the currently active tab, the stuck timer is held back even if the container is silent. The control board will not open while you are engaged with the output. The timer starts only once both the container and the user have been idle for 10 seconds. Background tabs are always checked using output time alone.

After you dismiss with **Esc**, the stuck timer resets. If the container stays silent for another 10 seconds, the dialog re-opens. The auto-open works even when the container window is maximized — the dialog appears over the full-screen terminal view.

In **yolo mode**, the behavior differs for background tabs: instead of deferring the control board until you switch, a live countdown runs directly in the tab bar. See [Yolo Mode — Background yolo countdown](05-yolo-mode.md#background-yolo-countdown).

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
| Unknown agent name in `Agent:` field | Error at parse time, before any containers run |
| `Agent:` field placed after `Prompt:` | Treated as prompt text, not a directive |
| Missing agent image at workflow start | Pre-flight prompt: build it, fall back to default, or abort |
| Agent Dockerfile download fails during pre-flight | Error surfaced; workflow does not start |
| Agent image build fails during pre-flight | Error surfaced; partial Dockerfile removed; workflow does not start |
| `--agent` flag + step with explicit `Agent:` field | Step's `Agent:` directive wins; `--agent` is only the default for unspecified steps |
| All steps specify non-default agents | Pre-flight still runs for each; default fallback offered only if setup is declined |
| Parallel steps with different agents | Each step runs in its own container — no cross-step sharing |
| Resume with a different `--agent` flag | Warning printed; persisted per-step agent assignments take precedence |
| Current step and next step use the same agent | "Same container" (**↓**) option available as usual |
| Current step and next step use different agents | "Same container" option greyed out (TUI) or skipped (CLI) with explanation |
| Empty workflow file | Rejected with a helpful message |
| Work item file not found | Error before loading the workflow |
| Workflow file not found / unreadable | Clear error with the file path |
| Agent failure mid-workflow | Step marked Error; user prompted to retry or abort |
| Very long step names | Truncated to 12 characters with `…` in the TUI strip |
| Large number of parallel steps | Capped at 3 visible rows; extra shown as `+ N more…` |
| Large number of sequential steps | `+ N more…` box at the far right of the strip |
| **d** pressed; auto-popup suppressed | Auto-open skipped until workflow advances; Ctrl+W still works |
| Container window maximized (auto-open) | Dialog opens over the maximized terminal; input routes to dialog |
| Another dialog already open | Both Ctrl+W and auto-open suppressed until open dialog is dismissed |
| Step silent on a background tab (non-yolo) | Auto-open deferred; control board appears when you switch to that tab |
| Step silent on a background tab (yolo) | Live countdown shown in tab bar; dialog opens when you switch to the tab; workflow auto-advances when countdown expires |
| Esc dismissed; container still silent | Timer resets; dialog re-opens after another 10 s |
| Output resumes before 10 s threshold | Stuck state clears; auto-open does not trigger |
| User actively scrolling on active tab | Stuck timer suppressed; control board does not open while user is engaged |
| User becomes idle after scrolling | Timer starts from idle moment; control board opens after another 10 s of silence |

### Limitations (v0.3)

- **Sequential only**: parallel groups run one step at a time. True concurrent container execution is not yet supported.
- **TUI resume dialogs**: hash-mismatch and resume prompts use auto-restart behaviour rather than a full dialog.

---

[← Security & Isolation](03-security-and-isolation.md) · [Next: Yolo Mode →](05-yolo-mode.md)
