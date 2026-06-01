//! `typed-trust` CLI.
//!
//! Reads a shipping `evident.yaml` (or a per-claim file), translates
//! each measurement-class claim into typed-trust constructors,
//! synthesizes a TrustReport, applies the renderer-aux layer, and
//! emits a JSON bundle to stdout.
//!
//! Usage:
//!   typed-trust <manifest.yaml> [claim_id]
//!
//! With a `claim_id`, only that claim's report is emitted (still
//! inside the bundle envelope). Without, all measurement claims are
//! translated; non-measurement claims appear in the `skipped` array
//! with the OutOfScope reason.

use std::env;
use std::fs;
use std::process::ExitCode;

use typed_trust::translate::{
    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
    TranslationContext,
};
use typed_trust::*;

#[derive(serde::Serialize)]
struct SkipReason {
    id: String,
    reason: String,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: typed-trust <manifest.yaml> [claim_id]");
        eprintln!();
        eprintln!("Translates each measurement-class claim in the manifest,");
        eprintln!("synthesizes a TrustReport per claim, applies the renderer-aux");
        eprintln!("layer, and emits a JSON bundle to stdout. Policy and reference");
        eprintln!("claims appear in `skipped` with the OutOfScope reason.");
        return ExitCode::from(2);
    }
    let path = args[1].clone();
    let filter = args.get(2).cloned();

    let yaml = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let manifest = match parse_manifest_file(&yaml) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error parsing manifest: {e}");
            return ExitCode::FAILURE;
        }
    };

    // CLI MVP: a placeholder synthesis timestamp. Production would
    // wire chrono or `time` for a real ISO-8601 stamp.
    let now: Timestamp = "1970-01-01T00:00:00Z".into();

    let ctx = TranslationContext {
        now: now.clone(),
        manifest_path: path.clone(),
    };

    let mut reports: Vec<serde_json::Value> = Vec::new();
    let mut skipped: Vec<SkipReason> = Vec::new();

    for (idx, mc) in manifest.claims.iter().enumerate() {
        if let Some(ref f) = filter {
            if mc.id != *f {
                continue;
            }
        }
        let span = format!("claims[{idx}]");

        if let Err(e) = translate_claim(&ctx, mc, &span) {
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

        let report = synthesize(
            ClaimId::new(&mc.id),
            criteria,
            &evidence,
            &[],
            &[],
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
