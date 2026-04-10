# Yolo Mode

Yolo mode is amux's fully autonomous operation mode. When `--yolo` is active, the agent skips all permission prompts and proceeds without pausing for confirmation on any action it would normally stop for.

Use it when you want to hand a task to the agent and return to a finished result — no babysitting required.

---

## When to use yolo mode

Yolo mode is appropriate when:

- You have a well-specified work item and trust the agent to implement it correctly
- You're running a multi-step workflow and want it to complete end-to-end with no manual advancement
- You've already reviewed the plan in a `--plan` session and are confident in the approach
- The task is running in an isolated worktree (implied automatically when `--yolo --workflow` are combined), so even if the output isn't ideal it's easy to discard

Yolo mode is **not** appropriate for:

- Tasks where the agent will encounter decisions that genuinely require your input
- Open-ended `chat` sessions where you want ongoing interaction
- Any situation where agent mistakes would be difficult to undo (tip: use `--worktree` to contain the blast radius)

---

## Basic usage

```sh
amux implement 0027 --yolo
amux chat --yolo
```

For the safest yolo experience — fully autonomous, changes isolated to a branch, easy to review or discard:

```sh
amux implement 0027 --yolo --workflow aspec/workflows/implement-feature.md
```

This implies `--worktree` automatically (see below).

---

## What `--yolo` does

### 1. Skips all agent permission prompts

The agent-specific skip-permissions flag is appended to the container entrypoint before launch:

| Agent | Flag appended |
|-------|--------------|
| `claude` | `--dangerously-skip-permissions` |
| `codex` | `--full-auto` |
| `opencode` | *(no equivalent — a warning is printed, flag omitted)* |

### 2. Applies `yoloDisallowedTools`

Any tools listed in `yoloDisallowedTools` in your config are passed to the agent as a deny list. This lets you grant broad autonomy while still preventing specific dangerous operations. See [Disallowed tools](#disallowed-tools) below.

| Agent | Flag used |
|-------|-----------|
| `claude` | `--disallowedTools tool1,tool2,...` |
| `codex` | *(no equivalent — a warning is printed)* |
| `opencode` | *(no equivalent — a warning is printed)* |

### 3. Implies `--worktree` when combined with `--workflow`

When both `--yolo` and `--workflow` are present, amux automatically creates an isolated Git worktree. A message is printed at startup:

```
--yolo with --workflow implies --worktree. Running in isolated worktree.
```

If `--worktree` is also passed explicitly, it is silently accepted — no duplicate worktree is created.

When `--yolo` is used **without** `--workflow`, `--worktree` is **not** implied. The flag only affects permission prompts and disallowed tools. Use `--worktree` explicitly if you want isolation in a single-step yolo run.

### 4. Auto-advances stuck workflow steps

Instead of opening the manual [workflow control board](04-workflows.md#workflow-control-board), amux opens the **yolo countdown dialog** when a workflow step goes silent for 10 seconds:

```
╭─────── Yolo: Auto-Advance ──────────────╮
│ Step: implement                          │
│                                          │
│  No activity detected.                   │
│  Advancing to next step in  47s...       │
│                                          │
│                    [Esc] cancel          │
╰──────────────────────────────────────────╯
```

The countdown runs for **60 seconds**. When it expires:
- If this is not the last step — amux advances to the next step in a new container
- If this is the last step — the workflow transitions to complete

**Cancellation:**
- Any PTY output during the countdown immediately dismisses the dialog — the agent is no longer stuck
- Press **Esc** to dismiss manually; the same 10-second backoff applies before the dialog re-opens

---

## Disallowed tools

Add `yoloDisallowedTools` to your per-repo or global config to restrict which tools the agent may use even under full autonomy:

```json
{
  "yoloDisallowedTools": ["Bash", "computer"]
}
```

This is your safety net for operations you never want the agent to perform autonomously, regardless of how well-specified the task is. Common choices:

- `"Bash"` — prevents arbitrary shell command execution
- `"computer"` — prevents GUI automation

**Config precedence:** per-repo config takes precedence over global config entirely (lists are not merged). To inherit the global list for a specific repo, omit the field from the repo config.

See [Configuration](07-configuration.md) for the full config reference.

---

## Security considerations

- `--yolo` removes the human checkpoints that catch unintended agent actions. Only use it with agents and work items you trust.
- The `yoloDisallowedTools` config provides a floor — operations the agent can never perform autonomously, even with `--yolo`.
- Combine `--yolo` with `--workflow` to get automatic `--worktree` isolation, making it easy to review the full diff before merging into your main branch.
- `--yolo --workflow` is the recommended pattern for long-running autonomous tasks: isolated branch, structured phases, auto-advancing, easy to discard if the output isn't right.

---

## Examples

```sh
# Implement a work item with no prompts, changes in an isolated worktree
amux implement 0027 --yolo --workflow aspec/workflows/implement-feature.md

# Single-step autonomous implementation (no worktree implied — add explicitly if wanted)
amux implement 0027 --yolo

# Single-step autonomous implementation, explicitly isolated
amux implement 0027 --yolo --worktree

# Autonomous chat session with Bash tool blocked
# (add to aspec/.amux.json: "yoloDisallowedTools": ["Bash"])
amux chat --yolo

# Explicit worktree flag with yolo + workflow — identical to omitting it
amux implement 0027 --yolo --worktree --workflow aspec/workflows/implement-feature.md
```

---

[← Workflows](04-workflows.md) · [Next: Nanoclaw →](06-nanoclaw.md)
