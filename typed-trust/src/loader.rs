//! Manifest loader with optional path-policy enforcement.
//!
//! Phase 3 (MCP) needs the loader to be a library function instead
//! of a `main.rs`-private helper. This module factors out
//! `load_claims` and `load_claims_with_policy`, plus the
//! [`PathPolicy`] trait that the MCP server's allow-list
//! enforcement plugs into. Existing callers (the typed-trust CLI)
//! call `load_claims`, which is a thin alias for
//! `load_claims_with_policy(_, &PermitAll)`.
//!
//! Codex review-2 F-3-12: which stages consult the policy?
//!
//! | Stage                                | Consults? |
//! |--------------------------------------|-----------|
//! | manifest loader (root path)          | yes       |
//! | `include:` resolution                | yes       |
//! | sidecar loaders (in the MCP layer)   | yes       |
//! | translate / synthesize / renderers   | no (no I/O) |
//!
//! Keeping the cascade tightly contained to the I/O surface is the
//! single biggest implementation risk codex flagged. This module is
//! the entire I/O surface for manifest loading.

use std::fs;
use std::path::{Path, PathBuf};

use crate::translate::{parse_manifest_file, ManifestClaim};

/// One manifest claim paired with the file it came from. Mirrors
/// the previous `main.rs`-private `ClaimWithSource` shape, exposed
/// here so both the CLI and the MCP layer can consume it.
#[derive(Debug, Clone)]
pub struct LoadedClaim {
    pub claim: ManifestClaim,
    /// Path that authored this claim. For top-level claims, the
    /// manifest's path; for included claims, the include file's
    /// path. Preserves the audit trail in `SourceSpan`.
    pub source_path: String,
    pub span: String,
}

/// Why a [`PathPolicy::check`] call denied a path. The reason is
/// formatted into the loader error so MCP tool-result errors can
/// surface it.
#[derive(Debug, Clone)]
pub struct PolicyDenied {
    pub reason: String,
}

impl std::fmt::Display for PolicyDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for PolicyDenied {}

/// Policy that gates every file the loader reads.
///
/// Implementors canonicalize the path themselves (via
/// `Path::canonicalize`) so symlink and `..` traversal collapse
/// before the policy check happens. Failure to canonicalize
/// (dangling symlink, missing file, permission denied) is a
/// `PolicyDenied` with the cause in the reason — NOT silent
/// acceptance.
pub trait PathPolicy: Send + Sync {
    /// Check that `path` is allowed. Return the canonical path on
    /// success.
    fn check(&self, path: &Path) -> Result<PathBuf, PolicyDenied>;
}

/// Permissive policy: accept any path that exists. The CLI passes
/// this; existing tests get exactly the pre-Phase-3 behavior.
///
/// Even the permissive policy canonicalizes, so a manifest that
/// reaches the loader has had `..` and symlinks resolved
/// uniformly. That's a quiet correctness win.
#[derive(Debug, Default)]
pub struct PermitAll;

impl PathPolicy for PermitAll {
    fn check(&self, path: &Path) -> Result<PathBuf, PolicyDenied> {
        path.canonicalize().map_err(|e| PolicyDenied {
            reason: format!("path not accessible ({}): {}", path.display(), e),
        })
    }
}

/// Errors a policy-aware loader can return.
#[derive(Debug)]
pub enum LoaderError {
    /// File system error (read failed for a reason other than
    /// policy denial).
    Io { path: String, error: String },
    /// YAML parse failure.
    Yaml { path: String, error: String },
    /// Policy rejected a path.
    PolicyDenied { path: String, reason: String },
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::Io { path, error } => {
                write!(f, "error reading {path}: {error}")
            }
            LoaderError::Yaml { path, error } => {
                write!(f, "error parsing {path}: {error}")
            }
            LoaderError::PolicyDenied { path, reason } => {
                write!(f, "path policy denied {path}: {reason}")
            }
        }
    }
}

impl std::error::Error for LoaderError {}

/// Read a manifest YAML and resolve any `include:` entries with
/// the permissive policy. Equivalent to the pre-Phase-3 loader and
/// the entry point existing callers (CLI, integration tests) keep
/// using.
pub fn load_claims(path_str: &str) -> Result<Vec<LoadedClaim>, LoaderError> {
    load_claims_with_policy(path_str, &PermitAll)
}

