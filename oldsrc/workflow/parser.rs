use anyhow::{bail, Result};
use serde::Deserialize;
use std::path::Path;

/// A single step in a multi-agent workflow.
#[derive(Debug, Clone)]
pub struct WorkflowStep {
    /// Unique name of this step (from `## Step: <name>` heading).
    pub name: String,
    /// Names of steps that must complete before this step can run.
    pub depends_on: Vec<String>,
    /// The raw prompt template string (may contain `{{...}}` variables).
    pub prompt_template: String,
    /// Optional agent override for this step (from `Agent:` field).
    /// When `None`, the workflow default agent (from config or `--agent` flag) is used.
    pub agent: Option<String>,
    /// Optional model override for this step (from `Model:` field).
    /// When `None`, the workflow-level --model flag (if any) is used; if that is also
    /// absent the agent uses its default model.
    pub model: Option<String>,
}

/// Parse a workflow Markdown file into an optional title and ordered list of steps.
///
/// Format:
/// ```markdown
/// # Optional Title
///
/// ## Step: step-name
/// Depends-on: other-step
/// Prompt: The prompt template text, which may span multiple lines.
/// ```
pub fn parse_workflow(content: &str) -> Result<(Option<String>, Vec<WorkflowStep>)> {
    let mut title: Option<String> = None;
    let mut steps: Vec<WorkflowStep> = Vec::new();

    let mut current_name: Option<String> = None;
    let mut current_depends: Vec<String> = Vec::new();
    let mut current_agent: Option<String> = None;
    let mut current_model: Option<String> = None;
    let mut current_body = String::new();
    let mut in_prompt = false;

    for line in content.lines() {
        // Top-level title (only before any step headings).
        if line.starts_with("# ") && title.is_none() && current_name.is_none() {
            title = Some(line[2..].trim().to_string());
            continue;
        }

        // Step heading: flush previous step, start a new one.
        if line.starts_with("## Step:") {
            flush_step(
                &mut steps,
                &mut current_name,
                &mut current_depends,
                &mut current_agent,
                &mut current_model,
                &mut current_body,
                &mut in_prompt,
            );
            let raw = line["## Step:".len()..].trim();
            current_name = Some(raw.to_string());
            continue;
        }

        // Skip other H2 headings (they end the current step).
        if line.starts_with("## ") && current_name.is_some() {
            flush_step(
                &mut steps,
                &mut current_name,
                &mut current_depends,
                &mut current_agent,
                &mut current_model,
                &mut current_body,
                &mut in_prompt,
            );
            continue;
        }

        if current_name.is_some() {
            let trimmed = line.trim();

            // Depends-on field.
            if trimmed.starts_with("Depends-on:") && !in_prompt {
                let deps_str = trimmed["Depends-on:".len()..].trim();
                current_depends = deps_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                continue;
            }

            // Agent: field — optional per-step agent override.
            if trimmed.starts_with("Agent:") && !in_prompt {
                let agent_str = trimmed["Agent:".len()..].trim().to_string();
                if !agent_str.is_empty() {
                    current_agent = Some(agent_str);
                }
                continue;
            }

            // Model: field — optional per-step model override.
            if trimmed.starts_with("Model:") && !in_prompt {
                let model_str = trimmed["Model:".len()..].trim().to_string();
                if !model_str.is_empty() {
                    current_model = Some(model_str);
                }
                continue;
            }

            // Prompt: field — everything after this is the prompt body.
            if (trimmed == "Prompt:" || trimmed.starts_with("Prompt: ")) && !in_prompt {
                in_prompt = true;
                let rest = trimmed["Prompt:".len()..].trim();
                if !rest.is_empty() {
                    current_body.push_str(rest);
                    current_body.push('\n');
                }
                continue;
            }

            if in_prompt {
                current_body.push_str(line);
                current_body.push('\n');
            }
        }
    }

    // Flush the final step.
    flush_step(
        &mut steps,
        &mut current_name,
        &mut current_depends,
        &mut current_agent,
        &mut current_model,
        &mut current_body,
        &mut in_prompt,
    );

    if steps.is_empty() {
        bail!(
            "Workflow file contains no steps. \
             Define steps with '## Step: <name>' headings."
        );
    }

    Ok((title, steps))
}

