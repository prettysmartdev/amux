use crate::commands::spec::CommandSpec;
use std::collections::HashMap;

/// Parse `parts` (the tokenized TUI command line) against `spec`.
///
/// Returns a map of flag name → value (empty string for boolean flags).
/// Supports both `--flag value` and `--flag=value` forms.
///
/// Tokens that do not start with `--` (e.g. positional arguments such as a
/// work item number) are silently ignored — callers must extract those separately.
/// Unknown `--flag` tokens that are not in `spec.flags` are also silently ignored.
pub fn parse_flags(parts: &[&str], spec: &CommandSpec) -> HashMap<&'static str, String> {
    let mut result = HashMap::new();
    let mut i = 0;
    while i < parts.len() {
        let token = parts[i];
        if let Some(rest) = token.strip_prefix("--") {
            // Handle --flag=value form.
            if let Some((key, val)) = rest.split_once('=') {
                if let Some(fs) = spec.flags.iter().find(|f| f.name == key) {
                    result.insert(fs.name, val.to_string());
                }
            } else {
                // Handle --flag or --flag value form.
                if let Some(fs) = spec.flags.iter().find(|f| f.name == rest) {
                    if fs.takes_value {
                        if let Some(val) = parts.get(i + 1) {
                            // Do not consume the next token if it looks like a flag itself.
                            if !val.starts_with("--") {
                                result.insert(fs.name, val.to_string());
                                i += 1;
                            }
                        }
                    } else {
                        result.insert(fs.name, String::new());
                    }
                }
            }
        }
        i += 1;
    }
    result
}

/// Returns `true` if `name` was present in the parsed flag map (boolean flag check).
pub fn flag_bool(flags: &HashMap<&str, String>, name: &str) -> bool {
    flags.contains_key(name)
}

/// Returns the string value for `name` if it was present in the parsed flag map,
/// or `None` if the flag was absent or had no value.
pub fn flag_string<'a>(flags: &'a HashMap<&str, String>, name: &str) -> Option<&'a str> {
    flags.get(name).map(|s| s.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::spec::ALL_COMMANDS;

    fn chat_spec() -> &'static crate::commands::spec::CommandSpec {
        ALL_COMMANDS.iter().find(|c| c.name == "chat").unwrap()
    }

    fn impl_spec() -> &'static crate::commands::spec::CommandSpec {
        ALL_COMMANDS.iter().find(|c| c.name == "implement").unwrap()
    }

    // ── empty / trivial ──────────────────────────────────────────────────────

    #[test]
    fn parse_flags_empty_parts_returns_empty_map() {
        let flags = parse_flags(&[], chat_spec());
        assert!(flags.is_empty());
    }

    #[test]
    fn parse_flags_unknown_flag_is_silently_ignored() {
        let flags = parse_flags(&["chat", "--unknown-flag"], chat_spec());
        assert!(!flags.contains_key("unknown-flag"));
        assert!(flags.is_empty());
    }

    // ── CHAT_FLAGS — value-taking flag, both forms ───────────────────────────

    #[test]
    fn parse_flags_chat_agent_space_form() {
        let flags = parse_flags(&["chat", "--agent", "codex"], chat_spec());
        assert_eq!(flag_string(&flags, "agent"), Some("codex"));
    }

    #[test]
    fn parse_flags_chat_agent_eq_form() {
        // "--agent=codex" must be handled as a single token (no space split).
        let flags = parse_flags(&["chat", "--agent=codex"], chat_spec());
        assert_eq!(flag_string(&flags, "agent"), Some("codex"));
    }

    /// `--flag=` with nothing after the `=` must produce `Some("")`, not `None`.
    /// Semantics are left to the caller; the parser just records the empty string.
    #[test]
    fn parse_flags_eq_form_empty_value_is_some_empty_string() {
        let flags = parse_flags(&["chat", "--agent="], chat_spec());
        assert_eq!(
            flag_string(&flags, "agent"),
            Some(""),
            "--agent= should yield Some(\"\"), not None",
        );
    }

    // ── CHAT_FLAGS — all bool flags present ─────────────────────────────────

    #[test]
    fn parse_flags_chat_all_bool_flags_present() {
        let flags = parse_flags(
            &["chat", "--non-interactive", "--plan", "--allow-docker",
              "--mount-ssh", "--yolo", "--auto"],
            chat_spec(),
        );
        assert!(flag_bool(&flags, "non-interactive"));
        assert!(flag_bool(&flags, "plan"));
        assert!(flag_bool(&flags, "allow-docker"));
        assert!(flag_bool(&flags, "mount-ssh"));
        assert!(flag_bool(&flags, "yolo"));
        assert!(flag_bool(&flags, "auto"));
    }

    #[test]
    fn parse_flags_chat_all_flags_absent_by_default() {
        let flags = parse_flags(&["chat"], chat_spec());
        assert!(!flag_bool(&flags, "non-interactive"));
        assert!(!flag_bool(&flags, "plan"));
        assert!(!flag_bool(&flags, "allow-docker"));
        assert!(!flag_bool(&flags, "mount-ssh"));
        assert!(!flag_bool(&flags, "yolo"));
        assert!(!flag_bool(&flags, "auto"));
        assert_eq!(flag_string(&flags, "agent"), None);
    }

    // ── IMPLEMENT_FLAGS — value-taking flags, both forms ────────────────────

    #[test]
    fn parse_flags_workflow_space_form_captures_value() {
        let flags = parse_flags(
            &["implement", "0042", "--workflow", "myfile.md"],
            impl_spec(),
        );
        assert_eq!(flag_string(&flags, "workflow"), Some("myfile.md"));
    }

    #[test]
    fn parse_flags_workflow_eq_form_captures_value() {
        let flags = parse_flags(
            &["implement", "0042", "--workflow=myfile.md"],
            impl_spec(),
        );
        assert_eq!(flag_string(&flags, "workflow"), Some("myfile.md"));
    }

    /// `--workflow --plan`: the next token looks like a flag, so it must NOT be
    /// consumed as the workflow value. `--plan` must still be recognized.
    #[test]
    fn parse_flags_workflow_next_is_flag_not_consumed() {
        let flags = parse_flags(
            &["implement", "0042", "--workflow", "--plan"],
            impl_spec(),
        );
        assert_eq!(
            flag_string(&flags, "workflow"),
            None,
            "--plan must not be captured as the workflow value",
        );
        assert!(
            flag_bool(&flags, "plan"),
            "--plan must still be recognized as a boolean flag",
        );
    }
}
