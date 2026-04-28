# Work Item: Feature

Title: `new` subcommand — interactive workflow and skill creation
Issue: n/a

## Summary:
- Introduce a top-level `new` subcommand that consolidates creation commands: `new spec` (alias for `specs new`), `new workflow`, and `new skill`.
- `new workflow [--interview] [--global] [--format md|toml|yaml]` interactively guides the user through constructing an amux workflow file by prompting for a title and then iteratively adding workflow steps; with `--interview` an agent completes the file based on a user-supplied summary.
- `new skill [--interview] [--global]` guides the user through creating a Claude Code skill file (YAML frontmatter + Markdown content); with `--interview` an agent fills out the skill body.
- When `--global` is passed the output goes to `~/.amux/workflows/<name>` or `~/.amux/skills/<name>/` respectively; when `--global` is combined with `--interview` the new directory is mounted into the agent container using the overlay system (WI-63) rather than mounting the whole repo or home directory.
- Both CLI (stdin prompts) and TUI (dialog with arrow-key navigation, Ctrl-N / Ctrl-Enter shortcuts) are supported, following the existing dialog state-machine pattern.


## User Stories

### User Story 1:
As a: user

I want to:
run `amux new workflow` and be guided step-by-step through naming a workflow and defining its steps (name, agent, model, depends_on, prompt)

So I can:
produce a valid `.toml` (or `.md`/`.yaml`) workflow file without having to remember the schema by heart.

## User Story 2:
As a: user

I want to:
run `amux new workflow --interview` and type a one-paragraph summary of what the workflow should accomplish

So I can:
let a code agent write the detailed workflow file for me, the same way `specs new --interview` writes a work item.

### User Story 3:
As a: user

I want to:
run `amux new workflow --global` (or `new skill --global`) to store the artefact in `~/.amux/workflows/` or `~/.amux/skills/` instead of the current repo

So I can:
maintain a personal library of reusable workflows and skills that travel with me across projects.

### User Story 4:
As a: user

I want to:
run `amux new skill` and be prompted for a skill name, short description, and body text

So I can:
create a Claude Code skill file (YAML frontmatter + Markdown) without copying an existing one and editing it by hand.

### User Story 5:
As a: user

I want to:
run `amux new skill --interview --global` and supply a summary, then have an agent write the skill body to the global skills directory — all without giving the agent access to my whole home directory

So I can:
safely extend my global skill library using an AI agent that only has write access to the new skill directory.

### User Story 6:
As a: user

I want to:
continue using `amux specs new [--interview]` unchanged

So I can:
avoid breaking muscle-memory while the new `new spec` alias becomes established.


## Implementation Details:

### 1. New CLI subcommand: `new` (`src/cli.rs`)

Add a new top-level `Command::New` variant with a `NewAction` sub-enum:

```rust
/// Create a new amux artefact (spec, workflow, or skill).
New {
    #[command(subcommand)]
    action: NewAction,
},
```

```rust
#[derive(Subcommand)]
pub enum NewAction {
    /// Create a new work item spec (alias for `specs new`).
    Spec {
        #[arg(long)]
        interview: bool,
    },

    /// Interactively create a new workflow file.
    Workflow {
        /// Let a code agent complete the workflow from a short summary.
        #[arg(long)]
        interview: bool,

        /// Write to ~/.amux/workflows/<name> instead of the current repo.
        #[arg(long)]
        global: bool,

        /// Output file format.
        #[arg(long, value_enum, default_value = "toml")]
        format: WorkflowFormat,
    },

    /// Interactively create a new skill file.
    Skill {
        /// Let a code agent complete the skill body from a short summary.
        #[arg(long)]
        interview: bool,

        /// Write to ~/.amux/skills/<name>/ instead of the current repo.
        #[arg(long)]
        global: bool,
    },
}

#[derive(Clone, ValueEnum)]
pub enum WorkflowFormat {
    Toml,
    Yaml,
    Md,
}
```

Dispatch in `src/commands/mod.rs` (or `src/main.rs`, following the existing pattern): route `Command::New { action }` to a new `src/commands/new_cmd.rs` module. Keep `Command::Specs` and its existing dispatch unchanged.

### 2. New command module: `src/commands/new_cmd.rs`