// ─── Format detection ─────────────────────────────────────────────────────────

/// Supported workflow file formats, detected by file extension.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowFormat {
    Markdown,
    Toml,
    Yaml,
}

/// Detect the workflow format from the file extension.
///
/// Returns an error for unknown or absent extensions.
pub fn detect_format(path: &Path) -> Result<WorkflowFormat> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("md") => Ok(WorkflowFormat::Markdown),
        Some("toml") => Ok(WorkflowFormat::Toml),
        Some("yml") | Some("yaml") => Ok(WorkflowFormat::Yaml),
        _ => bail!("unsupported workflow format: expected .md, .toml, .yml, or .yaml"),
    }
}

// ─── Intermediate serde structs for TOML/YAML ─────────────────────────────────

/// Raw, deserialized representation of a single workflow step.
/// `name` and `prompt` are kept as `Option<String>` so that missing fields can
/// be caught with a descriptive error that includes the step index.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    name: Option<String>,
    prompt: Option<String>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

/// Top-level TOML workflow document.  Steps live under the `[[step]]` array
/// (TOML key `step`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlWorkflow {
    #[serde(default)]
    title: Option<String>,
    /// TOML `[[step]]` arrays map to the key "step".
    #[serde(rename = "step", default)]
    steps: Vec<RawStep>,
}

/// Top-level YAML workflow document.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct YamlWorkflow {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    steps: Vec<RawStep>,
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

/// Strip a UTF-8 BOM (`U+FEFF`) from the start of the string, if present.
fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

/// Validate and convert a `Vec<RawStep>` into `Vec<WorkflowStep>`.
///
/// Errors if the list is empty, or if any step is missing `name` or `prompt`.
fn raw_steps_to_workflow_steps(raw: Vec<RawStep>) -> Result<Vec<WorkflowStep>> {
    if raw.is_empty() {
        bail!("workflow file contains no steps");
    }
    let mut steps = Vec::with_capacity(raw.len());
    for (i, r) in raw.into_iter().enumerate() {
        let name = r.name.ok_or_else(|| {
            anyhow::anyhow!("step at index {} is missing the required 'name' field", i)
        })?;
        let prompt = r.prompt.ok_or_else(|| {
            anyhow::anyhow!(
                "step '{}' (index {}) is missing the required 'prompt' field",
                name,
                i
            )
        })?;
        steps.push(WorkflowStep {
            name,
            depends_on: r.depends_on,
            prompt_template: prompt.trim().to_string(),
            agent: r.agent,
            model: r.model,
        });
    }
    Ok(steps)
}

// ─── TOML parser ─────────────────────────────────────────────────────────────

/// Parse a TOML workflow file into an optional title and ordered list of steps.
pub fn parse_workflow_toml(content: &str) -> Result<(Option<String>, Vec<WorkflowStep>)> {
    let content = strip_bom(content);
    let workflow: TomlWorkflow =
        toml::from_str(content).map_err(|e| anyhow::anyhow!("TOML parse error: {}", e))?;
    let steps = raw_steps_to_workflow_steps(workflow.steps)?;
    Ok((workflow.title, steps))
}

// ─── YAML parser ─────────────────────────────────────────────────────────────

/// Parse a YAML workflow file into an optional title and ordered list of steps.
pub fn parse_workflow_yaml(content: &str) -> Result<(Option<String>, Vec<WorkflowStep>)> {
    let content = strip_bom(content);
    let workflow: YamlWorkflow =
        serde_yaml::from_str(content).map_err(|e| anyhow::anyhow!("YAML parse error: {}", e))?;
    let steps = raw_steps_to_workflow_steps(workflow.steps)?;
    Ok((workflow.title, steps))
}

