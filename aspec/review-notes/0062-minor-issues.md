# Minor / Trivial Issues — Work Item 0062 (copilot, crush, cline agents)

Recorded after parity review (2026-04-27). Critical and medium priority issues were already
fixed during this review session. The items below are minor or trivial and left for a
follow-up review stage.

---

## 1. `exec_prompt_entrypoint_parity_with_chat_non_interactive` test coverage gap

**File:** `src/commands/exec.rs` (unit test at the bottom of the file)
**Severity:** Trivial (by-design asymmetry, not a real bug)

The existing parity test asserts that `chat_entrypoint_with_prompt(agent, prompt, false)`
produces the same result as `chat_entrypoint_non_interactive(agent, false)` with the prompt
appended. It currently only covers `["claude", "codex", "opencode", "maki", "gemini"]`.

The three new agents (copilot, crush, cline) are intentionally excluded because:
- **copilot**: `chat_entrypoint_non_interactive` → `["copilot", "-p"]`; then `chat_entrypoint_with_prompt` → `["copilot", "-p", "-i", prompt]`. The `-i` flag is not simply "append prompt" — the parity assumption breaks.
- **crush**: `chat_entrypoint_non_interactive` → `["crush", "run"]`; `chat_entrypoint_with_prompt` → `["crush", "run", prompt]`. The appending _does_ hold here, but the test would need adjustment to model the append-at-end pattern (not the `-i <prompt>` pattern).
- **cline**: `chat_entrypoint_non_interactive` → `["cline", "task", "--json"]`; `chat_entrypoint_with_prompt` → `["cline", "task", prompt]` (no `--json`). The parity assumption breaks: the non-interactive base path uses `--json` but `chat_entrypoint_with_prompt` does not include `--json` (it expects the caller to append flags separately).

**Recommended fix:** Add a separate, agent-specific assertion block in this test for the three new
agents, documenting the exact mapping for each rather than relying on the generic "append prompt"
invariant. Add a comment in the test explaining why the generic parity check is not used for these
agents.

---

## 2. `dockerfile_for_agent_embedded_does_not_use_npm_install` test lacks exemption rationale

**File:** `src/commands/init_flow.rs` (unit test)
**Severity:** Trivial (doc/comment quality, no behavior impact)

The test exempts `Agent::Gemini`, `Agent::Crush`, and `Agent::Cline` from the "no bare
`npm install` without `-g`" check, since these use `npm install -g` (official global install).
However, the exemption in the test likely does not carry an inline comment explaining *why* these
three agents are exempt while others are not.

**Recommended fix:** Add a comment next to each exempted agent variant:
```rust
// Gemini/Crush/Cline use `npm install -g <pkg>` as the official global install method.
// The -g flag is intentional — this is NOT a local dependency install.
Agent::Gemini | Agent::Crush | Agent::Cline => continue,
```
This makes the exemption self-documenting and prevents future contributors from "fixing" the test
by removing the exemption.

---

## 3. `check_local_agent` for cline omits `--json` flag

**File:** `src/commands/ready.rs` — `check_local_agent` function
**Severity:** Minor (behavioral, low-risk for the version-check path)

`check_local_agent` runs a lightweight version-check command inside the container to verify
the agent is installed. For cline, this runs something like `cline task "<greeting>"` without
the `--json` flag.

Cline enters non-interactive mode automatically when stdout is not a TTY (which it won't be in
a captured container exec), so this mostly works. However, passing `--json` explicitly would be
belt-and-suspenders: it prevents any interactive prompts even on cline versions that do not
auto-detect the non-TTY condition reliably, and produces structured output that is easier to
parse for "did the binary respond at all?" detection.

**Recommended fix:** Update the cline arm in `check_local_agent` to include `--json`:
```rust
// Before:
"cline" => vec!["cline".to_string(), "task".to_string(), greeting],
// After:
"cline" => vec!["cline".to_string(), "task".to_string(), "--json".to_string(), greeting],
```
This is belt-and-suspenders. The existing behavior is not wrong, just slightly fragile against
future cline versions that may change TTY detection behavior.

---

## 4. Crush interactive `chat_entrypoint` uses bare `crush` (no subcommand)

**File:** `src/commands/chat.rs` — `chat_entrypoint`
**Severity:** Trivial (research verification needed, low risk)

The interactive entrypoint for crush is `["crush"]` (bare binary, drops into TUI). This is
correct per the spec and the upstream documentation. However, crush's TUI mode may not behave
identically to the `crush run "<prompt>"` non-interactive mode in terms of context file loading,
skill activation, and session management.

No action needed unless testing reveals a UX inconsistency in the interactive TUI flow.
This is noted as a follow-up validation item, not a code bug.

---

## 5. Copilot telemetry / offline env var not set by default

**File:** `src/passthrough.rs` — `CopilotPassthrough`
**Severity:** Trivial (UX quality, not a correctness issue)

The work item spec (section "Edge Case Considerations") notes that `COPILOT_OFFLINE=true` can
suppress copilot telemetry and restrict network access to configured model providers in container
environments. Currently `CopilotPassthrough` does not set this env var by default.

Setting `COPILOT_OFFLINE=true` by default would reduce unnecessary outbound telemetry from
containers, but may also restrict some copilot features that require network access for model
routing (depending on copilot configuration). The correct default is ambiguous.

**Recommended follow-up:** Document `COPILOT_OFFLINE=true` in `envPassthrough` examples in the
docs, and consider adding an `extra_env_vars()` implementation to `CopilotPassthrough` that
sets it optionally (or conditionally based on a config option). Do not set it unconditionally
without user validation.
