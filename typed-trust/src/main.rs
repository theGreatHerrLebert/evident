//! `typed-trust` CLI.
//!
//! Reads a shipping `evident.yaml` (or a per-claim file), translates
//! each measurement-class claim into typed-trust constructors,
//! synthesizes a TrustReport, applies the renderer-aux layer, and
//! emits either a JSON bundle or a human-readable markdown rollup to
//! stdout.
//!
//! Usage:
//!   typed-trust [--format json|md] <manifest.yaml> [claim_id]
//!
//! With a `claim_id`, only that claim's report is emitted. Without,
//! every measurement claim is translated. Non-measurement claims
//! (policy, reference) are listed in the `skipped` section with the
//! OutOfScope reason.
//!
//! Manifests with a top-level `include:` list (e.g. proteon's
//! `evident.yaml`) have each included claim file resolved and merged
//! into a single sequence before translation. Paths in `include` are
//! resolved relative to the top-level manifest's directory.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use typed_trust::translate::{
    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
    TranslationContext,
};
use typed_trust::*;

#[derive(serde::Serialize)]
struct SkipReason {
    id: String,
    reason: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Markdown,
}

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().collect();
    let (format, positional) = match parse_args(&raw_args) {
        Some(parsed) => parsed,
        None => return ExitCode::from(2),
    };

    let Some(path) = positional.first() else {
        usage();
        return ExitCode::from(2);
    };
    let filter = positional.get(1).cloned();

    let claims = match load_claims(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let now: Timestamp = "1970-01-01T00:00:00Z".into();

    let mut reports: Vec<serde_json::Value> = Vec::new();
    let mut skipped: Vec<SkipReason> = Vec::new();

    for cw in claims.iter() {
        let mc = &cw.claim;
        if let Some(ref f) = filter {
            if mc.id != *f {
                continue;
            }
        }
        // Per-claim TranslationContext so the resulting SourceSpan
        // points at the originating manifest file (for `include:`
        // top-level manifests, that's the included file, not the
        // top-level evident.yaml).
        let ctx = TranslationContext {
            now: now.clone(),
            manifest_path: cw.source_path.clone(),
        };

        if let Err(e) = translate_claim(&ctx, mc, &cw.span) {
            skipped.push(SkipReason {
                id: mc.id.clone(),
                reason: format!("{e}"),
            });
            continue;
        }

        let criteria = match translate_tolerances(mc) {
            Ok(c) => c,
            Err(e) => {
                skipped.push(SkipReason {
                    id: mc.id.clone(),
                    reason: format!("{e}"),
                });
                continue;
            }
        };

        let evidence: Vec<Evidence> = translate_evidence(&ctx, mc, &criteria)
            .into_iter()
            .collect();

        // CLI has no review events, so no cycle set is needed.
        let report = synthesize(
            ClaimId::new(&mc.id),
            criteria,
            &evidence,
            &[],
            &[],
            &std::collections::HashSet::new(),
            now.clone(),
        );

        let augmented = render_augmented(&RenderInput {
            report: &report,
            evidence: &evidence,
            related_events: &[],
            backing_reports: &[],
        });

        reports.push(augmented);
    }

    match format {
        Format::Json => emit_json(path, &now, &reports, &skipped),
        Format::Markdown => emit_markdown(path, &reports, &skipped),
    }
}

fn parse_args(args: &[String]) -> Option<(Format, Vec<String>)> {
    if args.len() < 2 || args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
        return None;
    }
    let mut format = Format::Json;
    let mut positional: Vec<String> = Vec::new();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--format=") {
            format = parse_format_value(value)?;
        } else if arg == "--format" {
            let Some(value) = iter.next() else {
                eprintln!("error: --format requires a value (json|md)");
                return None;
            };
            format = parse_format_value(value)?;
        } else {
            positional.push(arg.clone());
        }
    }
    Some((format, positional))
}

fn parse_format_value(v: &str) -> Option<Format> {
    match v {
        "json" => Some(Format::Json),
        "md" | "markdown" => Some(Format::Markdown),
        other => {
            eprintln!("error: unknown --format {other:?} (expected json or md)");
            None
        }
    }
}