/// Read a manifest YAML and resolve any `include:` entries with
/// an explicit policy. The MCP server's allow-list enforcement
/// passes its `AllowListPathPolicy` here.
///
/// Both the root manifest path AND every include resolution are
/// gated by the policy. A denial on the root manifest is the
/// caller's signal to map to a tier-1 protocol error (the
/// attacker named an unauthorized path); a denial on an include
/// is the caller's signal to map to a tier-2 data error (the
/// authorized manifest contains a bad include).
pub fn load_claims_with_policy(
    path_str: &str,
    policy: &dyn PathPolicy,
) -> Result<Vec<LoadedClaim>, LoaderError> {
    let path = PathBuf::from(path_str);
    let canonical_root =
        policy
            .check(&path)
            .map_err(|denied| LoaderError::PolicyDenied {
                path: path_str.into(),
                reason: denied.reason,
            })?;

    let yaml = fs::read_to_string(&canonical_root).map_err(|e| LoaderError::Io {
        path: canonical_root.display().to_string(),
        error: e.to_string(),
    })?;

    let manifest = parse_manifest_file(&yaml).map_err(|e| LoaderError::Yaml {
        path: canonical_root.display().to_string(),
        error: e.to_string(),
    })?;

    let mut out: Vec<LoadedClaim> = Vec::new();
    for (idx, c) in manifest.claims.into_iter().enumerate() {
        out.push(LoadedClaim {
            claim: c,
            source_path: path_str.to_string(),
            span: format!("claims[{idx}]"),
        });
    }

    for inc in extract_includes(&yaml) {
        // Resolve the include relative to the canonical root's
        // parent (so symlinks in the root resolved already);
        // canonicalize again via the policy.
        let resolved = canonical_root
            .parent()
            .map(|p| p.join(&inc))
            .unwrap_or_else(|| Path::new(&inc).to_path_buf());
        let canonical_inc =
            policy
                .check(&resolved)
                .map_err(|denied| LoaderError::PolicyDenied {
                    path: resolved.display().to_string(),
                    reason: denied.reason,
                })?;
        let inc_yaml = fs::read_to_string(&canonical_inc).map_err(|e| LoaderError::Io {
            path: canonical_inc.display().to_string(),
            error: e.to_string(),
        })?;
        let inc_manifest = parse_manifest_file(&inc_yaml).map_err(|e| LoaderError::Yaml {
            path: canonical_inc.display().to_string(),
            error: e.to_string(),
        })?;
        let inc_path_str = canonical_inc.to_string_lossy().into_owned();
        for (idx, c) in inc_manifest.claims.into_iter().enumerate() {
            out.push(LoadedClaim {
                claim: c,
                source_path: inc_path_str.clone(),
                span: format!("claims[{idx}]"),
            });
        }
    }

    Ok(out)
}

/// Parse the top-level YAML by hand to extract `include:` paths.
/// `ManifestFile` doesn't carry an `include` field (the schema
/// layer is intentionally small).
fn extract_includes(yaml: &str) -> Vec<String> {
    let parsed: serde_yaml_ng::Value = match serde_yaml_ng::from_str(yaml) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(includes) = parsed.get("include").and_then(|v| v.as_sequence()) else {
        return Vec::new();
    };
    includes
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect()
}

// ============================================================
// AllowListPathPolicy — MCP server's enforcement implementation.
// Lives here so the test suite can construct one without spawning
// the MCP server.
// ============================================================

/// Allow-list policy used by the MCP server. Configured at server
/// startup from `--allow-manifest <path>` flags (each canonicalized
/// at registration time). A request's canonical path must either
/// equal an allowed file path exactly, or live under an allowed
/// directory's canonical tree.
#[derive(Debug, Default, Clone)]
pub struct AllowListPathPolicy {
    /// Canonical paths registered as allowed files (exact-match)
    /// or directories (subtree-allow).
    roots: Vec<AllowedRoot>,
}

#[derive(Debug, Clone)]
enum AllowedRoot {
    File(PathBuf),
    Dir(PathBuf),
}

