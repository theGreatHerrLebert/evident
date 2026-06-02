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

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use typed_trust::translate::{
    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
    ManifestLastVerified, TranslationContext,
};
use typed_trust::*;

#[derive(serde::Serialize)]
struct SkipReason {
    id: String,
    reason: String,
    /// `true` when the skip indicates a manifest error (an unparseable
    /// or invalid measurement claim) rather than a deliberate scope
    /// boundary. Any fatal skip makes the CLI exit non-zero so CI gates
    /// don't silently pass on broken inputs.
    fatal: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Markdown,
    Html,
    Mermaid,
}

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().collect();
    let parsed = match parse_args(&raw_args) {
        Some(p) => p,
        None => return ExitCode::from(2),
    };
    let format = parsed.format;
    let sidecar_path = parsed.sidecar;
    let positional = parsed.positional;

    let Some(path) = positional.first() else {
        usage();
        return ExitCode::from(2);
    };
    let filter = positional.get(1).cloned();

    let mut claims = match load_claims(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    // Overlay sidecar entries onto each claim's last_verified field
    // before translation. Sidecar key is the claim id; value matches
    // ManifestLastVerified's deserialization shape.
    if let Some(sidecar_path) = &sidecar_path {
        match load_sidecar(sidecar_path) {
            Ok(overlay) => {
                for cw in claims.iter_mut() {
                    if let Some(lv) = overlay.get(&cw.claim.id) {
                        cw.claim.last_verified = Some(lv.clone());
                    }
                }
            }
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }

    let now: Timestamp = "1970-01-01T00:00:00Z".into();

    let mut reports: Vec<serde_json::Value> = Vec::new();
    let mut skipped: Vec<SkipReason> = Vec::new();
    let mut filter_matched = false;

    for cw in claims.iter() {
        let mc = &cw.claim;
        if let Some(ref f) = filter {
            if mc.id != *f {
                continue;
            }
            filter_matched = true;
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
            // OutOfScope is a deliberate scope boundary (policy /
            // reference claims); any other error is a manifest bug.
            let fatal = !matches!(e, typed_trust::translate::TranslateError::OutOfScope { .. });
            skipped.push(SkipReason {
                id: mc.id.clone(),
                reason: format!("{e}"),
                fatal,
            });
            continue;
        }

        let criteria = match translate_tolerances(mc) {
            Ok(c) => c,
            Err(e) => {
                // All translate_tolerances errors at this point are
                // measurement-claim manifest bugs (UnknownOp,
                // PartialTolerance, ProseOnlyOutsideResearch,
                // MeasurementWithoutTolerances).
                skipped.push(SkipReason {
                    id: mc.id.clone(),
                    reason: format!("{e}"),
                    fatal: true,
                });
                continue;
            }
        };

        let evidence: Vec<Evidence> = match translate_evidence(&ctx, mc, &criteria) {
            Ok(opt) => opt.into_iter().collect(),
            Err(e) => {
                // MeasurementWithoutEvidence is a manifest bug.
                skipped.push(SkipReason {
                    id: mc.id.clone(),
                    reason: format!("{e}"),
                    fatal: true,
                });
                continue;
            }
        };

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
        cycle_contested: &std::collections::HashSet::new(),
        });

        reports.push(augmented);
    }

    // A filter that matched nothing is a manifest typo. CI gates with
    // a stale claim id would otherwise see "0 reports, exit 0" and
    // green-light a run that actually checked nothing.
    if let Some(ref f) = filter {
        if !filter_matched {
            eprintln!(
                "error: claim id {f:?} not found in manifest {path}"
            );
            return ExitCode::FAILURE;
        }
    }

    let any_fatal = skipped.iter().any(|s| s.fatal);
    if any_fatal {
        eprintln!(
            "error: {} manifest claim(s) failed translation; see `skipped` in the output",
            skipped.iter().filter(|s| s.fatal).count()
        );
    }

    let render_result = match format {
        Format::Json => emit_json(path, &now, &reports, &skipped),
        Format::Markdown => emit_markdown(path, &reports, &skipped),
        Format::Html => emit_html(path, &reports, &skipped),
        Format::Mermaid => emit_mermaid(&reports),
    };

    // Translation failures override a successful render. CI gates
    // should NOT treat a broken manifest as a passing report.
    if any_fatal {
        ExitCode::FAILURE
    } else {
        render_result
    }
}

struct ParsedArgs {
    format: Format,
    positional: Vec<String>,
    sidecar: Option<String>,
}

fn parse_args(args: &[String]) -> Option<ParsedArgs> {
    if args.len() < 2 || args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
        return None;
    }
    let mut format = Format::Json;
    let mut positional: Vec<String> = Vec::new();
    let mut sidecar: Option<String> = None;
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--format=") {
            format = parse_format_value(value)?;
        } else if arg == "--format" {
            let Some(value) = iter.next() else {
                eprintln!("error: --format requires a value (json|md|html|mermaid)");
                return None;
            };
            format = parse_format_value(value)?;
        } else if let Some(value) = arg.strip_prefix("--last-verified-sidecar=") {
            sidecar = Some(value.to_string());
        } else if arg == "--last-verified-sidecar" {
            let Some(value) = iter.next() else {
                eprintln!("error: --last-verified-sidecar requires a path");
                return None;
            };
            sidecar = Some(value.clone());
        } else {
            positional.push(arg.clone());
        }
    }
    Some(ParsedArgs {
        format,
        positional,
        sidecar,
    })
}