fn usage() {
    eprintln!("usage: typed-trust [--format json|md] <manifest.yaml> [claim_id]");
    eprintln!();
    eprintln!("Translates each measurement-class claim, synthesizes a TrustReport,");
    eprintln!("applies the renderer-aux layer, and emits one of:");
    eprintln!("  --format json (default) — one JSON bundle for tools / CI gates");
    eprintln!("  --format md             — markdown rollup for humans");
    eprintln!();
    eprintln!("Manifests with a top-level `include:` list have each included file");
    eprintln!("merged in before translation.");
}

/// A manifest claim paired with the file it actually came from. When
/// the top-level manifest uses `include:`, claims from included files
/// keep the include file's path as their `source_path` and the
/// per-file index as their `span` — so the resulting `SourceSpan`
/// points at the real authored location instead of the top-level
/// manifest's merged index. Preserves the audit trail.
struct ClaimWithSource {
    claim: ManifestClaim,
    source_path: String,
    span: String,
}

/// Read a manifest YAML and resolve any `include:` entries (paths
/// relative to the manifest's directory). Returns the merged claim
/// list. Per workflow/SCHEMA.md, includes are flat (no chained
/// includes), so we resolve one level only.
fn load_claims(path_str: &str) -> Result<Vec<ClaimWithSource>, String> {
    let path = PathBuf::from(path_str);
    let yaml = fs::read_to_string(&path)
        .map_err(|e| format!("error reading {}: {e}", path.display()))?;

    let manifest = parse_manifest_file(&yaml)
        .map_err(|e| format!("error parsing {}: {e}", path.display()))?;

    let mut out: Vec<ClaimWithSource> = Vec::new();
    for (idx, c) in manifest.claims.into_iter().enumerate() {
        out.push(ClaimWithSource {
            claim: c,
            source_path: path_str.to_string(),
            span: format!("claims[{idx}]"),
        });
    }

    for inc in extract_includes(&yaml) {
        let resolved = path
            .parent()
            .map(|p| p.join(&inc))
            .unwrap_or_else(|| Path::new(&inc).to_path_buf());
        let inc_yaml = fs::read_to_string(&resolved)
            .map_err(|e| format!("error reading include {}: {e}", resolved.display()))?;
        let inc_manifest = parse_manifest_file(&inc_yaml)
            .map_err(|e| format!("error parsing include {}: {e}", resolved.display()))?;
        let inc_path_str = resolved.to_string_lossy().into_owned();
        for (idx, c) in inc_manifest.claims.into_iter().enumerate() {
            out.push(ClaimWithSource {
                claim: c,
                source_path: inc_path_str.clone(),
                span: format!("claims[{idx}]"),
            });
        }
    }

    Ok(out)
}

/// Parse the top-level YAML by hand to extract `include:` paths.
/// The ManifestFile struct in `typed_trust::translate` doesn't carry
/// an `include` field (the schema layer is intentionally small).
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

fn emit_json(
    path: &str,
    now: &str,
    reports: &[serde_json::Value],
    skipped: &[SkipReason],
) -> ExitCode {
    let bundle = serde_json::json!({
        "synthesizer": {
            "name": "typed-trust",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "synthesized_at": now,
        "manifest_path": path,
        "reports": reports,
        "skipped": skipped,
    });
    match serde_json::to_string_pretty(&bundle) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error serializing bundle: {e}");
            ExitCode::FAILURE
        }
    }
}

fn emit_markdown(
    path: &str,
    reports: &[serde_json::Value],
    skipped: &[SkipReason],
) -> ExitCode {
    let mut counts = (0usize, 0usize, 0usize); // current, contested, superseded
    for r in reports {
        match r["status"].as_str() {
            Some("current") => counts.0 += 1,
            Some("contested") => counts.1 += 1,
            Some("superseded") => counts.2 += 1,
            _ => {}
        }
    }

    println!("# Typed Trust rollup\n");
    println!("**Manifest:** `{path}`  ");
    println!(
        "**Reports:** {} ({} current, {} contested, {} superseded)  ",
        reports.len(),
        counts.0,
        counts.1,
        counts.2,
    );
    println!("**Skipped:** {} (out of scope or translation error)  ", skipped.len());
    println!();

    if !reports.is_empty() {
        println!("---\n");
        for r in reports {
            println!("{}", render_markdown(r));
            println!("---\n");
        }
    }

    if !skipped.is_empty() {
        println!("## Skipped\n");
        for s in skipped {
            println!("- `{}` — {}", s.id, s.reason);
        }
    }

    ExitCode::SUCCESS
}