impl AllowListPathPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an allowed path. Canonicalizes immediately; a
    /// non-existent or otherwise inaccessible path returns
    /// `PolicyDenied` so the server can fail at startup rather than
    /// pretending to be configured.
    pub fn allow(&mut self, path: impl AsRef<Path>) -> Result<(), PolicyDenied> {
        let path = path.as_ref();
        let canonical = path.canonicalize().map_err(|e| PolicyDenied {
            reason: format!(
                "cannot canonicalize allow-list path {} ({e})",
                path.display()
            ),
        })?;
        if canonical.is_dir() {
            self.roots.push(AllowedRoot::Dir(canonical));
        } else if canonical.is_file() {
            self.roots.push(AllowedRoot::File(canonical));
        } else {
            return Err(PolicyDenied {
                reason: format!(
                    "{} is neither a file nor a directory; allow-list entries must be one",
                    canonical.display()
                ),
            });
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
}

impl PathPolicy for AllowListPathPolicy {
    fn check(&self, path: &Path) -> Result<PathBuf, PolicyDenied> {
        if self.roots.is_empty() {
            return Err(PolicyDenied {
                reason: "no allowed paths configured (use --allow-manifest)".into(),
            });
        }
        let canonical = path.canonicalize().map_err(|e| PolicyDenied {
            reason: format!("path not accessible ({}): {}", path.display(), e),
        })?;
        for root in &self.roots {
            match root {
                AllowedRoot::File(allowed) => {
                    if &canonical == allowed {
                        return Ok(canonical);
                    }
                }
                AllowedRoot::Dir(allowed) => {
                    if canonical.starts_with(allowed) {
                        return Ok(canonical);
                    }
                }
            }
        }
        Err(PolicyDenied {
            reason: format!(
                "{} not under any allowed root",
                canonical.display()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn permitall_canonicalizes() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("x.yaml");
        std::fs::write(&f, "claims: []").unwrap();
        let canonical = PermitAll.check(&f).unwrap();
        // canonical path resolves the tempdir's eventual symlink
        // (macOS /var → /private/var); on Linux it's identity.
        assert!(canonical.is_absolute());
    }

    #[test]
    fn permitall_rejects_missing_path() {
        let tmp = tempfile::tempdir().unwrap();
        let nope = tmp.path().join("does-not-exist.yaml");
        let err = PermitAll.check(&nope).expect_err("missing path");
        assert!(err.reason.contains("not accessible"));
    }

    #[test]
    fn allowlist_empty_denies_everything() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("x.yaml");
        std::fs::write(&f, "claims: []").unwrap();
        let policy = AllowListPathPolicy::new();
        let err = policy.check(&f).expect_err("empty allow-list");
        assert!(err.reason.contains("no allowed paths configured"));
    }

    #[test]
    fn allowlist_dir_admits_children_only() {
        let tmp = tempfile::tempdir().unwrap();
        let inside = tmp.path().join("inside.yaml");
        std::fs::write(&inside, "claims: []").unwrap();

        let outside_dir = tempfile::tempdir().unwrap();
        let outside = outside_dir.path().join("outside.yaml");
        std::fs::write(&outside, "claims: []").unwrap();

        let mut policy = AllowListPathPolicy::new();
        policy.allow(tmp.path()).unwrap();
        assert!(policy.check(&inside).is_ok());
        assert!(policy.check(&outside).is_err());
    }

    #[test]
    fn allowlist_file_requires_exact_match() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.yaml");
        let b = tmp.path().join("b.yaml");
        std::fs::write(&a, "claims: []").unwrap();
        std::fs::write(&b, "claims: []").unwrap();
        let mut policy = AllowListPathPolicy::new();
        policy.allow(&a).unwrap();
        assert!(policy.check(&a).is_ok());
        assert!(policy.check(&b).is_err());
    }

    #[test]
    fn load_claims_with_policy_denies_root_outside_allow() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let m = outside.path().join("evident.yaml");
        let mut f = std::fs::File::create(&m).unwrap();
        writeln!(f, "claims: []").unwrap();

        let mut policy = AllowListPathPolicy::new();
        policy.allow(tmp.path()).unwrap();

        let err = load_claims_with_policy(m.to_str().unwrap(), &policy).expect_err("denied");
        assert!(
            matches!(err, LoaderError::PolicyDenied { .. }),
            "expected PolicyDenied, got {err:?}"
        );
    }

    #[test]
    fn load_claims_with_policy_denies_include_outside_allow() {
        // The load-bearing property: an authorized root manifest
        // with a bad `include:` lands in PolicyDenied via the
        // include path, not silently followed.
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.yaml");
        std::fs::write(&secret, "claims: []").unwrap();

        let root = tmp.path().join("evident.yaml");
        std::fs::write(
            &root,
            format!(
                "version: 0.1\nproject: x\ninclude:\n  - {}\n",
                secret.to_str().unwrap()
            ),
        )
        .unwrap();

        let mut policy = AllowListPathPolicy::new();
        policy.allow(tmp.path()).unwrap();

        let err = load_claims_with_policy(root.to_str().unwrap(), &policy)
            .expect_err("include outside allow should be denied");
        match err {
            LoaderError::PolicyDenied { path, reason } => {
                assert!(
                    reason.contains("not under any allowed root"),
                    "denial reason should cite allow-list miss; got: {reason}"
                );
                assert!(
                    path.contains("secret.yaml"),
                    "denial path should cite the bad include, got: {path}"
                );
            }
            other => panic!("expected PolicyDenied, got {other:?}"),
        }
    }
}