This module is the shared entry-point for all three `new` actions and re-exports the shared logic.

```
src/commands/new_cmd.rs
src/commands/new_workflow.rs   ← workflow creation logic
src/commands/new_skill.rs      ← skill creation logic
```

`new spec` simply calls the existing `crate::commands::specs::run_new(interview)` — no new logic.

### 3. Workflow creation: `src/commands/new_workflow.rs`

#### 3a. Data types

```rust
pub struct WorkflowStepInput {
    pub name: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub depends_on: Vec<String>,
    pub prompt: String,
}

pub struct WorkflowInput {
    pub title: String,
    pub steps: Vec<WorkflowStepInput>,
}
```

#### 3b. CLI (stdin) flow

`run_new_workflow_cli(global: bool, format: WorkflowFormat, interview: bool) -> Result<()>`

1. Prompt for workflow name (used as filename and slug, e.g. `my-workflow`).
2. If `interview`:
   - Prompt for a one-line summary (same pattern as `prompt_summary` in `specs.rs`).
   - Resolve destination path (global or repo).
   - Write a skeleton workflow file (title only, no steps) to that path.
   - Launch agent interview (see §3e).
   - Return.
3. Otherwise (interactive step entry):
   - Prompt for workflow title (human-readable, may differ from filename).
   - Loop:
     - Prompt for step name.
     - Prompt for agent (optional, press Enter to skip).
     - Prompt for model (optional, press Enter to skip).
     - Prompt for `depends_on` (optional, comma-separated step names, press Enter to skip).
     - Prompt for prompt text — because this is multi-line on CLI, read lines until the user enters a line containing only `.` (period) on its own, similar to traditional UNIX mail composition. Print a hint: `"Enter prompt text. End with a line containing only '.':"`.
     - Ask: `"Add another step? [y/N]: "`. If `y`, loop; otherwise finish.
   - Serialize to the chosen format and write the file (see §3d).
   - Print the output path.

#### 3c. TUI dialog flow

Add a new `Dialog::NewWorkflow(NewWorkflowDialogState)` variant to `src/tui/state.rs`:

```rust
pub struct NewWorkflowDialogState {
    pub title: String,
    pub title_cursor: usize,
    pub steps: Vec<WorkflowStepInput>,
    // Current step being edited:
    pub step_name: String,
    pub step_name_cursor: usize,
    pub step_agent: String,
    pub step_agent_cursor: usize,
    pub step_model: String,
    pub step_model_cursor: usize,
    pub step_depends_on: String,   // comma-separated; parsed on submit
    pub step_depends_on_cursor: usize,
    pub step_prompt: String,       // multi-line
    pub step_prompt_cursor: usize,
    pub focused_field: WorkflowField,
    pub global: bool,
    pub format: WorkflowFormat,
    pub interview: bool,
    pub error: Option<String>,
}

pub enum WorkflowField {
    Title,
    StepName,
    StepAgent,
    StepModel,
    StepDependsOn,
    StepPrompt,
}
```

**Key bindings in `NewWorkflow` dialog:**
- `Tab` / `Shift-Tab`: cycle through fields (Title → StepName → StepAgent → StepModel → StepDependsOn → StepPrompt → StepName…).
- Arrow keys in single-line fields: move cursor left/right.
- Arrow keys in `StepPrompt`: move cursor within multi-line text (Up/Down move between lines; Left/Right move within a line).
- `Ctrl-N`: commit the current step (validate that `step_name` is non-empty) and reset step fields to blank, keeping `Title` and accumulated `steps`.
- `Ctrl-Enter`: finish the workflow — commit the current step (if non-empty), serialize, write file, close dialog.
- `Esc`: cancel without writing.

**Rendering** (`src/tui/render.rs`): Draw a bordered popup containing:
- A single-line text box for "Workflow title" (visible only when `focused_field == Title`; after the title is set, collapse it to a label).
- Labelled single-line text boxes for StepName, StepAgent, StepModel, StepDependsOn.
- A multi-line text box for StepPrompt (same widget as `NewInterviewSummary`, see `src/tui/state.rs:158-166`).
- A footer showing `[Tab] next field  [Ctrl-N] add step  [Ctrl-Enter] finish  [Esc] cancel`.
- A breadcrumb showing steps already committed: `Steps: impl → tests → docs`.

