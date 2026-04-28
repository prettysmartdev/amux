use anyhow::{bail, Result};
use std::path::PathBuf;

use super::directory::{DirectoryOverlay, MountPermission};

/// A typed overlay expression parsed from CLI flags or env vars.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedOverlay {
    Directory(DirectoryOverlay),
    // Future: Secret(SecretOverlay), Skill(SkillOverlay), …
}

/// Parse a comma-separated list of typed overlay expressions.
///
/// Grammar:
/// ```text
/// overlay-list   := overlay-expr ("," overlay-expr)*
/// overlay-expr   := type-tag "(" overlay-args ")"
/// type-tag       := "dir"
/// overlay-args   := host-path ":" container-path [ ":" permission ]
/// permission     := "ro" | "rw"
/// ```
///
/// Examples:
/// - `dir(/data/ref:/mnt/ref:ro)`
/// - `dir(/data/ref:/mnt/ref), dir(~/prompts:/mnt/prompts:rw)`
pub fn parse_overlay_list(input: &str) -> Result<Vec<TypedOverlay>> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(vec![]);
    }

    let mut results = Vec::new();
    // Split on commas that are outside parentheses.
    let exprs = split_top_level_commas(input);

    for expr in exprs {
        let expr = expr.trim();
        if expr.is_empty() {
            continue;
        }
        results.push(parse_single_overlay(expr)?);
    }

    Ok(results)
}

/// Split a string on commas that are not inside parentheses.
fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;

    for (i, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                results.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    results.push(&input[start..]);
    results
}

/// Parse a single overlay expression like `dir(/host/path:/container/path:ro)`.
fn parse_single_overlay(expr: &str) -> Result<TypedOverlay> {
    // Find the type tag and the parenthesised arguments.
    let open = expr.find('(').ok_or_else(|| {
        anyhow::anyhow!(
            "malformed overlay expression (missing opening parenthesis): {:?}",
            expr
        )
    })?;
    let close = expr.rfind(')').ok_or_else(|| {
        anyhow::anyhow!(
            "malformed overlay expression (missing closing parenthesis): {:?}",
            expr
        )
    })?;
    if close <= open {
        bail!(
            "malformed overlay expression (parentheses out of order): {:?}",
            expr
        );
    }

    let tag = expr[..open].trim();
    let args = expr[open + 1..close].trim();

    match tag {
        "dir" => parse_dir_overlay(args, expr),
        _ => bail!(
            "unknown overlay type {:?} in expression {:?}; supported types: dir",
            tag,
            expr
        ),
    }
}

