//! Self-contained HTML report rendering with an embedded Mermaid
//! attestation graph.
//!
//! Produces a single HTML document with inline CSS and a Mermaid
//! script tag pulled from a CDN, so the output can be opened directly
//! in any modern browser without local install or build step.

use serde_json::Value;

use crate::graph::render_mermaid_graph;

/// Inline CSS used by [`render_html`]. Exposed so multi-report rollups
/// can include it in their own `<head>` when embedding fragments via
/// [`render_html_fragment`].
pub const CSS: &str = r#"
* { box-sizing: border-box; }
body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    max-width: 1100px;
    margin: 2em auto;
    padding: 0 1em;
    line-height: 1.55;
    color: #2c3e50;
    background: #fafbfc;
}
h1 { border-bottom: 2px solid #2c3e50; padding-bottom: 0.4em; }
h2 { margin-top: 2.5em; border-bottom: 1px solid #ccc; padding-bottom: 0.3em; }
h3 { margin: 1.2em 0 0.4em; }
.status {
    display: inline-block; padding: 0.15em 0.6em; border-radius: 3px;
    font-weight: 600; font-size: 0.95em;
}
.status-current { background: #d4edda; color: #155724; }
.status-contested { background: #fff3cd; color: #856404; }
.status-superseded { background: #e2e3e5; color: #383d41; }
.criterion {
    border-left: 4px solid #ccc; padding: 0.75em 1em;
    margin: 1em 0; background: #fff;
    border-radius: 0 4px 4px 0;
    box-shadow: 0 1px 2px rgba(0,0,0,0.04);
}
.criterion.pass { border-left-color: #28a745; }
.criterion.fail { border-left-color: #dc3545; }
.criterion.contested { border-left-color: #ffc107; }
.criterion.not-assessed { border-left-color: #6c757d; }
.challenge {
    background: #fff3cd; padding: 1em 1.2em; border-radius: 4px;
    margin: 1em 0; border-left: 4px solid #856404;
}
.review-event {
    padding: 0.75em 1em; border-radius: 4px; margin: 0.6em 0;
    background: #fff; border-left: 4px solid #ccc;
    box-shadow: 0 1px 2px rgba(0,0,0,0.04);
}
.review-event.endorsement { border-left-color: #28a745; background: #f1f9f3; }
.review-event.dissent { border-left-color: #e67e22; background: #fef5ec; }
.panel { background: #fff; padding: 0.75em 1.2em; border-left: 4px solid #6c757d; border-radius: 0 4px 4px 0; margin: 1em 0; box-shadow: 0 1px 2px rgba(0,0,0,0.04); }
.panel-list { list-style: none; padding-left: 0; }
.panel-list li { margin: 0.35em 0; }
.panel-footnote { color: #6c757d; font-size: 0.92em; margin-top: 0.7em; }
.gap {
    background: #f8d7da; padding: 0.6em 1em; border-radius: 4px;
    margin: 0.6em 0; border-left: 4px solid #dc3545;
}
.mermaid {
    background: #fff; padding: 2em; border: 1px solid #dee2e6;
    border-radius: 4px; margin: 1em 0; overflow-x: auto;
}
code, .tolerance {
    background: #f0f0f0; padding: 2px 6px; border-radius: 2px;
    font-family: "SF Mono", Monaco, Consolas, monospace; font-size: 0.92em;
}
.detail-row { margin: 0.35em 0; }
.detail-label { font-weight: 600; color: #495057; }
.result-pass { color: #28a745; font-weight: 600; }
.result-fail { color: #dc3545; font-weight: 600; }
.result-na { color: #6c757d; font-style: italic; }
ul { padding-left: 1.5em; }
.backing-list li { margin: 0.4em 0; }
"#;

/// `<script>` tag that loads Mermaid from a CDN. Exposed so rollups
/// embedding fragments can include it in their own `<head>` once.
pub const MERMAID_SCRIPT: &str = r#"<script type="module">
  import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.esm.min.mjs';
  mermaid.initialize({ startOnLoad: true, theme: 'default' });
</script>"#;

/// Render the augmented TrustReport JSON as a self-contained HTML
/// document (with `<!DOCTYPE>`, `<html>`, `<head>` including the CSS
/// and Mermaid script tag, and a wrapping `<body>`).
///
/// For multi-report rollups, use [`render_html_fragment`] to emit
/// just the inner content (no doctype, no head, no body wrapper) so
/// the rollup can wrap many fragments in a single outer document.
pub fn render_html(augmented_json: &Value) -> String {
    let claim = augmented_json["claim"].as_str().unwrap_or("(unknown)");
    let mut out = String::new();

    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("  <meta charset=\"utf-8\">\n");
    out.push_str(&format!(
        "  <title>Trust Report: {}</title>\n",
        escape_html(claim)
    ));
    out.push_str(&format!("  <style>{CSS}</style>\n"));
    out.push_str(&format!("  {MERMAID_SCRIPT}\n"));
    out.push_str("</head>\n<body>\n");

    out.push_str(&render_html_fragment(augmented_json));

    out.push_str("</body>\n</html>\n");
    out
}

/// Render just the inner HTML content of a TrustReport (no `<!DOCTYPE>`,
/// `<html>`, `<head>`, or `<body>` wrappers). For embedding inside a
/// multi-report rollup that supplies its own document chrome.
pub fn render_html_fragment(augmented_json: &Value) -> String {
    let mut out = String::new();

    let claim = augmented_json["claim"].as_str().unwrap_or("(unknown)");
    let status = augmented_json["status"].as_str().unwrap_or("(unknown)");

    out.push_str("  <h1>Trust Report</h1>\n");
    out.push_str(&format!(
        "  <p><strong>Claim:</strong> <code>{}</code></p>\n",
        escape_html(claim)
    ));
    out.push_str(&format!(
        "  <p><strong>Status:</strong> <span class=\"status status-{}\">{}</span></p>\n",
        status,
        format_status(status)
    ));

    // Attestation graph (Mermaid).
    out.push_str("  <h2>Attestation graph</h2>\n");
    out.push_str("  <div class=\"mermaid\">\n");
    out.push_str(&render_mermaid_graph(augmented_json));
    out.push_str("  </div>\n");

    // Criteria.
    if let Some(criteria) = augmented_json["criteria"].as_array() {
        if !criteria.is_empty() {
            out.push_str("  <h2>Criteria</h2>\n");
            for c in criteria {
                render_criterion(&mut out, c);
            }
        }
    }

    // PR5c: metadata_compatibility claims surface the typed
    // declaration in place of the (absent) criteria section.
    if let Some(md) = augmented_json.get("metadata_declaration") {
        render_metadata_declaration(&mut out, md);
    }

    // PR5f: behavioral_concordance — pattern + paper_locator +
    // prior_binding rendered as a <dl> block.
    if let Some(cd) = augmented_json.get("concordance_declaration") {
        render_concordance_declaration(&mut out, cd);
    }

    // PR5h: comparator verdict.
    if let Some(cr) = augmented_json.get("concordance_result") {
        render_concordance_result(&mut out, cr);
    }

    // PR5i: observation declaration + verdict.
    if let Some(od) = augmented_json.get("observation_declaration") {
        render_observation_declaration(&mut out, od);
    }
    if let Some(or) = augmented_json.get("observation_result") {
        render_observation_result(&mut out, or);
    }

    // Challenges (filtered to kind == challenge).
    if let Some(events) = augmented_json
        .pointer("/_graph/review_events")
        .and_then(|v| v.as_array())
    {
        let challenges: Vec<&Value> = events
            .iter()
            .filter(|e| e["kind"]["type"].as_str() == Some("challenge"))
            .collect();
        if !challenges.is_empty() {
            out.push_str("  <h2>Active challenges</h2>\n");
            for c in challenges {
                render_challenge(&mut out, c);
            }
        }

        // Phase 2c: panel section between challenges and per-kind
        // detail. Only when n_reviewers > 1.
        if let Some(panel) = augmented_json.pointer("/_graph/panel_summary") {
            if panel["n_reviewers"].as_u64().unwrap_or(0) > 1 {
                render_panel(&mut out, panel);
            }
        }

        // Phase 2a: Endorse and Dissent events are surfaced as
        // recorded reviewer activity. Dissent is framed as "evidence
        // found insufficient" — it does not flip Pass to Fail; only
        // backed Challenges do (Phase 2b).
        let endorsements: Vec<&Value> = events
            .iter()
            .filter(|e| e["kind"]["type"].as_str() == Some("endorse"))
            .collect();
        if !endorsements.is_empty() {
            out.push_str("  <h2>Reviewer endorsements</h2>\n");
            for e in endorsements {
                render_review(&mut out, e, "endorses", "endorsement");
            }
        }
        let dissents: Vec<&Value> = events
            .iter()
            .filter(|e| e["kind"]["type"].as_str() == Some("dissent"))
            .collect();
        if !dissents.is_empty() {
            out.push_str(
                "  <h2>Reviewer dissents <small>(evidence found insufficient)</small></h2>\n",
            );
            for e in dissents {
                render_review(&mut out, e, "dissents — evidence insufficient", "dissent");
            }
        }
    }

    // Gaps.
    if let Some(gaps) = augmented_json["gaps"].as_array() {
        if !gaps.is_empty() {
            out.push_str("  <h2>Gaps</h2>\n");
            for g in gaps {
                render_gap(&mut out, g);
            }
        }
    }

    // Backing claims summary.
    if let Some(backing) = augmented_json
        .pointer("/_graph/backing_reports")
        .and_then(|v| v.as_array())
    {
        if !backing.is_empty() {
            out.push_str("  <h2>Backing claims</h2>\n  <ul class=\"backing-list\">\n");
            for b in backing {
                let bclaim = b["claim"].as_str().unwrap_or("?");
                let bstatus = b["status"].as_str().unwrap_or("?");
                let n_crit = b["criteria"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0);
                out.push_str(&format!(
                    "    <li><code>{}</code> — <span class=\"status status-{}\">{}</span> ({} criteria)</li>\n",
                    escape_html(bclaim),
                    bstatus,
                    format_status(bstatus),
                    n_crit,
                ));
            }
            out.push_str("  </ul>\n");
        }
    }

    out
}

fn render_criterion(out: &mut String, c: &Value) {
    let name = c["name"].as_str().unwrap_or("(unnamed)");
    let result_type = c["result"]["value"]["type"].as_str().unwrap_or("?");
    let crit_status = c["result"]["criterion_status"]
        .as_str()
        .unwrap_or("current");

    let class_name = match (result_type, crit_status) {
        (_, "contested") => "contested",
        (_, "superseded") => "not-assessed",
        ("pass", _) => "pass",
        ("fail", _) => "fail",
        _ => "not-assessed",
    };

    out.push_str(&format!(
        "  <div class=\"criterion {class_name}\">\n    <h3>{}</h3>\n",
        escape_html(name)
    ));

    out.push_str(&format!(
        "    <div class=\"detail-row\"><span class=\"detail-label\">Result:</span> {}</div>\n",
        format_result(result_type, c)
    ));
    out.push_str(&format!(
        "    <div class=\"detail-row\"><span class=\"detail-label\">Render status:</span> <span class=\"status status-{crit_status}\">{}</span></div>\n",
        format_status(crit_status)
    ));

    if let Some(observed) = c["result"]["observed_value"].as_f64() {
        out.push_str(&format!(
            "    <div class=\"detail-row\"><span class=\"detail-label\">Observed:</span> <code>{observed}</code></div>\n"
        ));
    }

    if let Some(tol) = c["tolerance"].as_object() {
        let metric = tol.get("metric").and_then(|v| v.as_str()).unwrap_or("?");
        let op = tol.get("op").and_then(|v| v.as_str()).unwrap_or("?");
        let value = tol
            .get("value")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".into());
        let against = tol
            .get("against")
            .and_then(|v| v.as_str())
            .map(|s| format!(" vs <code>{}</code>", escape_html(s)))
            .unwrap_or_default();
        out.push_str(&format!(
            "    <div class=\"detail-row\"><span class=\"detail-label\">Tolerance:</span> <span class=\"tolerance\">{} {} {}</span>{against}</div>\n",
            escape_html(metric),
            escape_html(op),
            value,
        ));
    } else {
        out.push_str(
            "    <div class=\"detail-row\"><span class=\"detail-label\">Tolerance:</span> <em>(prose-only)</em></div>\n",
        );
    }

    if let Some(contested_by) = c["result"]["contested_by"].as_array() {
        if !contested_by.is_empty() {
            out.push_str(
                "    <div class=\"detail-row\"><span class=\"detail-label\">Contested by:</span>\n      <ul>\n",
            );
            for e in contested_by {
                if let Some(s) = e.as_str() {
                    out.push_str(&format!(
                        "        <li><code>{}</code></li>\n",
                        escape_html(s)
                    ));
                }
            }
            out.push_str("      </ul>\n    </div>\n");
        }
    }

    out.push_str("  </div>\n");
}

fn render_metadata_declaration(out: &mut String, md: &Value) {
    let field = md.get("field").and_then(Value::as_str).unwrap_or("?");
    let declared = md
        .get("declared_value")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let source_file = md.get("source_file").and_then(Value::as_str).unwrap_or("?");
    let source_path = md.get("source_path").and_then(Value::as_str).unwrap_or("?");

    out.push_str("  <h2>Metadata declaration</h2>\n");
    out.push_str("  <dl class=\"metadata-declaration\">\n");
    out.push_str(&format!(
        "    <dt>Field</dt><dd><code>{}</code></dd>\n",
        escape_html(field)
    ));
    out.push_str(&format!(
        "    <dt>Declared value</dt><dd><code>{}</code></dd>\n",
        escape_html(declared)
    ));
    out.push_str(&format!(
        "    <dt>Source</dt><dd><code>{}</code> &rarr; <code>{}</code></dd>\n",
        escape_html(source_file),
        escape_html(source_path)
    ));
    out.push_str("  </dl>\n");
}

fn render_concordance_declaration(out: &mut String, cd: &Value) {
    let pattern_kind = cd
        .get("pattern")
        .and_then(|p| p.get("pattern_kind"))
        .and_then(Value::as_str)
        .unwrap_or("?");
    let paper_locator = cd
        .get("paper_locator")
        .and_then(Value::as_str)
        .unwrap_or("?");

    out.push_str("  <h2>Concordance</h2>\n");
    out.push_str("  <dl class=\"concordance-declaration\">\n");
    out.push_str(&format!(
        "    <dt>Pattern</dt><dd><code>{}</code></dd>\n",
        escape_html(pattern_kind)
    ));
    out.push_str(&format!(
        "    <dt>Paper locator</dt><dd><code>{}</code></dd>\n",
        escape_html(paper_locator)
    ));
    if let Some(pb) = cd.get("prior_binding") {
        for (label, key) in [
            ("Prior unit", "prior_unit"),
            ("Prior metric definition", "prior_metric_definition"),
            ("Locator", "locator"),
            ("Extraction note", "prior_extraction_note"),
            ("Source id", "source_id"),
        ] {
            let v = pb.get(key).and_then(Value::as_str).unwrap_or("?");
            out.push_str(&format!(
                "    <dt>{label}</dt><dd><code>{}</code></dd>\n",
                escape_html(v)
            ));
        }
    }
    out.push_str("  </dl>\n");
}

fn render_concordance_result(out: &mut String, cr: &Value) {
    let status = cr
        .get("comparison_status")
        .and_then(Value::as_str)
        .unwrap_or("?");
    out.push_str("  <h2>Concordance result</h2>\n");
    out.push_str(&format!(
        "  <p class=\"concordance-status concordance-status-{}\"><strong>Status:</strong> {}</p>\n",
        escape_html(status),
        escape_html(status),
    ));
    out.push_str("  <dl class=\"concordance-result\">\n");
    if let Some(observed) = cr.get("observed_value").and_then(Value::as_f64) {
        out.push_str(&format!(
            "    <dt>Observed value</dt><dd><code>{}</code></dd>\n",
            observed
        ));
    }
    if let Some(unit) = cr.get("observed_unit").and_then(Value::as_str) {
        out.push_str(&format!(
            "    <dt>Observed unit</dt><dd><code>{}</code></dd>\n",
            escape_html(unit)
        ));
    }
    if let Some(img) = cr.get("image_digest").and_then(Value::as_str) {
        out.push_str(&format!(
            "    <dt>Image digest</dt><dd><code>{}</code></dd>\n",
            escape_html(img)
        ));
    }
    if let Some(at) = cr.get("produced_at").and_then(Value::as_str) {
        out.push_str(&format!(
            "    <dt>Produced at</dt><dd><code>{}</code></dd>\n",
            escape_html(at)
        ));
    }
    out.push_str("  </dl>\n");
}

fn render_challenge(out: &mut String, e: &Value) {
    let id = e["id"].as_str().unwrap_or("?");
    let by_name = e["by"]["name"].as_str().unwrap_or("?");
    let by_kind = e["by"]["kind"].as_str().unwrap_or("?");
    let rationale = e["rationale"].as_str().unwrap_or("");
    let category = e["kind"]["data"]["category"]["type"]
        .as_str()
        .unwrap_or("?");
    let protocol = e["protocol"].as_str();

    out.push_str("  <div class=\"challenge\">\n");
    out.push_str(&format!("    <h3><code>{}</code></h3>\n", escape_html(id)));
    out.push_str(&format!(
        "    <div class=\"detail-row\"><span class=\"detail-label\">Category:</span> {}</div>\n",
        escape_html(category)
    ));
    out.push_str(&format!(
        "    <div class=\"detail-row\"><span class=\"detail-label\">By:</span> {} ({})</div>\n",
        escape_html(by_name),
        escape_html(by_kind)
    ));

    if let Some(details) = e["by"]["details"].as_array() {
        for d in details {
            let k = d["key"].as_str().unwrap_or("?");
            let v = d["value"].as_str().unwrap_or("?");
            out.push_str(&format!(
                "    <div class=\"detail-row\" style=\"margin-left:1em\">{}: <code>{}</code></div>\n",
                escape_html(k),
                escape_html(v)
            ));
        }
    }

    if let Some(p) = protocol {
        out.push_str(&format!(
            "    <div class=\"detail-row\"><span class=\"detail-label\">Protocol:</span> <code>{}</code></div>\n",
            escape_html(p)
        ));
    }

    if let Some(backed) = e["kind"]["data"]["backed_by"].as_str() {
        out.push_str(&format!(
            "    <div class=\"detail-row\"><span class=\"detail-label\">Backed by:</span> <code>{}</code></div>\n",
            escape_html(backed)
        ));
    }

    out.push_str(&format!(
        "    <div class=\"detail-row\"><span class=\"detail-label\">Rationale:</span> {}</div>\n",
        escape_html(rationale)
    ));
    out.push_str("  </div>\n");
}

/// Render an Endorse/Dissent ReviewEvent (Phase 2a). `verb` is the
/// human-readable stance label; `kind_class` selects the CSS class
/// (`endorsement` or `dissent`).
fn render_review(out: &mut String, e: &Value, verb: &str, kind_class: &str) {
    let id = e["id"].as_str().unwrap_or("?");
    let by_name = e["by"]["name"].as_str().unwrap_or("?");
    let by_kind = e["by"]["kind"].as_str().unwrap_or("?");
    let rationale = e["rationale"].as_str().unwrap_or("");
    let at = e["at"].as_str().unwrap_or("");

    out.push_str(&format!(
        "  <div class=\"review-event {kind_class}\">\n"
    ));
    out.push_str(&format!(
        "    <h3><code>{}</code></h3>\n",
        escape_html(id)
    ));
    out.push_str(&format!(
        "    <div class=\"detail-row\"><span class=\"detail-label\">{}:</span> {} ({})</div>\n",
        escape_html(verb),
        escape_html(by_name),
        escape_html(by_kind)
    ));
    if !at.is_empty() {
        out.push_str(&format!(
            "    <div class=\"detail-row\"><span class=\"detail-label\">At:</span> <code>{}</code></div>\n",
            escape_html(at)
        ));
    }

    if let Some(details) = e["by"]["details"].as_array() {
        for d in details {
            let k = d["key"].as_str().unwrap_or("?");
            let v = d["value"].as_str().unwrap_or("?");
            out.push_str(&format!(
                "    <div class=\"detail-row\" style=\"margin-left:1em\">{}: <code>{}</code></div>\n",
                escape_html(k),
                escape_html(v)
            ));
        }
    }

    // Structured submit_review payload (Phase 2a), decorated onto
    // event entries by main.rs::decorate_with_aux.
    if let Some(checks) = e.get("checks").and_then(|v| v.as_object()) {
        out.push_str("    <div class=\"detail-row\"><span class=\"detail-label\">Checks:</span> ");
        let pairs: Vec<String> = checks
            .iter()
            .map(|(k, v)| {
                format!(
                    "<code>{}={}</code>",
                    escape_html(k),
                    escape_html(v.as_str().unwrap_or("?"))
                )
            })
            .collect();
        out.push_str(&pairs.join(", "));
        out.push_str("</div>\n");
    }
    if let Some(obs) = e.get("observed_value").and_then(|v| v.as_str()) {
        out.push_str(&format!(
            "    <div class=\"detail-row\"><span class=\"detail-label\">Observed value:</span> <code>{}</code></div>\n",
            escape_html(obs)
        ));
    }
    if let Some(tol) = e.get("tolerance").and_then(|v| v.as_str()) {
        out.push_str(&format!(
            "    <div class=\"detail-row\"><span class=\"detail-label\">Tolerance:</span> <code>{}</code></div>\n",
            escape_html(tol)
        ));
    }
    if let Some(fr) = e.get("failure_reason").and_then(|v| v.as_str()) {
        out.push_str(&format!(
            "    <div class=\"detail-row\"><span class=\"detail-label\">Failure reason:</span> {}</div>\n",
            escape_html(fr)
        ));
    }

    out.push_str(&format!(
        "    <div class=\"detail-row\"><span class=\"detail-label\">Rationale:</span> {}</div>\n",
        escape_html(rationale)
    ));
    out.push_str("  </div>\n");
}

/// Phase 2c: HTML reviewer-panel section. Mirrors human_render's
/// markdown layout.
fn render_panel(out: &mut String, panel: &Value) {
    let n_reviewers = panel["n_reviewers"].as_u64().unwrap_or(0);
    let n_endorse = panel["n_endorse"].as_u64().unwrap_or(0);
    let n_dissent = panel["n_dissent"].as_u64().unwrap_or(0);
    let n_challenge = panel["n_challenge"].as_u64().unwrap_or(0);
    let n_supersede = panel["n_supersede"].as_u64().unwrap_or(0);

    out.push_str("  <h2>Reviewer panel</h2>\n");
    out.push_str("  <div class=\"panel\">\n");

    let kinds = [
        ("endorsed", n_endorse),
        ("dissented", n_dissent),
        ("challenged", n_challenge),
        ("superseded", n_supersede),
    ];
    let nonzero: Vec<&(&str, u64)> = kinds.iter().filter(|(_, n)| *n > 0).collect();
    if nonzero.len() == 1 {
        let (label, _) = nonzero[0];
        out.push_str(&format!(
            "    <p><strong>{n_reviewers} reviewers, all {label}.</strong></p>\n"
        ));
    } else {
        let parts: Vec<String> = nonzero
            .iter()
            .map(|(label, n)| format!("{n} {label}"))
            .collect();
        out.push_str(&format!(
            "    <p><strong>Panel divergent — {n_reviewers} reviewers:</strong> {}</p>\n",
            escape_html(&parts.join(", "))
        ));
    }

    if let Some(rows) = panel.get("verdicts_by_reviewer").and_then(|v| v.as_array()) {
        out.push_str("    <ul class=\"panel-list\">\n");
        for row in rows {
            let kind = row["author"]["kind"].as_str().unwrap_or("?");
            let name = row["author"]["name"].as_str().unwrap_or("?");
            let version = row["author"].get("version").and_then(|v| v.as_str());
            let verdict = row["kind"].as_str().unwrap_or("?");
            let has_backing = row["has_backing"].as_bool().unwrap_or(false);
            let backed_by = row["backed_by"].as_str();
            let author_label = match version {
                Some(v) => format!("{name} ({v})"),
                None => name.to_string(),
            };
            let backing_fragment = if has_backing {
                match backed_by {
                    Some(b) => format!(
                        ", backed_by=<code>{}</code>",
                        escape_html(b)
                    ),
                    None => ", backed".into(),
                }
            } else {
                String::new()
            };
            out.push_str(&format!(
                "      <li><strong>{}</strong> <code>{}</code> — <em>{}</em>{}</li>\n",
                escape_html(kind),
                escape_html(&author_label),
                escape_html(verdict),
                backing_fragment
            ));
        }
        out.push_str("    </ul>\n");
    }

    // Phase 2d-i removed the "supersedes not yet applied" footnote.
    let _ = n_supersede;
    out.push_str("  </div>\n");
}

fn render_gap(out: &mut String, g: &Value) {
    let desc = g["description"].as_str().unwrap_or("?");
    out.push_str("  <div class=\"gap\">\n");
    out.push_str(&format!("    <p>{}</p>\n", escape_html(desc)));
    if let Some(would) = g["would_satisfy"].as_array() {
        if !would.is_empty() {
            out.push_str("    <ul>\n");
            for w in would {
                if let Some(s) = w.as_str() {
                    out.push_str(&format!(
                        "      <li>Would be satisfied by: <em>{}</em></li>\n",
                        escape_html(s)
                    ));
                }
            }
            out.push_str("    </ul>\n");
        }
    }
    out.push_str("  </div>\n");
}

fn format_status(s: &str) -> &'static str {
    match s {
        "current" => "Current ✓",
        "contested" => "Contested ⚠",
        "superseded" => "Superseded ✗",
        _ => "Unknown",
    }
}

fn format_result(t: &str, c: &Value) -> String {
    match t {
        "pass" => "<span class=\"result-pass\">Pass ✓</span>".into(),
        "fail" => "<span class=\"result-fail\">Fail ✗</span>".into(),
        "not_assessed" => {
            let reason = c["result"]["value"]["data"]["reason"]
                .as_str()
                .unwrap_or("");
            format!("<span class=\"result-na\">Not assessed — {}</span>", escape_html(reason))
        }
        "partial" => {
            let detail = c["result"]["value"]["data"]["detail"]
                .as_str()
                .unwrap_or("");
            format!("<span class=\"result-na\">Partial — {}</span>", escape_html(detail))
        }
        "not_applicable" => "<span class=\"result-na\">Not applicable</span>".into(),
        other => escape_html(other),
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn render_observation_declaration(out: &mut String, od: &Value) {
    let pattern_kind = od
        .get("pattern")
        .and_then(|p| p.get("pattern_kind"))
        .and_then(Value::as_str)
        .unwrap_or("?");
    let third_party = od
        .get("third_party_tool")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let metric_def = od
        .get("metric_definition")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let paper_locator = od
        .get("paper_locator")
        .and_then(Value::as_str)
        .unwrap_or("?");

    out.push_str("  <h2>Observation</h2>\n");
    out.push_str("  <dl class=\"observation-declaration\">\n");
    out.push_str(&format!(
        "    <dt>Pattern</dt><dd><code>{}</code></dd>\n",
        escape_html(pattern_kind)
    ));
    out.push_str(&format!(
        "    <dt>Third-party tool</dt><dd><code>{}</code></dd>\n",
        escape_html(third_party)
    ));
    out.push_str(&format!(
        "    <dt>Metric definition</dt><dd><code>{}</code></dd>\n",
        escape_html(metric_def)
    ));
    out.push_str(&format!(
        "    <dt>Paper locator</dt><dd><code>{}</code></dd>\n",
        escape_html(paper_locator)
    ));
    out.push_str("  </dl>\n");
}

fn render_observation_result(out: &mut String, or: &Value) {
    let status = or
        .get("comparison_status")
        .and_then(Value::as_str)
        .unwrap_or("?");
    out.push_str("  <h2>Observation result</h2>\n");
    out.push_str(&format!(
        "  <p class=\"observation-status observation-status-{}\"><strong>Status:</strong> {}</p>\n",
        escape_html(status),
        escape_html(status),
    ));
    out.push_str("  <dl class=\"observation-result\">\n");
    if let Some(observed) = or.get("observed_value").and_then(Value::as_f64) {
        out.push_str(&format!(
            "    <dt>Observed value</dt><dd><code>{}</code></dd>\n",
            observed
        ));
    }
    if let Some(unit) = or.get("observed_unit").and_then(Value::as_str) {
        out.push_str(&format!(
            "    <dt>Observed unit</dt><dd><code>{}</code></dd>\n",
            escape_html(unit)
        ));
    }
    if let Some(img) = or.get("image_digest").and_then(Value::as_str) {
        out.push_str(&format!(
            "    <dt>Image digest</dt><dd><code>{}</code></dd>\n",
            escape_html(img)
        ));
    }
    if let Some(at) = or.get("produced_at").and_then(Value::as_str) {
        out.push_str(&format!(
            "    <dt>Produced at</dt><dd><code>{}</code></dd>\n",
            escape_html(at)
        ));
    }
    out.push_str("  </dl>\n");
}