When `interview == true` the dialog only shows:
- Workflow name (single-line).
- Summary (multi-line, same as `NewInterviewSummary`).
- Footer: `[Ctrl-Enter] start interview  [Esc] cancel`.

#### 3d. Serialisation

`write_workflow_file(input: &WorkflowInput, dest: &Path, format: WorkflowFormat) -> Result<()>`

- `WorkflowFormat::Toml`: write TOML using the `toml` crate, following the existing `aspec/workflows/implement-preplanned.toml` schema (`title = "..."`, `[[step]]` tables with `name`, `depends_on`, `agent`, `model`, `prompt` fields).
- `WorkflowFormat::Yaml`: write YAML using `serde_yaml`; mirror the existing `.yaml` workflow schema.
- `WorkflowFormat::Md`: write Markdown; mirror the existing `.md` workflow schema (`# Title`, `## Step: name`, `Depends-on:`, `Agent:`, `Model:`, `Prompt:` paragraph).
- Optional fields (`agent`, `model`, `depends_on`) are omitted when empty/None.

**Destination path resolution** (`resolve_workflow_dest(name: &str, global: bool, format: WorkflowFormat, git_root: Option<&Path>) -> Result<PathBuf>`):
- `global == true`: `~/.amux/workflows/<name>.<ext>`; create `~/.amux/workflows/` if absent.
- `global == false`: `<git_root>/aspec/workflows/<name>.<ext>`.
- If the file already exists, bail with an error (`"Workflow '{name}' already exists at {path}"`).

#### 3e. Interview mode for `new workflow`

`WORKFLOW_INTERVIEW_PROMPT_TEMPLATE`:

```
Workflow file {filename} has been created at {path}. Help complete the workflow based on the following summary. 
The workflow should include all necessary steps with clear step names, explicit depends_on relationships, 
appropriate agent and model choices where relevant, and detailed, actionable prompts for each step. 
Only edit the workflow file. Do not create or edit any other files. Follow the file format already present in 
the skeleton. Do not summarize your work at the end — let the user review the file themselves.

Summary:
{summary}
```

Substitutions: `{filename}` (basename), `{path}` (absolute path inside the container), `{summary}`.

Agent entrypoint builders follow the same pattern as `interview_agent_entrypoint` in `specs.rs`:

```rust
pub fn workflow_interview_agent_entrypoint(agent: &str, path: &str, filename: &str, summary: &str) -> Vec<String>
pub fn workflow_interview_agent_entrypoint_non_interactive(...) -> Vec<String>
```

**Mount strategy (`--global` + `--interview`):**
- When `global == false`: mount the git root (existing behaviour, same as `specs new --interview`).
- When `global == true`: do **not** mount the git root. Instead, construct a `DirectoryOverlay`:
  ```
  host_path:      ~/.amux/workflows/          (the directory containing the new file)
  container_path: /var/amux-workflows/
  permission:     ReadWrite
  ```
  Pass this as the sole overlay to `run_agent_with_sink`. The `mount_path` argument should be `None`; the overlay is sufficient. Update the prompt's `{path}` substitution to use `/var/amux-workflows/<filename>`.
- The agent only has write access to the new workflows directory — not the whole home directory, not any repo.

### 4. Skill creation: `src/commands/new_skill.rs`

#### 4a. Skill file format

Skills follow the YAML-frontmatter + Markdown format from `.claude/skills/`:

```markdown
---
name: <slug>
description: <one-line description>
---

# <Title>

<body>
```

The `name` field is the slug (kebab-case); `description` is a single sentence; the body is free-form Markdown.

Global skill location: `~/.amux/skills/<name>/SKILL.md` (create `~/.amux/skills/<name>/` if absent).
Repo skill location: `.claude/skills/<name>/SKILL.md` (create parent if absent).

#### 4b. CLI flow

`run_new_skill_cli(global: bool, interview: bool) -> Result<()>`

1. Prompt for skill name (slug, e.g. `my-skill`).
2. Prompt for short description (single line).
3. If `interview`:
   - Prompt for summary (single line, same pattern as `prompt_summary`).
   - Resolve destination directory and write skeleton `SKILL.md`:
     ```markdown
     ---
     name: {name}
     description: {description}
     ---

     # {Title}

     <!-- Agent will complete this file -->
     ```
   - Launch agent interview (see §4d).
   - Return.
