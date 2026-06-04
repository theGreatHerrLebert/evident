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
use std::process::ExitCode;

use typed_trust::translate::{
    backing_claim_for_event, translate_claim, translate_evidence,
    translate_review_event, translate_tolerances, ManifestClaim, ManifestLastVerified,
    ReviewEventSidecar, TranslationContext,
};
use typed_trust::report::TrustReport;
use typed_trust::review::ReviewEvent;
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
    let review_sidecar_path = parsed.review_sidecar;
    let concorded_sidecar_path = parsed.concorded_sidecar;
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
    let last_verified_overlay: HashMap<String, ManifestLastVerified> =
        if let Some(sidecar_path) = &sidecar_path {
            match load_sidecar(sidecar_path) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            HashMap::new()
        };
    for cw in claims.iter_mut() {
        if let Some(lv) = last_verified_overlay.get(&cw.claim.id) {
            cw.claim.last_verified = Some(lv.clone());
        }
    }

    // PR5h: load `last_concorded.json` (concordance comparator
    // results). The two sidecars are disjoint by claim_id per v4
    // design's sidecar boundary section — measurement claims use
    // `last_verified`, concordance claims use `last_concorded`.
    // Same id appearing in both is a manifest+sidecar bug; we
    // refuse to load.
    let concorded_overlay: HashMap<String, typed_trust::claim::ConcordanceResult> =
        if let Some(p) = &concorded_sidecar_path {
            match load_concorded_sidecar(p) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            HashMap::new()
        };
    let mut id_overlap: Vec<&String> = last_verified_overlay
        .keys()
        .filter(|k| concorded_overlay.contains_key(*k))
        .collect();
    id_overlap.sort();
    if !id_overlap.is_empty() {
        eprintln!(
            "error: {} claim id(s) appear in BOTH last_verified.json and last_concorded.json: {}",
            id_overlap.len(),
            id_overlap
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        eprintln!(
            "hint: measurement claims use last_verified; concordance claims use last_concorded. \
             They MUST be disjoint by claim id."
        );
        return ExitCode::FAILURE;
    }

    // Load and group the review-events sidecar by claim_id. Any event
    // referencing a claim id not present in the manifest is a hard
    // error — silently dropping would mask drift between the agent's
    // run and the current manifest, leaving orphan attestations that
    // cannot be interpreted. Same posture as the round-11 unmatched-
    // filter fix.
    let known_claim_ids: std::collections::HashSet<String> =
        claims.iter().map(|c| c.claim.id.clone()).collect();
    let (
        review_events_by_claim,
        review_event_aux,
        backing_claims_by_target,
        raw_review_events_by_claim,
    ): (
        HashMap<String, Vec<ReviewEvent>>,
        HashMap<String, serde_json::Value>,
        HashMap<String, Vec<ManifestClaim>>,
        HashMap<String, Vec<typed_trust::translate::ManifestReviewEvent>>,
    ) = match review_sidecar_path.as_deref() {
        Some(path) => match load_review_sidecar(path, &known_claim_ids) {
            Ok(quad) => quad,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        },
        None => (HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new()),
    };

    // Phase 5 PR3: fire the promotion gate. An extracted claim
    // (provenance.kind = extracted-from-paper | extracted-from-repo)
    // at tier > research must have a matching PromoteFromExtracted
    // event in the sidecar. Validator is no-op for non-extracted
    // claims and research-tier extracted claims.
    for cw in claims.iter() {
        let empty: Vec<typed_trust::translate::ManifestReviewEvent> = vec![];
        let events_for_claim = raw_review_events_by_claim
            .get(&cw.claim.id)
            .unwrap_or(&empty);
        if let Err(e) =
            typed_trust::translate::validate_promotion_rules(&cw.claim, events_for_claim)
        {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
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

        let typed_claim = match translate_claim(&ctx, mc, &cw.span) {
            Ok(attested) => attested.value,
            Err(e) => {
                // OutOfScope is a deliberate scope boundary (policy /
                // reference claims); any other error is a manifest bug.
                let fatal = !matches!(
                    e,
                    typed_trust::translate::TranslateError::OutOfScope { .. }
                );
                skipped.push(SkipReason {
                    id: mc.id.clone(),
                    reason: format!("{e}"),
                    fatal,
                });
                continue;
            }
        };

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

        let events: &[ReviewEvent] = review_events_by_claim
            .get(&mc.id)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        // Phase 2b: synthesize a TrustReport for every backing claim
        // attached to this target's substantive Challenge events. The
        // backing reports flow into both synthesize() (so the target's
        // render status flips to Contested when a backed Challenge
        // sustains) and render_augmented() (so the rendered output
        // surfaces the backing report under `_graph.backing_reports`).
        //
        // Translation failures on backing claims are surfaced as
        // fatal skips on the target — a broken backing claim must
        // not silently let the Challenge fail to sustain.
        let backing_reports = match synthesize_backing_reports(
            &ctx,
            backing_claims_by_target.get(&mc.id).map(|v| v.as_slice()).unwrap_or(&[]),
            &now,
        ) {
            Ok(rs) => rs,
            Err(e) => {
                skipped.push(SkipReason {
                    id: mc.id.clone(),
                    reason: format!("backing claim translation failed: {e}"),
                    fatal: true,
                });
                continue;
            }
        };

        let report = synthesize(
            ClaimId::new(&mc.id),
            criteria,
            &evidence,
            events,
            &backing_reports,
            &std::collections::HashSet::new(),
            now.clone(),
        );

        let mut augmented = render_augmented(&RenderInput {
            report: &report,
            evidence: &evidence,
            related_events: events,
            backing_reports: &backing_reports,
            cycle_contested: &std::collections::HashSet::new(),
            metadata: typed_claim.metadata.as_ref(),
            concordance: typed_claim.concordance.as_ref(),
            concordance_result: concorded_overlay.get(&mc.id),
            observation: typed_claim.observation.as_ref(),
            observation_result: None,
        });

        // Decorate _graph.review_events entries with their structured
        // submit_review payload (checks/observed_value/tolerance/
        // failure_reason) from the sidecar. ReviewEvent itself doesn't
        // carry these fields — they round-trip via the aux map keyed
        // by event_id.
        decorate_with_aux(&mut augmented, &review_event_aux);

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
    review_sidecar: Option<String>,
    /// PR5h: `--last-concorded-sidecar <path>`. Concordance claims'
    /// status comes from `last_concorded.json`; measurement claims'
    /// from `last_verified.json` (the `sidecar` field above).
    concorded_sidecar: Option<String>,
}

fn parse_args(args: &[String]) -> Option<ParsedArgs> {
    if args.len() < 2 || args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
        return None;
    }
    let mut format = Format::Json;
    let mut positional: Vec<String> = Vec::new();
    let mut sidecar: Option<String> = None;
    let mut review_sidecar: Option<String> = None;
    let mut concorded_sidecar: Option<String> = None;
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
        } else if let Some(value) = arg.strip_prefix("--review-events-sidecar=") {
            review_sidecar = Some(value.to_string());
        } else if arg == "--review-events-sidecar" {
            let Some(value) = iter.next() else {
                eprintln!("error: --review-events-sidecar requires a path");
                return None;
            };
            review_sidecar = Some(value.clone());
        } else if let Some(value) = arg.strip_prefix("--last-concorded-sidecar=") {
            concorded_sidecar = Some(value.to_string());
        } else if arg == "--last-concorded-sidecar" {
            let Some(value) = iter.next() else {
                eprintln!("error: --last-concorded-sidecar requires a path");
                return None;
            };
            concorded_sidecar = Some(value.clone());
        } else {
            positional.push(arg.clone());
        }
    }
    Some(ParsedArgs {
        format,
        positional,
        sidecar,
        review_sidecar,
        concorded_sidecar,
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
        "usage: typed-trust [--format json|md|html|mermaid] \\\n               [--last-verified-sidecar <path>] \\\n               [--review-events-sidecar <path>] <manifest.yaml> [claim_id]"
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
    eprintln!("  --review-events-sidecar <path>");
    eprintln!("    inject ReviewEvents from an append-only sidecar. Shape:");
    eprintln!("    {{events: [ {{event_id, claim_id, kind, author, rationale,");
    eprintln!("    timestamp, ...}} ]}}. Endorse / Dissent only in Phase 2a;");
    eprintln!("    `kind: challenge` is rejected pending Phase 2b. Any event whose");
    eprintln!("    `claim_id` isn't in the manifest causes exit 1.");
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
///
/// CLI uses the permissive policy; the MCP server (Phase 3) passes
/// an `AllowListPathPolicy` via the library's
/// `load_claims_with_policy`.
fn load_claims(path_str: &str) -> Result<Vec<ClaimWithSource>, String> {
    typed_trust::loader::load_claims(path_str)
        .map(|claims| {
            claims
                .into_iter()
                .map(|c| ClaimWithSource {
                    claim: c.claim,
                    source_path: c.source_path,
                    span: c.span,
                })
                .collect()
        })
        .map_err(|e| e.to_string())
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

/// PR5h: load `last_concorded.json`. The shape matches the Python
/// agent's `LastConcordedEntry` exactly, so the Rust side just
/// deserializes through `ConcordanceResult`.
fn load_concorded_sidecar(
    path: &str,
) -> Result<HashMap<String, typed_trust::claim::ConcordanceResult>, String> {
    let bytes = fs::read_to_string(path)
        .map_err(|e| format!("error reading concorded sidecar {path}: {e}"))?;
    serde_json::from_str(&bytes)
        .map_err(|e| format!("error parsing concorded sidecar {path}: {e}"))
}

/// Load a review-events sidecar (`{events: [...]}` shape) and group
/// entries by `claim_id`. Any entry referencing a claim id not in the
/// manifest is a hard error — see comment at call site.
///
/// Returns:
/// - `events_by_claim`: ReviewEvents grouped by claim id, fed to
///   synthesize() and render_augmented().
/// - `aux_by_event_id`: structured submit_review payload (checks,
///   observed_value, tolerance, failure_reason) keyed by event_id.
///   ReviewEvent doesn't carry these fields directly; they're
///   decorated onto the rendered `_graph.review_events` entries by
///   `decorate_with_aux`.
fn load_review_sidecar(
    path: &str,
    known_claim_ids: &std::collections::HashSet<String>,
) -> Result<
    (
        HashMap<String, Vec<ReviewEvent>>,
        HashMap<String, serde_json::Value>,
        HashMap<String, Vec<ManifestClaim>>,
        // Phase 5 PR3: raw ManifestReviewEvent entries grouped by
        // claim_id. validate_promotion_rules needs the raw entries
        // (the translated ReviewEvent loses the per-entry
        // promote_from_extracted block).
        HashMap<String, Vec<typed_trust::translate::ManifestReviewEvent>>,
    ),
    String,
> {
    let bytes = fs::read_to_string(path)
        .map_err(|e| format!("error reading review-events sidecar {path}: {e}"))?;
    let parsed: ReviewEventSidecar = serde_json::from_str(&bytes)
        .map_err(|e| format!("error parsing review-events sidecar {path}: {e}"))?;

    // Reject any sidecar event whose claim_id isn't in the manifest
    // before doing any translation work.
    let mut unknown: Vec<String> = parsed
        .events
        .iter()
        .map(|e| e.claim_id.clone())
        .filter(|cid| !known_claim_ids.contains(cid))
        .collect();
    unknown.sort();
    unknown.dedup();
    if !unknown.is_empty() {
        return Err(format!(
            "error: review-events sidecar {path} references {} unknown claim id(s): {}",
            unknown.len(),
            unknown.join(", ")
        ));
    }

    let mut grouped: HashMap<String, Vec<ReviewEvent>> = HashMap::new();
    let mut aux: HashMap<String, serde_json::Value> = HashMap::new();
    let mut backing: HashMap<String, Vec<ManifestClaim>> = HashMap::new();
    let mut raw_by_claim: HashMap<
        String,
        Vec<typed_trust::translate::ManifestReviewEvent>,
    > = HashMap::new();
    // Phase 2d-i (codex F-2D-12): reject duplicate event_ids at
    // load time. If two entries share the same id, Supersede
    // semantics become ambiguous (Target::ReviewEvent(id) can't
    // pick which event the Supersede applies to). Sources of
    // duplicates: explicit event_id collisions in hand-authored
    // sidecars, two truly-identical payloads producing identical
    // canonical hashes, append-merge bugs. Loader treats all as
    // hard errors.
    let mut seen_event_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for entry in &parsed.events {
        let event = translate_review_event(entry)
            .map_err(|e| format!("error translating review event in {path}: {e}"))?;
        let event_id_str = event.id.as_str().to_string();
        if !seen_event_ids.insert(event_id_str.clone()) {
            return Err(format!(
                "error: review-events sidecar {path} contains duplicate event_id {event_id_str:?} — ambiguity in Supersede target resolution would result"
            ));
        }
        grouped
            .entry(entry.claim_id.clone())
            .or_default()
            .push(event);
        raw_by_claim
            .entry(entry.claim_id.clone())
            .or_default()
            .push(entry.clone());
        let aux_value = aux_value_for(entry);
        if let serde_json::Value::Object(ref m) = aux_value {
            if !m.is_empty() {
                aux.insert(event_id_str, aux_value);
            }
        }
        // Phase 2b: collect inline backing claims keyed by target id.
        // backing_claim_for_event returns None for procedural and
        // non-Challenge events.
        if let Some(bc) = backing_claim_for_event(entry) {
            backing
                .entry(entry.claim_id.clone())
                .or_default()
                .push(bc.clone());
        }
    }
    Ok((grouped, aux, backing, raw_by_claim))
}

/// Synthesize a `TrustReport` for each inline backing claim attached
/// to a target's substantive Challenge events. Reuses the regular
/// translation pipeline so backing claims go through the same
/// validation as top-level manifest claims.
///
/// Translation errors are surfaced; if any backing claim fails to
/// translate, the caller treats it as a fatal skip on the target
/// (the Challenge cannot sustain a backing that doesn't exist).
fn synthesize_backing_reports(
    ctx: &TranslationContext,
    backing_claims: &[ManifestClaim],
    now: &Timestamp,
) -> Result<Vec<TrustReport>, String> {
    let mut out: Vec<TrustReport> = Vec::with_capacity(backing_claims.len());
    for (idx, bc) in backing_claims.iter().enumerate() {
        let span = format!("review_events_sidecar.events[?].challenge.backing_claim[{idx}]");
        translate_claim(ctx, bc, &span)
            .map_err(|e| format!("backing claim {}: {e}", bc.id))?;
        let bc_criteria = translate_tolerances(bc)
            .map_err(|e| format!("backing claim {}: {e}", bc.id))?;
        let bc_evidence: Vec<Evidence> = translate_evidence(ctx, bc, &bc_criteria)
            .map_err(|e| format!("backing claim {}: {e}", bc.id))?
            .into_iter()
            .collect();
        let bc_report = synthesize(
            ClaimId::new(&bc.id),
            bc_criteria,
            &bc_evidence,
            // Backing claims in Phase 2b are leaves: no events of
            // their own, no recursive backing. typed-trust would
            // accept events here if we passed them in, but the
            // schema rejects nested review_events on backing claims
            // (depth > 1) at translation time.
            &[],
            &[],
            &std::collections::HashSet::new(),
            now.clone(),
        );
        out.push(bc_report);
    }
    Ok(out)
}

/// Build the JSON aux block (checks / observed_value / tolerance /
/// failure_reason) for one sidecar entry. Empty fields are omitted so
/// the renderer doesn't print empty rows.
fn aux_value_for(entry: &typed_trust::translate::ManifestReviewEvent) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    if let Some(c) = &entry.checks {
        m.insert("checks".into(), c.clone());
    }
    if let Some(o) = &entry.observed_value {
        m.insert("observed_value".into(), serde_json::Value::String(o.clone()));
    }
    if let Some(t) = &entry.tolerance {
        m.insert("tolerance".into(), serde_json::Value::String(t.clone()));
    }
    if let Some(fr) = &entry.failure_reason {
        m.insert(
            "failure_reason".into(),
            serde_json::Value::String(fr.clone()),
        );
    }
    serde_json::Value::Object(m)
}

/// Walk the augmented JSON's `_graph.review_events` array (if present)
/// and merge the matching aux fields from `aux_by_event_id` onto each
/// event entry, looked up by `id`. Renderers read these fields
/// top-level on the event (`e["checks"]` etc.).
fn decorate_with_aux(
    augmented: &mut serde_json::Value,
    aux_by_event_id: &HashMap<String, serde_json::Value>,
) {
    let Some(events) = augmented
        .pointer_mut("/_graph/review_events")
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };
    for event in events.iter_mut() {
        let Some(id) = event.get("id").and_then(|v| v.as_str()).map(String::from) else {
            continue;
        };
        let Some(aux) = aux_by_event_id.get(&id) else {
            continue;
        };
        let Some(aux_obj) = aux.as_object() else {
            continue;
        };
        let Some(event_obj) = event.as_object_mut() else {
            continue;
        };
        for (k, v) in aux_obj {
            event_obj.insert(k.clone(), v.clone());
        }
    }
}
