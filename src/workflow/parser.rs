use anyhow::{bail, Result};

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
}