4. Otherwise:
   - Prompt for body text — same multi-line period-termination convention as workflow CLI (`"Enter skill body. End with a line containing only '.':" `).
   - Write full `SKILL.md`.
   - Print output path.

#### 4c. TUI dialog

Add `Dialog::NewSkill(NewSkillDialogState)` to `src/tui/state.rs`:

```rust
pub struct NewSkillDialogState {
    pub name: String,
    pub name_cursor: usize,
    pub description: String,
    pub description_cursor: usize,
    pub body: String,               // multi-line
    pub body_cursor: usize,
    pub focused_field: SkillField,
    pub global: bool,
    pub interview: bool,
    pub error: Option<String>,
}

pub enum SkillField {
    Name,
    Description,
    Body,
}
```

**Key bindings:**
- `Tab` / `Shift-Tab`: cycle Name → Description → Body.
- Arrow keys: navigate within the focused field (multi-line for Body).
- `Ctrl-Enter`: write file (or start interview if `interview == true`) and close.
- `Esc`: cancel.

**Rendering:** Bordered popup with three fields (Name single-line, Description single-line, Body multi-line). Footer: `[Tab] next field  [Ctrl-Enter] finish  [Esc] cancel`. When `interview == true`, replace Body with a Summary multi-line field (same as `NewInterviewSummary`).

#### 4d. Interview mode for `new skill`

`SKILL_INTERVIEW_PROMPT_TEMPLATE`:

```
A skill file has been created at {path}. Help complete the skill based on the following summary. 
The skill should include clear instructions that a code agent can follow step-by-step, with any 
relevant commands, code examples, or decision trees needed. Write the skill in the second person 
imperative ("Run ...", "Check ...", "If ... then ..."). Only edit the skill file at {path}. 
Do not create or edit any other files. Follow the YAML frontmatter already present in the skeleton. 
Do not summarize your work at the end — let the user review the file themselves.

Summary:
{summary}
```

Substitutions: `{path}` (absolute path inside the container), `{summary}`.

Agent entrypoint builders:

```rust
pub fn skill_interview_agent_entrypoint(agent: &str, path: &str, summary: &str) -> Vec<String>
pub fn skill_interview_agent_entrypoint_non_interactive(...) -> Vec<String>
```

**Mount strategy (`--global` + `--interview`):**
- `global == false`: mount the git root (existing behaviour).
- `global == true`: construct a `DirectoryOverlay`:
  ```
  host_path:      ~/.amux/skills/<name>/      (the new skill's directory)
  container_path: /var/amux-skills-<name>/
  permission:     ReadWrite
  ```
  Pass as the sole overlay; `mount_path` is `None`. Update `{path}` in the prompt to `/var/amux-skills-<name>/SKILL.md`.
- The agent only has write access to the single skill directory.

### 5. Shared helpers

**Global directory helpers** (`src/config/mod.rs` or `src/commands/new_cmd.rs`):

```rust
pub fn global_workflows_dir() -> Result<PathBuf>   // ~/.amux/workflows/
pub fn global_skills_dir() -> Result<PathBuf>       // ~/.amux/skills/
```

Both use `dirs::home_dir()` (already a transitive dependency via WI-63) and create the directory with `std::fs::create_dir_all` if absent.

**Overlay construction for global interview** (reuse `DirectoryOverlay` from `src/overlays/directory.rs`):

```rust
pub fn overlay_for_global_dir(host_dir: &Path, container_path: &str) -> DirectoryOverlay {
    DirectoryOverlay {
        host_path: host_dir.to_path_buf(),
        container_path: PathBuf::from(container_path),
        permission: MountPermission::ReadWrite,
    }
}
```

### 6. `src/commands/mod.rs` dispatch

Add arms to the existing dispatch match:

```rust
Command::New { action } => match action {
    NewAction::Spec { interview } => specs::run_new(interview).await,
    NewAction::Workflow { interview, global, format } =>
        new_workflow::run_new_workflow(interview, global, format).await,
    NewAction::Skill { interview, global } =>
        new_skill::run_new_skill(interview, global).await,
},
```