fn parse_format_value(v: &str) -> Option<Format> {
    match v {
        "json" => Some(Format::Json),
        "md" | "markdown" => Some(Format::Markdown),
        "html" => Some(Format::Html),
        "mermaid" => Some(Format::Mermaid),
        other => {
            eprintln!(
                "error: unknown --format {other:?} (expected json, md, html, or mermaid)"
            );
            None
        }
    }
}

fn usage() {
    eprintln!(
        "usage: typed-trust [--format json|md|html|mermaid] \\\n               [--last-verified-sidecar <path>] <manifest.yaml> [claim_id]"
    );
    eprintln!();
    eprintln!("Translates each measurement-class claim, synthesizes a TrustReport,");
    eprintln!("applies the renderer-aux layer, and emits one of:");
    eprintln!("  --format json (default) — one JSON bundle for tools / CI gates");
    eprintln!("  --format md             — markdown rollup for humans");
    eprintln!("  --format html           — self-contained HTML with Mermaid graph");
    eprintln!("  --format mermaid        — just the Mermaid attestation-graph source");
    eprintln!();
    eprintln!("  --last-verified-sidecar <path>");
    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
    eprintln!("    before translation. Sidecar shape: {{claim_id: {{commit, date, value,");
    eprintln!("    corpus_sha}}}}. Matches workflow/evident.py's existing convention.");
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

fn emit_html(
    path: &str,
    reports: &[serde_json::Value],
    skipped: &[SkipReason],
) -> ExitCode {
    let mut counts = (0usize, 0usize, 0usize);
    for r in reports {
        match r["status"].as_str() {
            Some("current") => counts.0 += 1,
            Some("contested") => counts.1 += 1,
            Some("superseded") => counts.2 += 1,
            _ => {}
        }
    }

    println!("<!DOCTYPE html>");
    println!("<html lang=\"en\"><head><meta charset=\"utf-8\">");
    println!("<title>Typed Trust rollup</title>");
    // Reuse the same per-report CSS so embedded fragments style
    // consistently, plus a few rollup-specific tweaks. Same Mermaid
    // script the per-report HTML uses.
    println!("<style>{}\n.rollup{{padding:1em;background:#fff;border-radius:4px;border:1px solid #dee2e6;}}\n.report{{margin:2em 0;padding-bottom:2em;border-bottom:1px solid #dee2e6;}}\n.skipped{{background:#f8f9fa;padding:1em;border-radius:4px;}}</style>", typed_trust::html_render::CSS);
    println!("{}", typed_trust::html_render::MERMAID_SCRIPT);
    println!("</head><body>");
    println!("<h1>Typed Trust rollup</h1>");
    println!("<div class=\"rollup\">");
    println!("<p><strong>Manifest:</strong> <code>{}</code></p>", html_escape(path));
    println!(
        "<p><strong>Reports:</strong> {} ({} current, {} contested, {} superseded)</p>",
        reports.len(),
        counts.0,
        counts.1,
        counts.2
    );
    println!(
        "<p><strong>Skipped:</strong> {} (out of scope or translation error)</p>",
        skipped.len()
    );
    println!("</div>");

    for r in reports {
        println!("<div class=\"report\">");
        // Use the fragment renderer here — render_html() would nest a
        // full <!DOCTYPE>/<html>/<head>/<body> document inside the
        // rollup body, producing invalid HTML. The rollup's <head>
        // above already supplies the CSS and Mermaid script tag.
        println!("{}", typed_trust::render_html_fragment(r));
        println!("</div>");
    }

    if !skipped.is_empty() {
        println!("<h2>Skipped</h2>");
        println!("<div class=\"skipped\"><ul>");
        for s in skipped {
            println!(
                "<li><code>{}</code> — {}</li>",
                html_escape(&s.id),
                html_escape(&s.reason)
            );
        }
        println!("</ul></div>");
    }

    println!("</body></html>");
    ExitCode::SUCCESS
}

fn emit_mermaid(reports: &[serde_json::Value]) -> ExitCode {
    // For multi-report manifests, emit one Mermaid diagram per report
    // separated by a delimiter comment. For single-report runs the
    // output is just the diagram.
    for (i, r) in reports.iter().enumerate() {
        if i > 0 {
            println!();
            println!(
                "%% --- next report: {} ---",
                r["claim"].as_str().unwrap_or("?")
            );
        }
        println!("{}", typed_trust::render_mermaid_graph(r));
    }
    ExitCode::SUCCESS
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Load a sidecar JSON file mapping claim-id → ManifestLastVerified.
/// The shape matches `workflow/evident.py`'s `last_verified.json`
/// convention: each entry has `commit`, `date`, `value`, `corpus_sha`
/// fields, all optional / nullable.
fn load_sidecar(path: &str) -> Result<HashMap<String, ManifestLastVerified>, String> {
    let bytes = fs::read_to_string(path)
        .map_err(|e| format!("error reading sidecar {path}: {e}"))?;
    serde_json::from_str(&bytes)
        .map_err(|e| format!("error parsing sidecar {path}: {e}"))
}