fn flush_step(
    steps: &mut Vec<WorkflowStep>,
    current_name: &mut Option<String>,
    current_depends: &mut Vec<String>,
    current_agent: &mut Option<String>,
    current_model: &mut Option<String>,
    current_body: &mut String,
    in_prompt: &mut bool,
) {
    if let Some(name) = current_name.take() {
        steps.push(WorkflowStep {
            name,
            depends_on: std::mem::take(current_depends),
            agent: current_agent.take(),
            model: current_model.take(),
            prompt_template: current_body.trim().to_string(),
        });
        current_body.clear();
        *in_prompt = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_step_no_deps() {
        let md = "## Step: plan\nPrompt: Do the thing.\n";
        let (title, steps) = parse_workflow(md).unwrap();
        assert!(title.is_none());
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].name, "plan");
        assert!(steps[0].depends_on.is_empty());
        assert_eq!(steps[0].prompt_template, "Do the thing.");
    }

    #[test]
    fn parse_title_and_steps() {
        let md = "# My Workflow\n\n## Step: plan\nPrompt: Plan it.\n\n## Step: implement\nDepends-on: plan\nPrompt: Implement it.\n";
        let (title, steps) = parse_workflow(md).unwrap();
        assert_eq!(title.as_deref(), Some("My Workflow"));
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1].depends_on, vec!["plan"]);
    }

    #[test]
    fn parse_missing_depends_on_gives_empty_vec() {
        let md = "## Step: solo\nPrompt: Do stuff.\n";
        let (_title, steps) = parse_workflow(md).unwrap();
        assert!(steps[0].depends_on.is_empty());
    }

    #[test]
    fn parse_multiple_depends_on() {
        let md = "## Step: merge\nDepends-on: plan, implement\nPrompt: Merge.\n";
        let (_title, steps) = parse_workflow(md).unwrap();
        assert_eq!(steps[0].depends_on, vec!["plan", "implement"]);
    }

    #[test]
    fn parse_empty_file_returns_error() {
        let result = parse_workflow("# Title only\n");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("no steps"));
    }

    #[test]
    fn parse_multiline_prompt() {
        let md = "## Step: plan\nPrompt: Line one.\nLine two.\nLine three.\n";
        let (_title, steps) = parse_workflow(md).unwrap();
        let prompt = &steps[0].prompt_template;
        assert!(prompt.contains("Line one."));
        assert!(prompt.contains("Line two."));
        assert!(prompt.contains("Line three."));
    }

    #[test]
    fn parse_prompt_with_inline_content() {
        let md = "## Step: plan\nPrompt: Do this: {{work_item_content}}\n";
        let (_title, steps) = parse_workflow(md).unwrap();
        assert!(steps[0].prompt_template.contains("{{work_item_content}}"));
    }

    #[test]
    fn parse_multiple_steps_preserves_order() {
        let md = "## Step: a\nPrompt: A.\n\n## Step: b\nDepends-on: a\nPrompt: B.\n\n## Step: c\nDepends-on: b\nPrompt: C.\n";
        let (_title, steps) = parse_workflow(md).unwrap();
        assert_eq!(steps[0].name, "a");
        assert_eq!(steps[1].name, "b");
        assert_eq!(steps[2].name, "c");
    }

    #[test]
    fn parse_step_with_no_prompt_section() {
        // A step without a "Prompt:" line should have an empty template.
        let md = "## Step: plan\nDepends-on: nothing\n";
        // This is valid — prompt_template will be empty.
        let result = parse_workflow(md);
        // parse_workflow only fails on empty file, not empty prompt
        assert!(result.is_ok());
        let (_t, steps) = result.unwrap();
        assert_eq!(steps[0].prompt_template, "");
    }

    // ─── Agent: field tests (work item 0052) ──────────────────────────────────

    #[test]
    fn parse_agent_field_populates_agent() {
        let md = "## Step: plan\nAgent: codex\nPrompt: Do the thing.\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert_eq!(
            steps[0].agent,
            Some("codex".to_string()),
            "Agent: field must populate the agent field on WorkflowStep"
        );
    }

    #[test]
    fn parse_step_without_agent_field_gives_none() {
        let md = "## Step: plan\nPrompt: Do the thing.\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert!(
            steps[0].agent.is_none(),
            "agent must be None when no Agent: field is present"
        );
    }

    #[test]
    fn parse_agent_after_prompt_is_body_not_directive() {
        // An `Agent:` line appearing after `Prompt:` belongs to the prompt body,
        // not to the step header — `agent` must remain `None`.
        let md = "## Step: plan\nPrompt: Do the thing.\nAgent: codex\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert!(
            steps[0].agent.is_none(),
            "`agent` must be None when Agent: appears after Prompt:"
        );
        assert!(
            steps[0].prompt_template.contains("Agent: codex"),
            "Agent: line after Prompt: must appear verbatim in the prompt body"
        );
    }

    #[test]
    fn parse_agent_field_isolated_per_step() {
        // Only the step with an Agent: field should have a non-None agent.
        let md = "## Step: a\nAgent: codex\nPrompt: A.\n\n## Step: b\nPrompt: B.\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert_eq!(
            steps[0].agent,
            Some("codex".to_string()),
            "step 'a' must carry the Agent: codex field"
        );
        assert!(
            steps[1].agent.is_none(),
            "step 'b' must have agent = None (no Agent: field)"
        );
    }

    // ─── Model: field tests (work item 0055) ──────────────────────────────────

    #[test]
    fn parse_model_field_populates_model() {
        let md = "## Step: plan\nModel: claude-opus-4-6\nPrompt: Do the thing.\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert_eq!(
            steps[0].model,
            Some("claude-opus-4-6".to_string()),
            "Model: field must populate the model field on WorkflowStep"
        );
    }

    #[test]
    fn parse_step_without_model_field_gives_none() {
        let md = "## Step: plan\nPrompt: Do the thing.\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert!(
            steps[0].model.is_none(),
            "model must be None when no Model: field is present"
        );
    }

    #[test]
    fn parse_model_after_prompt_is_body_not_directive() {
        // A `Model:` line appearing after `Prompt:` belongs to the prompt body.
        let md = "## Step: plan\nPrompt: Do the thing.\nModel: claude-opus-4-6\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert!(
            steps[0].model.is_none(),
            "`model` must be None when Model: appears after Prompt:"
        );
        assert!(
            steps[0].prompt_template.contains("Model: claude-opus-4-6"),
            "Model: line after Prompt: must appear verbatim in the prompt body"
        );
    }

    #[test]
    fn parse_empty_model_field_gives_none() {
        // A `Model:` line with no value is treated as absent.
        let md = "## Step: plan\nModel:\nPrompt: Do the thing.\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert!(
            steps[0].model.is_none(),
            "empty Model: field must produce model = None"
        );
    }

    #[test]
    fn parse_model_and_agent_in_same_step() {
        // Model: and Agent: are independent fields.
        let md = "## Step: plan\nAgent: codex\nModel: claude-haiku-4-5\nPrompt: Do the thing.\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert_eq!(steps[0].agent, Some("codex".to_string()));
        assert_eq!(steps[0].model, Some("claude-haiku-4-5".to_string()));
    }

    #[test]
    fn parse_model_field_isolated_per_step() {
        // Only the step with a Model: field should have a non-None model.
        let md = "## Step: a\nModel: big-model\nPrompt: A.\n\n## Step: b\nPrompt: B.\n";
        let (_, steps) = parse_workflow(md).unwrap();
        assert_eq!(
            steps[0].model,
            Some("big-model".to_string()),
            "step 'a' must carry the Model: field"
        );
        assert!(
            steps[1].model.is_none(),
            "step 'b' must have model = None (no Model: field)"
        );
    }

    // ─── TOML parser tests (work item 0056) ───────────────────────────────────

    #[test]
    fn toml_happy_path_all_fields() {
        let toml = r#"
title = "My Workflow"

[[step]]
name = "alpha"
prompt = "Do alpha."

[[step]]
name = "beta"
depends_on = ["alpha"]
agent = "codex"
model = "claude-opus-4-6"
prompt = "Do beta."
"#;
        let (title, steps) = parse_workflow_toml(toml).unwrap();
        assert_eq!(title.as_deref(), Some("My Workflow"));
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].name, "alpha");
        assert!(steps[0].depends_on.is_empty());
        assert_eq!(steps[0].prompt_template, "Do alpha.");
        assert!(steps[0].agent.is_none());
        assert!(steps[0].model.is_none());
        assert_eq!(steps[1].name, "beta");
        assert_eq!(steps[1].depends_on, vec!["alpha"]);
        assert_eq!(steps[1].agent, Some("codex".to_string()));
        assert_eq!(steps[1].model, Some("claude-opus-4-6".to_string()));
        assert_eq!(steps[1].prompt_template, "Do beta.");
    }

    #[test]
    fn toml_no_title_field_steps_still_parse() {
        let toml = r#"
[[step]]
name = "only-step"
prompt = "A prompt."
"#;
        let (title, steps) = parse_workflow_toml(toml).unwrap();
        assert!(title.is_none(), "title must be None when not specified");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].name, "only-step");
        assert_eq!(steps[0].prompt_template, "A prompt.");
    }

    #[test]
    fn toml_multiline_prompt_preserves_newlines_and_template_vars() {
        let toml = r#"
[[step]]
name = "go"
prompt = """
Line one.
Line two with {{work_item_number}}.
{{work_item_section:[Impl]}}
"""
"#;
        let (_, steps) = parse_workflow_toml(toml).unwrap();
        let p = &steps[0].prompt_template;
        assert!(p.contains("Line one."), "first line must be present");
        assert!(
            p.contains("{{work_item_number}}"),
            "work_item_number template var must survive parsing"
        );
        assert!(
            p.contains("{{work_item_section:[Impl]}}"),
            "section template var must survive parsing"
        );
        // Newline between the two content lines must be preserved.
        let pos_one = p.find("Line one.").unwrap();
        let pos_two = p.find("Line two with").unwrap();
        assert!(
            p[pos_one..pos_two].contains('\n'),
            "newline between lines must be preserved; got: {p:?}"
        );
    }

    #[test]
    fn toml_missing_name_field_returns_error() {
        let toml = r#"
[[step]]
prompt = "Do something."
"#;
        let result = parse_workflow_toml(toml);
        assert!(result.is_err(), "missing name field must produce an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("name") || msg.contains("index 0"),
            "error must mention missing name or step index; got: {msg}"
        );
    }

    #[test]
    fn toml_missing_prompt_field_returns_error() {
        let toml = r#"
[[step]]
name = "orphan"
"#;
        let result = parse_workflow_toml(toml);
        assert!(result.is_err(), "missing prompt field must produce an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("prompt") || msg.contains("orphan"),
            "error must mention missing prompt or step name; got: {msg}"
        );
    }

    #[test]
    fn toml_empty_steps_returns_error() {
        // A TOML file with a title but no [[step]] entries must error.
        let toml = r#"title = "Empty""#;
        let result = parse_workflow_toml(toml);
        assert!(result.is_err(), "empty steps array must produce an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no steps"),
            "error must mention no steps; got: {msg}"
        );
    }

    #[test]
    fn toml_unknown_field_returns_error() {
        // deny_unknown_fields must reject typos / extra keys.
        let toml = r#"
[[step]]
name = "a"
prompt = "Do it."
typo_field = "oops"
"#;
        let result = parse_workflow_toml(toml);
        assert!(
            result.is_err(),
            "unknown field must produce an error (deny_unknown_fields)"
        );
    }

    // ─── YAML parser tests (work item 0056) ───────────────────────────────────

    #[test]
    fn yaml_happy_path_all_fields() {
        let yaml = r#"
title: "My Workflow"
steps:
  - name: alpha
    prompt: "Do alpha."
  - name: beta
    depends_on: [alpha]
    agent: codex
    model: "claude-opus-4-6"
    prompt: "Do beta."
"#;
        let (title, steps) = parse_workflow_yaml(yaml).unwrap();
        assert_eq!(title.as_deref(), Some("My Workflow"));
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].name, "alpha");
        assert!(steps[0].depends_on.is_empty());
        assert_eq!(steps[0].prompt_template, "Do alpha.");
        assert!(steps[0].agent.is_none());
        assert!(steps[0].model.is_none());
        assert_eq!(steps[1].name, "beta");
        assert_eq!(steps[1].depends_on, vec!["alpha"]);
        assert_eq!(steps[1].agent, Some("codex".to_string()));
        assert_eq!(steps[1].model, Some("claude-opus-4-6".to_string()));
        assert_eq!(steps[1].prompt_template, "Do beta.");
    }

    #[test]
    fn yaml_depends_on_as_sequence() {
        let yaml = r#"
steps:
  - name: a
    prompt: "A."
  - name: b
    prompt: "B."
  - name: c
    depends_on: [a, b]
    prompt: "C."
"#;
        let (_, steps) = parse_workflow_yaml(yaml).unwrap();
        assert_eq!(
            steps[2].depends_on,
            vec!["a", "b"],
            "flow-sequence depends_on must parse into a Vec"
        );
    }

    #[test]
    fn yaml_depends_on_omitted_gives_empty_vec() {
        let yaml = r#"
steps:
  - name: root
    prompt: "I have no deps."
"#;
        let (_, steps) = parse_workflow_yaml(yaml).unwrap();
        assert!(
            steps[0].depends_on.is_empty(),
            "omitted depends_on must produce an empty Vec"
        );
    }

    #[test]
    fn yaml_literal_block_prompt_preserves_newlines_and_template_vars() {
        let yaml = r#"
steps:
  - name: go
    prompt: |
      Line one.
      Line two with {{work_item_number}}.
      {{work_item_section:[Impl]}}
"#;
        let (_, steps) = parse_workflow_yaml(yaml).unwrap();
        let p = &steps[0].prompt_template;
        assert!(p.contains("Line one."), "first line must be present");
        assert!(
            p.contains("{{work_item_number}}"),
            "work_item_number template var must survive parsing"
        );
        assert!(
            p.contains("{{work_item_section:[Impl]}}"),
            "section template var must survive parsing"
        );
        // Newline between the two content lines must be preserved.
        let pos_one = p.find("Line one.").unwrap();
        let pos_two = p.find("Line two with").unwrap();
        assert!(
            p[pos_one..pos_two].contains('\n'),
            "newline between lines must be preserved; got: {p:?}"
        );
    }

    #[test]
    fn yaml_missing_name_field_returns_error() {
        let yaml = r#"
steps:
  - prompt: "Do something."
"#;
        let result = parse_workflow_yaml(yaml);
        assert!(result.is_err(), "missing name field must produce an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("name") || msg.contains("index 0"),
            "error must mention missing name or step index; got: {msg}"
        );
    }

    #[test]
    fn yaml_missing_prompt_field_returns_error() {
        let yaml = r#"
steps:
  - name: orphan
"#;
        let result = parse_workflow_yaml(yaml);
        assert!(result.is_err(), "missing prompt field must produce an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("prompt") || msg.contains("orphan"),
            "error must mention missing prompt or step name; got: {msg}"
        );
    }

    #[test]
    fn yaml_empty_steps_returns_error() {
        let yaml = "title: \"Empty\"\nsteps: []\n";
        let result = parse_workflow_yaml(yaml);
        assert!(result.is_err(), "empty steps array must produce an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("no steps"),
            "error must mention no steps; got: {msg}"
        );
    }

    #[test]
    fn yaml_unknown_field_returns_error() {
        // deny_unknown_fields must reject typos / extra keys.
        let yaml = r#"
steps:
  - name: a
    prompt: "Do it."
    typo_field: "oops"
"#;
        let result = parse_workflow_yaml(yaml);
        assert!(
            result.is_err(),
            "unknown field must produce an error (deny_unknown_fields)"
        );
    }

    // ─── Format detection tests (work item 0056) ──────────────────────────────

    #[test]
    fn detect_format_md_returns_markdown() {
        assert_eq!(
            detect_format(Path::new("workflow.md")).unwrap(),
            WorkflowFormat::Markdown
        );
    }

    #[test]
    fn detect_format_toml_returns_toml() {
        assert_eq!(
            detect_format(Path::new("workflow.toml")).unwrap(),
            WorkflowFormat::Toml
        );
    }

    #[test]
    fn detect_format_yml_returns_yaml() {
        assert_eq!(
            detect_format(Path::new("workflow.yml")).unwrap(),
            WorkflowFormat::Yaml
        );
    }

    #[test]
    fn detect_format_yaml_returns_yaml() {
        assert_eq!(
            detect_format(Path::new("workflow.yaml")).unwrap(),
            WorkflowFormat::Yaml
        );
    }

    #[test]
    fn detect_format_json_returns_error() {
        let result = detect_format(Path::new("workflow.json"));
        assert!(result.is_err(), ".json extension must return an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("unsupported") || msg.contains(".md"),
            "error must describe supported formats; got: {msg}"
        );
    }
}