### 7. TUI integration

In `src/tui/mod.rs`, add handling for the new dialog states in:
- The key event loop: map `Ctrl-N`, `Ctrl-Enter`, `Tab`/`Shift-Tab`, arrow keys, and text input for `Dialog::NewWorkflow` and `Dialog::NewSkill`.
- The async action dispatcher: when the user confirms (`Ctrl-Enter`) in `NewWorkflow` or `NewSkill`, spawn the write (and optional agent launch) as a background task, exactly as done for `NewInterviewSummary` in the existing `specs new` TUI flow.

In `src/tui/render.rs`, add rendering for `Dialog::NewWorkflow` and `Dialog::NewSkill`. Reuse the existing multi-line text box widget (the same one rendered for `NewInterviewSummary`) for the `prompt` (workflow) and `body` / `summary` (skill) fields.

### 8. `specs new` compatibility

`Command::Specs { action: SpecsAction::New { interview } }` must continue to work exactly as before. No changes to `src/commands/specs.rs`.


## Edge Case Considerations:

- **Name validation**: Skill and workflow names must be non-empty, contain only alphanumeric characters, hyphens, and underscores, and must not contain path separators. Reject invalid names with a descriptive error before any file I/O.
- **File already exists**: If the resolved destination path already exists, bail with an error rather than overwriting silently. Print the existing path so the user can inspect or delete it.
- **Global directory creation**: `~/.amux/workflows/` and `~/.amux/skills/` may not yet exist; create them with `create_dir_all` before writing. Do not error if they already exist.
- **No git root for non-global mode**: If the user is not inside a git repository and `--global` is not passed, bail with a clear message ("Not inside a git repository. Use --global to write to ~/.amux/").
- **`--interview` without an agent configured**: If no agent is configured and none can be inferred, fall back to `"claude"` (matching existing `specs new` behaviour).
- **`--global --interview` overlay mount**: The skill/workflow directory must exist (and contain the skeleton file) before the agent is launched. Create it synchronously before calling `run_agent_with_sink`.
- **Multi-line period termination (CLI)**: If the user types a prompt/body that is legitimately empty (only `.`), treat it as an empty string and emit a warning ("Prompt is empty. Continuing with empty prompt.") rather than erroring.
- **TUI `Ctrl-N` with empty step name**: Validate that `step_name` is non-empty before committing a step; show an inline error in the dialog (`error: Some("Step name cannot be empty")`) and keep the dialog open.
- **TUI `Ctrl-Enter` with no steps**: If the user presses `Ctrl-Enter` on the workflow dialog without adding any steps, show an inline error ("At least one step is required") and keep the dialog open.
- **Format extension mapping**: `--format toml` → `.toml`, `--format yaml` → `.yaml`, `--format md` → `.md`. The YAML writer uses `serde_yaml`; confirm this crate is already in `Cargo.toml` (it is used by the workflow parser).
- **`depends_on` referencing non-existent steps (CLI/TUI interactive)**: Warn the user but do not block completion — the workflow file may reference steps that will be added later or renamed.
- **Container path collisions (global interview)**: `/var/amux-workflows/` and `/var/amux-skills-<name>/` must not conflict with any existing container paths. These are chosen to avoid collisions with `/workspace` (git root mount) and `/root` (home).


## Test Considerations:

### Unit tests

- **`src/commands/new_workflow.rs`**:
  - `write_workflow_file` serialises a two-step `WorkflowInput` correctly to TOML, YAML, and Markdown. Verify field presence and absence for optional fields.
  - `resolve_workflow_dest` returns correct paths for `global == true` (uses `~/.amux/workflows/`) and `global == false` (uses git root).
  - `workflow_interview_agent_entrypoint` substitutes `{filename}`, `{path}`, and `{summary}` correctly for `claude`, `codex`, and `opencode` agents.
  - Name validation rejects names with spaces, path separators, and empty strings.

- **`src/commands/new_skill.rs`**:
  - `write_skill_file` produces correct YAML frontmatter + Markdown body.
  - `skill_interview_agent_entrypoint` substitutes `{path}` and `{summary}` correctly for all three agents.
  - Skeleton file written in interview mode contains correct frontmatter but placeholder body.

