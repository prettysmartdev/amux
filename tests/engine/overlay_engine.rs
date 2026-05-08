//! OverlayEngine structural tests.
//!
//! These tests verify the denylist and overlay types without touching the
//! filesystem or Docker. All run under `make test-fast`.

use amux::engine::container::options::OverlayPermission;
use amux::engine::overlay::{DirectorySpec, OverlayRequest, CLAUDE_DENYLIST};

// ─── CLAUDE_DENYLIST integrity ────────────────────────────────────────────────

#[test]
fn claude_denylist_contains_projects() {
    assert!(CLAUDE_DENYLIST.contains(&"projects"));
}

#[test]
fn claude_denylist_contains_sessions() {
    assert!(CLAUDE_DENYLIST.contains(&"sessions"));
}

#[test]
fn claude_denylist_contains_history_jsonl() {
    assert!(CLAUDE_DENYLIST.contains(&"history.jsonl"));
}

#[test]
fn claude_denylist_contains_telemetry() {
    assert!(CLAUDE_DENYLIST.contains(&"telemetry"));
}

#[test]
fn claude_denylist_does_not_contain_settings_json() {
    // settings.json must NOT be on the denylist — it is the overlay file.
    assert!(!CLAUDE_DENYLIST.contains(&"settings.json"));
}

#[test]
fn claude_denylist_is_non_empty() {
    assert!(!CLAUDE_DENYLIST.is_empty());
}

// ─── OverlayRequest defaults ──────────────────────────────────────────────────

#[test]
fn overlay_request_default_has_no_agent() {
    let req = OverlayRequest::default();
    assert!(req.agent.is_none());
    assert!(!req.yolo);
    assert!(req.directories.is_empty());
}

// ─── DirectorySpec construction ───────────────────────────────────────────────

#[test]
fn directory_spec_fields_accessible() {
    let spec = DirectorySpec {
        host: "/host/path".to_string(),
        container: "/container/path".to_string(),
        permission: OverlayPermission::ReadOnly,
    };
    assert_eq!(spec.host, "/host/path");
    assert_eq!(spec.container, "/container/path");
    assert_eq!(spec.permission, OverlayPermission::ReadOnly);
}

#[test]
fn overlay_permission_variants_distinct() {
    assert_ne!(OverlayPermission::ReadOnly, OverlayPermission::ReadWrite);
}