/// Parse the arguments inside `dir(...)`: `host-path:container-path[:permission]`.
fn parse_dir_overlay(args: &str, full_expr: &str) -> Result<TypedOverlay> {
    if args.is_empty() {
        bail!(
            "empty arguments in directory overlay expression: {:?}",
            full_expr
        );
    }

    // Split on ':' — but we need to be careful with Windows paths like C:\foo.
    // The spec says host-path and container-path are absolute, so on Unix there's
    // no ambiguity. We split from the right to handle the optional permission field.
    let parts: Vec<&str> = args.splitn(3, ':').collect();

    let (host_str, container_str, perm_str) = match parts.len() {
        2 => (parts[0], parts[1], None),
        3 => {
            // parts[2] might be a permission or part of the container path.
            let candidate = parts[2].trim();
            if candidate == "ro" || candidate == "rw" {
                (parts[0], parts[1], Some(candidate))
            } else {
                // Not a known permission — treat as container_path:rest.
                // This handles edge cases, but the grammar says permission is optional.
                bail!(
                    "invalid permission {:?} in overlay expression {:?}; expected 'ro' or 'rw'",
                    candidate,
                    full_expr
                );
            }
        }
        _ => bail!(
            "expected 'host_path:container_path[:permission]' in overlay expression {:?}",
            full_expr
        ),
    };

    let host_str = host_str.trim();
    let container_str = container_str.trim();

    if host_str.is_empty() {
        bail!("empty host path in overlay expression {:?}", full_expr);
    }
    if container_str.is_empty() {
        bail!(
            "empty container path in overlay expression {:?}",
            full_expr
        );
    }

    let permission = match perm_str {
        Some(p) => MountPermission::from_str_opt(p).ok_or_else(|| {
            anyhow::anyhow!(
                "invalid permission {:?} in overlay expression {:?}; expected 'ro' or 'rw'",
                p,
                full_expr
            )
        })?,
        None => MountPermission::default(),
    };

    // Expand ~ and resolve relative paths to an absolute path.
    let host_path = super::make_host_path_absolute(host_str);

    Ok(TypedOverlay::Directory(DirectoryOverlay {
        host_path,
        container_path: PathBuf::from(container_str),
        permission,
    }))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_string_returns_empty() {
        let result = parse_overlay_list("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_single_dir_ro() {
        let result = parse_overlay_list("dir(/data/ref:/mnt/ref:ro)").unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            TypedOverlay::Directory(d) => {
                assert_eq!(d.host_path, PathBuf::from("/data/ref"));
                assert_eq!(d.container_path, PathBuf::from("/mnt/ref"));
                assert_eq!(d.permission, MountPermission::ReadOnly);
            }
        }
    }

    #[test]
    fn parse_single_dir_rw() {
        let result = parse_overlay_list("dir(/data/ref:/mnt/ref:rw)").unwrap();
        match &result[0] {
            TypedOverlay::Directory(d) => {
                assert_eq!(d.permission, MountPermission::ReadWrite);
            }
        }
    }

    #[test]
    fn parse_single_dir_default_permission() {
        let result = parse_overlay_list("dir(/data/ref:/mnt/ref)").unwrap();
        match &result[0] {
            TypedOverlay::Directory(d) => {
                assert_eq!(d.permission, MountPermission::ReadOnly);
            }
        }
    }

    #[test]
    fn parse_multiple_overlays() {
        let result =
            parse_overlay_list("dir(/data/ref:/mnt/ref), dir(/home/user/prompts:/mnt/prompts:rw)")
                .unwrap();
        assert_eq!(result.len(), 2);
        match &result[0] {
            TypedOverlay::Directory(d) => {
                assert_eq!(d.host_path, PathBuf::from("/data/ref"));
                assert_eq!(d.container_path, PathBuf::from("/mnt/ref"));
                assert_eq!(d.permission, MountPermission::ReadOnly);
            }
        }
        match &result[1] {
            TypedOverlay::Directory(d) => {
                assert_eq!(d.host_path, PathBuf::from("/home/user/prompts"));
                assert_eq!(d.container_path, PathBuf::from("/mnt/prompts"));
                assert_eq!(d.permission, MountPermission::ReadWrite);
            }
        }
    }

    #[test]
    fn parse_unknown_type_tag_errors() {
        let result = parse_overlay_list("secret(/foo:/bar)");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unknown overlay type"));
    }

    #[test]
    fn parse_missing_parens_errors() {
        let result = parse_overlay_list("dir/foo:/bar");
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_host_path_errors() {
        let result = parse_overlay_list("dir(:/mnt/ref)");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("empty host path"));
    }

    #[test]
    fn parse_empty_container_path_errors() {
        let result = parse_overlay_list("dir(/data/ref:)");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("empty container path"));
    }

    #[test]
    fn parse_invalid_permission_errors() {
        let result = parse_overlay_list("dir(/data/ref:/mnt/ref:rwx)");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("invalid permission"));
    }

    #[test]
    fn parse_tilde_expands_home() {
        // We can't predict the home dir, but we can verify ~ is not literal.
        let result = parse_overlay_list("dir(~/prompts:/mnt/prompts)").unwrap();
        match &result[0] {
            TypedOverlay::Directory(d) => {
                let path_str = d.host_path.to_string_lossy();
                // If home dir is available, ~ should be expanded.
                // If not, the path stays as ~/prompts.
                if dirs::home_dir().is_some() {
                    assert!(
                        !path_str.starts_with('~'),
                        "tilde should be expanded; got: {}",
                        path_str
                    );
                }
            }
        }
    }

    #[test]
    fn parse_missing_colon_separator_errors() {
        // dir(/foo/bar) has no ':' between host and container paths.
        // splitn(3, ':') yields a single element, hitting the catch-all bail!.
        let result = parse_overlay_list("dir(/foo/bar)");
        assert!(result.is_err(), "missing ':' separator must be an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("expected") || msg.contains("host_path") || msg.contains("container"),
            "error should describe the expected format; got: {msg}"
        );
    }

    #[test]
    fn parse_path_with_spaces_works() {
        // Convention: spaces in paths are supported natively because the grammar
        // splits on ':' (not ' ').  No quoting or percent-encoding is required;
        // spaces appear literally in the host/container path strings.
        let result = parse_overlay_list("dir(/path with spaces:/mnt/ref:ro)").unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            TypedOverlay::Directory(d) => {
                assert_eq!(
                    d.host_path,
                    PathBuf::from("/path with spaces"),
                    "spaces in host path must be preserved literally"
                );
                assert_eq!(d.container_path, PathBuf::from("/mnt/ref"));
                assert_eq!(d.permission, MountPermission::ReadOnly);
            }
        }
    }

    #[test]
    fn parse_whitespace_around_expressions_is_trimmed() {
        let result = parse_overlay_list("  dir( /data/ref : /mnt/ref : ro )  ").unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            TypedOverlay::Directory(d) => {
                assert_eq!(d.host_path, PathBuf::from("/data/ref"));
                assert_eq!(d.container_path, PathBuf::from("/mnt/ref"));
                assert_eq!(d.permission, MountPermission::ReadOnly);
            }
        }
    }
}