- **Overlay construction**:
  - `overlay_for_global_dir` produces a `DirectoryOverlay` with `ReadWrite` permission and correct paths.
  - For `--global --interview` workflow: overlay `host_path` equals `~/.amux/workflows/`, `container_path` equals `/var/amux-workflows/`.
  - For `--global --interview` skill named `"foo"`: overlay `host_path` equals `~/.amux/skills/foo/`, `container_path` equals `/var/amux-skills-foo/`.

### Integration tests

- `amux new spec` invokes the same logic as `amux specs new` (including `--interview`). Verify by running both and comparing side-effects in a temp directory.
- `amux new workflow` (non-interview) with stdin simulation writes a valid TOML file to `aspec/workflows/` in the test repo.
- `amux new workflow --format yaml` writes a `.yaml` file.
- `amux new workflow --global` writes to a temp `~/.amux/workflows/` directory (use env override in test).
- `amux new workflow --interview --global` writes a skeleton file and constructs the correct `DirectoryOverlay` (verify by inspecting `run_agent_with_sink` arguments).
- `amux new skill` (non-interview) writes a valid `SKILL.md` to `.claude/skills/<name>/`.
- `amux new skill --global` writes to `~/.amux/skills/<name>/SKILL.md`.
- `amux new skill --interview --global` writes skeleton and constructs overlay with `host_path = ~/.amux/skills/<name>/` and `container_path = /var/amux-skills-<name>/`.

### TUI dialog tests (`src/tui/`)

- `NewWorkflowDialogState`: Tab cycles fields in order. `Ctrl-N` with non-empty step name appends to `steps` and resets step fields. `Ctrl-N` with empty step name sets `error`. `Ctrl-Enter` with at least one step closes dialog and triggers write. `Ctrl-Enter` with zero steps sets `error`.
- `NewSkillDialogState`: Tab cycles Name → Description → Body. `Ctrl-Enter` with non-empty name and description closes dialog.

### End-to-end

- Run `amux new workflow` in a test repo with simulated stdin; verify the output file passes `amux exec workflow <path>` (parses without error).
- Run `amux new skill` in a test repo; verify the resulting `SKILL.md` has valid YAML frontmatter parseable by a YAML parser.
- `amux specs new` and `amux new spec` produce identical skeleton files in a fresh test repo (same numbering, same template content).


## Codebase Integration:

- Follow established conventions, best practices, testing, and architecture patterns from the project's `aspec/`.
- New modules (`new_cmd.rs`, `new_workflow.rs`, `new_skill.rs`) go in `src/commands/`; declare them in `src/commands/mod.rs`.
- Reuse `prompt_kind`, `prompt_title`, `create_file_return_number` from `src/commands/new.rs` for the `new spec` alias; do not duplicate logic.
- Reuse the existing `run_agent_with_sink` from `src/commands/agent.rs` for all interview modes — same signature, same container lifecycle.
- Reuse `DirectoryOverlay` and `MountPermission` from `src/overlays/directory.rs` (introduced in WI-63) for the `--global --interview` overlay construction.
- Reuse `dirs::home_dir()` (transitive dependency confirmed in WI-63) for `~/.amux/` path resolution.
- The `serde_yaml` crate is already a dependency (used in `src/workflow/parser.rs`); use it for YAML output without adding a new dependency.
- The `toml` crate is already a dependency; use it for TOML output.
- New dialog state variants (`Dialog::NewWorkflow`, `Dialog::NewSkill`) in `src/tui/state.rs` must follow the same naming and field conventions as adjacent variants (`NewTitleInput`, `NewInterviewSummary`, etc.).
- Multi-line text box rendering must reuse the existing widget used for `NewInterviewSummary` — do not write a second implementation.
- All prompt strings (interview templates) live as `const &str` at the top of their respective command modules, following the `INTERVIEW_PROMPT_TEMPLATE` / `AMEND_PROMPT_TEMPLATE` convention in `specs.rs`.
- `tracing::warn!` (not `eprintln!`) for any non-fatal runtime warnings.
- Global directory creation uses `std::fs::create_dir_all`; call it unconditionally (idempotent).
- The `--format` flag uses a `clap::ValueEnum` so that `--help` lists valid values automatically; mirror `Agent` and other enums in `src/cli.rs`.
