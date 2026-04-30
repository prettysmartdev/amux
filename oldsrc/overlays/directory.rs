use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Trait for typed overlay entries: defines conflict detection and merge semantics.
///
/// Implemented by every concrete overlay type so that the resolution pipeline
/// can work generically.  Currently only `DirectoryOverlay` exists; future types
/// (e.g. `SecretOverlay`) will implement this trait and integrate into the same
/// pipeline without touching `effective_overlays`.
pub trait Overlay: Clone + PartialEq {
    /// Returns the string key that uniquely identifies the "source resource" of
    /// this overlay.  Two overlays with the same `conflict_key` are considered to
    /// reference the same host resource and will be deduplicated during resolution.
    ///
    /// For `DirectoryOverlay`, the key is the **canonicalized** host path
    /// (symlinks and `..`/`.` components resolved via `fs::canonicalize`) so that
    /// `/foo/baz/../bar` and `/foo/bar` produce the same key.  Falls back to the
    /// raw path when canonicalize fails (e.g. path does not yet exist — such
    /// entries are dropped by the existence check in `resolve_overlays`).
    fn conflict_key(&self) -> String;

    /// Merge `self` (higher priority) with `other` (lower priority) that share
    /// the same `conflict_key`.  Returns the resolved overlay.
    ///
    /// Convention:
    /// - Non-permission fields: `self` (higher priority) wins.
    /// - Permission: the **more restrictive** of the two wins regardless of
    ///   source priority.  A `warn!` is emitted whenever they differ.
    fn merge_with_lower(&self, other: &Self) -> Self;
}

/// Permission for a directory overlay mount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountPermission {
    /// Read-only mount (`:ro`) — the default.
    ReadOnly,
    /// Read-write mount (`:rw`).
    ReadWrite,
}

impl Default for MountPermission {
    fn default() -> Self {
        MountPermission::ReadOnly
    }
}

impl MountPermission {
    /// Returns the Docker mount suffix string.
    pub fn as_str(&self) -> &'static str {
        match self {
            MountPermission::ReadOnly => "ro",
            MountPermission::ReadWrite => "rw",
        }
    }

    /// Parse a permission string. Returns `None` for unrecognised values.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "ro" => Some(MountPermission::ReadOnly),
            "rw" => Some(MountPermission::ReadWrite),
            _ => None,
        }
    }

    /// Returns the more restrictive of two permissions (`ro` beats `rw`).
    pub fn most_restrictive(&self, other: &Self) -> Self {
        if *self == MountPermission::ReadOnly || *other == MountPermission::ReadOnly {
            MountPermission::ReadOnly
        } else {
            MountPermission::ReadWrite
        }
    }
}

/// A directory overlay: mounts a host directory into the agent container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectoryOverlay {
    pub host_path: PathBuf,
    pub container_path: PathBuf,
    pub permission: MountPermission,
}

impl Overlay for DirectoryOverlay {
    fn conflict_key(&self) -> String {
        // Resolve symlinks and normalise `..`/`.` components so that paths like
        // `/foo/baz/../bar` and `/foo/bar` (and symlink targets) map to the same
        // key.  Falls back to the raw path string when canonicalize fails — e.g.
        // the path does not yet exist.  Non-existent entries are dropped later by
        // the existence check in `resolve_overlays`.
        std::fs::canonicalize(&self.host_path)
            .unwrap_or_else(|_| self.host_path.clone())
            .to_string_lossy()
            .to_string()
    }

    fn merge_with_lower(&self, other: &Self) -> Self {
        if self.permission != other.permission {
            tracing::warn!(
                "overlay permission conflict for host path '{}': \
                 higher priority has {:?}, lower has {:?}; using most restrictive",
                self.host_path.display(),
                self.permission,
                other.permission,
            );
        }
        if self.container_path != other.container_path {
            tracing::warn!(
                "overlay container_path conflict for host path '{}': \
                 higher priority mounts at '{}', lower at '{}'; using higher priority",
                self.host_path.display(),
                self.container_path.display(),
                other.container_path.display(),
            );
        }
        DirectoryOverlay {
            host_path: self.host_path.clone(),
            container_path: self.container_path.clone(),
            permission: self.permission.most_restrictive(&other.permission),
        }
    }
}
