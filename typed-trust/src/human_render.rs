//! Human-facing markdown rendering of an augmented TrustReport.
//!
//! Takes the JSON produced by [`render_augmented`](crate::render_augmented)
//! and emits a markdown summary suitable for a PR comment, a release
//! page, or a documentation block. Not normative — the JSON is the
//! source of truth; this layer is presentation.

use serde_json::Value;

/// Render the augmented TrustReport JSON as a markdown document.
pub fn render_markdown(augmented_json: &Value) -> String {
    let mut out = String::new();

    let claim = augmented_json["claim"].as_str().unwrap_or("(unknown)");
    let status = augmented_json["status"].as_str().unwrap_or("(unknown)");

    out.push_str(&format!("# Trust Report\n\n"));
    out.push_str(&format!("**Claim:** `{claim}`  \n"));
    out.push_str(&format!(
        "**Status:** {}\n\n",
        status_label(status)
    ));

    if let Some(criteria) = augmented_json["criteria"].as_array() {
        if !criteria.is_empty() {
            out.push_str("## Criteria\n\n");
            for c in criteria {
                render_criterion(&mut out, c);
            }
        }
    }

    // PR5c: metadata_compatibility claims have no criteria — their
    // declaration IS the evidence. Surface the typed declaration in
    // place of the criteria section.
    if let Some(md) = augmented_json.get("metadata_declaration") {
        render_metadata_declaration(&mut out, md);
    }

    if let Some(events) = augmented_json
        .get("_graph")
        .and_then(|g| g.get("review_events"))
        .and_then(|v| v.as_array())
    {
        // Phase 2d-i: filter per-kind sections to ACTIVE verdicts
        // only. Superseded events still appear in the report, but in
        // the dedicated `## Superseded Events` section below.
        let active_event_ids = active_event_ids_from_panel(augmented_json);
        let is_active = |e: &&Value| {
            // No panel_summary present (single-event report) → treat
            // every event as active. This preserves Phase 2a/b/c
            // rendering when the projection wasn't built.
            match &active_event_ids {
                Some(ids) => e["id"]
                    .as_str()
                    .map(|id| ids.contains(id))
                    .unwrap_or(false),
                None => true,
            }
        };

        let challenges: Vec<&Value> = events
            .iter()
            .filter(|e| e["kind"]["type"].as_str() == Some("challenge"))
            .filter(is_active)
            .collect();
        if !challenges.is_empty() {
            out.push_str("## Active Challenges\n\n");
            for e in challenges {
                render_challenge_event(&mut out, e);
            }
        }

        // Phase 2c: reviewer panel. Only rendered when more than one
        // distinct reviewer authored events on this claim.
        if let Some(panel) = augmented_json
            .get("_graph")
            .and_then(|g| g.get("panel_summary"))
        {
            if panel["n_reviewers"].as_u64().unwrap_or(0) > 1 {
                render_panel_section(&mut out, panel);
            }
        }

        // Phase 2a: reviewer endorsements and dissents are surfaced
        // here so model-authored attestations are visible in the
        // rendered report. Phase 2d filters to active only.
        let endorsements: Vec<&Value> = events
            .iter()
            .filter(|e| e["kind"]["type"].as_str() == Some("endorse"))
            .filter(is_active)
            .collect();
        if !endorsements.is_empty() {
            out.push_str("## Reviewer Endorsements\n\n");
            for e in endorsements {
                render_review_event(&mut out, e, "endorses");
            }
        }
        let dissents: Vec<&Value> = events
            .iter()
            .filter(|e| e["kind"]["type"].as_str() == Some("dissent"))
            .filter(is_active)
            .collect();
        if !dissents.is_empty() {
            out.push_str("## Reviewer Dissents (evidence found insufficient)\n\n");
            for e in dissents {
                render_review_event(&mut out, e, "dissents — evidence insufficient");
            }
        }

        // Phase 2d: consolidated audit section for superseded events.
        // Three subsections in fixed order (valid pairs → unresolved
        // → invalid); each subsection sorted by (timestamp, event_id)
        // by the projection layer.
        if let Some(panel) = augmented_json
            .get("_graph")
            .and_then(|g| g.get("panel_summary"))
        {
            render_superseded_section(&mut out, panel);
        }
    }

    if let Some(gaps) = augmented_json["gaps"].as_array() {
        if !gaps.is_empty() {
            out.push_str("## Gaps\n\n");
            for g in gaps {
                render_gap(&mut out, g);
            }
        }
    }

    if let Some(backing) = augmented_json
        .get("_graph")
        .and_then(|g| g.get("backing_reports"))
        .and_then(|v| v.as_array())
    {
        if !backing.is_empty() {
            out.push_str("## Backing Claims\n\n");
            for b in backing {
                let claim = b["claim"].as_str().unwrap_or("?");
                let s = b["status"].as_str().unwrap_or("?");
                let n_crit = b["criteria"].as_array().map(|a| a.len()).unwrap_or(0);
                out.push_str(&format!(
                    "- `{claim}` — {} ({n_crit} criteria)\n",
                    status_label(s)
                ));
            }
            out.push('\n');
        }
    }

    out
}

fn render_metadata_declaration(out: &mut String, md: &Value) {
    let field = md.get("field").and_then(Value::as_str).unwrap_or("?");
    let declared = md
        .get("declared_value")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let source_file = md.get("source_file").and_then(Value::as_str).unwrap_or("?");
    let source_path = md.get("source_path").and_then(Value::as_str).unwrap_or("?");

    out.push_str("## Metadata declaration\n\n");
    out.push_str(&format!("- **Field:** {}\n", inline_code(field)));
    out.push_str(&format!(
        "- **Declared value:** {}\n",
        inline_code(declared)
    ));
    out.push_str(&format!(
        "- **Source:** {} → {}\n\n",
        inline_code(source_file),
        inline_code(source_path),
    ));
}

/// Wrap a string in a markdown inline-code span, defending against
/// backticks and newlines in the value so an extracted-manifest
/// string can't break out of the code span and inject markdown
/// (codex F-PR5c-CR3). Strategy: collapse newlines to spaces, pick
/// the shortest backtick run that doesn't appear in the value, and
/// wrap with that fence.
fn inline_code(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    // Find the shortest backtick run not in the value; wrap with
    // (run+1) backticks and surround by a space if value starts/ends
    // with backtick — the GFM trick.
    let max_run = max_backtick_run(&sanitized);
    let fence: String = "`".repeat(max_run + 1);
    let needs_pad = sanitized.starts_with('`') || sanitized.ends_with('`');
    if needs_pad {
        format!("{fence} {sanitized} {fence}")
    } else {
        format!("{fence}{sanitized}{fence}")
    }
}

fn max_backtick_run(s: &str) -> usize {
    let mut max = 0usize;
    let mut cur = 0usize;
    for c in s.chars() {
        if c == '`' {
            cur += 1;
            if cur > max {
                max = cur;
            }
        } else {
            cur = 0;
        }
    }
    max
}

fn status_label(s: &str) -> &'static str {
    match s {
        "current" => "Current ✓",
        "contested" => "Contested ⚠",
        "superseded" => "Superseded ✗",
        _ => "Unknown",
    }
}

fn render_criterion(out: &mut String, c: &Value) {
    let name = c["name"].as_str().unwrap_or("(unnamed)");
    let crit_status = c["result"]["criterion_status"]
        .as_str()
        .unwrap_or("current");
    let result_type = c["result"]["value"]["type"].as_str().unwrap_or("?");

    out.push_str(&format!("### {name}\n\n"));
    out.push_str(&format!(
        "- **Result:** {}\n",
        result_label(result_type, c)
    ));
    out.push_str(&format!(
        "- **Render status:** {}\n",
        status_label(crit_status)
    ));

    if let Some(observed) = c["result"]["observed_value"].as_f64() {
        out.push_str(&format!("- **Observed value:** `{observed}`\n"));
    }

    if let Some(tol) = c["tolerance"].as_object() {
        let metric = tol.get("metric").and_then(|v| v.as_str()).unwrap_or("?");
        let op = tol.get("op").and_then(|v| v.as_str()).unwrap_or("?");
        let value = tol
            .get("value")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".into());
        let output = tol
            .get("output")
            .and_then(|v| v.as_str())
            .map(|s| format!(" on `{s}`"))
            .unwrap_or_default();
        let against = tol
            .get("against")
            .and_then(|v| v.as_str())
            .map(|s| format!(" vs `{s}`"))
            .unwrap_or_default();
        out.push_str(&format!(
            "- **Tolerance:** `{metric} {op} {value}`{output}{against}\n"
        ));
    } else {
        out.push_str("- **Tolerance:** (prose-only)\n");
    }

    if let Some(contested) = c["result"]["contested_by"].as_array() {
        out.push_str("- **Contested by:**\n");
        for evt in contested {
            if let Some(s) = evt.as_str() {
                out.push_str(&format!("  - `{s}`\n"));
            }
        }
    }

    out.push('\n');
}

fn result_label(t: &str, c: &Value) -> String {
    match t {
        "pass" => "Pass ✓".into(),
        "fail" => "Fail ✗".into(),
        "not_assessed" => {
            let reason = c["result"]["value"]["data"]["reason"]
                .as_str()
                .unwrap_or("");
            format!("Not assessed — {reason}")
        }
        "partial" => {
            let detail = c["result"]["value"]["data"]["detail"]
                .as_str()
                .unwrap_or("");
            format!("Partial — {detail}")
        }
        "not_applicable" => "Not applicable".into(),
        other => other.into(),
    }
}

fn render_challenge_event(out: &mut String, e: &Value) {
    let id = e["id"].as_str().unwrap_or("?");
    let target_type = e["target"]["type"].as_str().unwrap_or("?");
    let target_data = e["target"]["data"].as_str().unwrap_or("");
    let by_name = e["by"]["name"].as_str().unwrap_or("?");
    let by_kind = e["by"]["kind"].as_str().unwrap_or("?");
    let rationale = e["rationale"].as_str().unwrap_or("");
    let kind_type = e["kind"]["type"].as_str().unwrap_or("?");

    out.push_str(&format!("### `{id}`\n\n"));
    out.push_str(&format!("- **Kind:** {kind_type}\n"));
    out.push_str(&format!(
        "- **Target:** {target_type} `{target_data}`\n"
    ));
    out.push_str(&format!("- **By:** {by_name} ({by_kind})\n"));

    if let Some(details) = e["by"]["details"].as_array() {
        for d in details {
            let k = d["key"].as_str().unwrap_or("?");
            let v = d["value"].as_str().unwrap_or("?");
            out.push_str(&format!("  - {k}: `{v}`\n"));
        }
    }

    if let Some(protocol) = e["protocol"].as_str() {
        out.push_str(&format!("- **Protocol:** `{protocol}`\n"));
    }

    if let Some(category) = e["kind"]["data"]["category"]["type"].as_str() {
        out.push_str(&format!("- **Category:** {category}\n"));
    }

    if let Some(backed_by) = e["kind"]["data"]["backed_by"].as_str() {
        out.push_str(&format!("- **Backed by:** `{backed_by}`\n"));
    }

    out.push_str(&format!("- **Rationale:** {rationale}\n\n"));
}

/// Render an Endorse/Dissent ReviewEvent (Phase 2a). Compact: author,
/// optional model version, structured per-check verdicts when present,
/// then the rationale. The `verb` parameter is the human-readable
/// stance string ("endorses" / "dissents — evidence insufficient").
fn render_review_event(out: &mut String, e: &Value, verb: &str) {
    let by_kind = e["by"]["kind"].as_str().unwrap_or("?");
    let by_name = e["by"]["name"].as_str().unwrap_or("?");
    let rationale = e["rationale"].as_str().unwrap_or("");
    let timestamp = e["at"].as_str().unwrap_or("");

    out.push_str(&format!(
        "- **{by_name}** ({by_kind}) {verb}"
    ));
    if !timestamp.is_empty() {
        out.push_str(&format!(" — _{timestamp}_"));
    }
    out.push('\n');

    // Surface details (model version, ci_run, etc.) on a continuation
    // line so the renderer agrees with the Identity model.
    if let Some(details) = e["by"]["details"].as_array() {
        for d in details {
            let k = d["key"].as_str().unwrap_or("?");
            let v = d["value"].as_str().unwrap_or("?");
            out.push_str(&format!("  - {k}: `{v}`\n"));
        }
    }

    // Structured submit_review payload (Phase 2a). Decorated onto each
    // event entry by main.rs after render_augmented, sourced from the
    // sidecar's ManifestReviewEvent. Show which checks the model ran,
    // not just the overall verdict.
    if let Some(checks) = e.get("checks").and_then(|v| v.as_object()) {
        out.push_str("  - **Checks:** ");
        let pairs: Vec<String> = checks
            .iter()
            .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or("?")))
            .collect();
        out.push_str(&pairs.join(", "));
        out.push('\n');
    }
    if let Some(obs) = e.get("observed_value").and_then(|v| v.as_str()) {
        out.push_str(&format!("  - **Observed value:** `{obs}`\n"));
    }
    if let Some(tol) = e.get("tolerance").and_then(|v| v.as_str()) {
        out.push_str(&format!("  - **Tolerance:** `{tol}`\n"));
    }
    if let Some(fr) = e.get("failure_reason").and_then(|v| v.as_str()) {
        out.push_str(&format!("  - **Failure reason:** {fr}\n"));
    }

    if !rationale.is_empty() {
        out.push_str(&format!("  - **Rationale:** {rationale}\n"));
    }
    out.push('\n');
}

/// Phase 2c: render the reviewer-panel section from `panel_summary`.
/// Only called when `n_reviewers > 1`; the caller gates that.
fn render_panel_section(out: &mut String, panel: &Value) {
    let n_reviewers = panel["n_reviewers"].as_u64().unwrap_or(0);
    let n_endorse = panel["n_endorse"].as_u64().unwrap_or(0);
    let n_dissent = panel["n_dissent"].as_u64().unwrap_or(0);
    let n_challenge = panel["n_challenge"].as_u64().unwrap_or(0);
    let n_supersede = panel["n_supersede"].as_u64().unwrap_or(0);

    out.push_str("## Reviewer Panel\n\n");

    // Consensus: a single kind > 0 means all reviewers agreed.
    let kinds_nonzero = [
        ("endorsed", n_endorse),
        ("dissented", n_dissent),
        ("challenged", n_challenge),
        ("superseded", n_supersede),
    ];
    let nonzero_kinds: Vec<&(&str, u64)> = kinds_nonzero.iter().filter(|(_, n)| *n > 0).collect();
    if nonzero_kinds.len() == 1 {
        let (label, _) = nonzero_kinds[0];
        out.push_str(&format!(
            "**{n_reviewers} reviewers, all {label}.**\n\n"
        ));
    } else {
        out.push_str(&format!(
            "**Panel divergent — {n_reviewers} reviewers:** "
        ));
        let parts: Vec<String> = nonzero_kinds
            .iter()
            .map(|(label, n)| format!("{n} {label}"))
            .collect();
        out.push_str(&parts.join(", "));
        out.push_str(".\n\n");
    }

    if let Some(rows) = panel.get("verdicts_by_reviewer").and_then(|v| v.as_array()) {
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
            let mut line = format!("- **{kind}** `{author_label}` — _{verdict}_");
            if has_backing {
                if let Some(b) = backed_by {
                    line.push_str(&format!(", backed_by=`{b}`"));
                } else {
                    line.push_str(", backed");
                }
            }
            line.push('\n');
            out.push_str(&line);
        }
        out.push('\n');
    }

    // Phase 2d-i removed the "supersedes not yet applied" footnote
    // — the panel now reflects ACTIVE verdicts. The audit material
    // (superseded pairs, unresolved, invalid) appears in the
    // dedicated `## Superseded Events` section below.
    let _ = n_supersede;
}

/// Build the active-event-id set from `_graph.panel_summary.
/// verdicts_by_reviewer` if present. Returns None when there's no
/// panel_summary (no projection has been run) — callers treat that
/// as "show every event" to preserve pre-Phase-2d rendering.
fn active_event_ids_from_panel(augmented_json: &Value) -> Option<std::collections::HashSet<String>> {
    let rows = augmented_json
        .pointer("/_graph/panel_summary/verdicts_by_reviewer")?
        .as_array()?;
    Some(
        rows.iter()
            .filter_map(|row| row["event_id"].as_str().map(String::from))
            .collect(),
    )
}

/// Phase 2d: render the consolidated `## Superseded Events`
/// section. Three ordered subsections (valid pairs → unresolved →
/// invalid); each subsection sorted by (timestamp, event_id) by
/// the projection layer. Section is omitted when all three
/// subsections are empty.
fn render_superseded_section(out: &mut String, panel: &Value) {
    let pairs = panel
        .get("superseded_pairs")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let unresolved = panel
        .get("unresolved_supersedes")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let invalid = panel
        .get("invalid_supersedes")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    if pairs.is_empty() && unresolved.is_empty() && invalid.is_empty() {
        return;
    }

    out.push_str("## Superseded Events\n\n");

    if !pairs.is_empty() {
        out.push_str("### Valid superseded pairs\n\n");
        for pair in pairs {
            render_superseded_pair(out, pair);
        }
    }
    if !unresolved.is_empty() {
        out.push_str("### Unresolved supersedes (target not in this slice)\n\n");
        for s in unresolved {
            render_lone_supersede(out, s, "target event not present in this claim's slice");
        }
    }
    if !invalid.is_empty() {
        out.push_str("### Invalid supersede chain\n\n");
        for s in invalid {
            render_lone_supersede(out, s, "meta-supersede / cycle / self-target — original verdict NOT reactivated");
        }
    }
}

/// One valid superseded pair: original verdict + the Supersede
/// event that retired it. Surfaces full audit context (event id,
/// author kind/name, timestamp, supersede rationale, successor id,
/// cross-author labeling).
fn render_superseded_pair(out: &mut String, pair: &Value) {
    let original = &pair["original"];
    let supersede = &pair["supersede"];

    let original_id = original["id"].as_str().unwrap_or("?");
    let original_kind = original["kind"]["type"].as_str().unwrap_or("?");
    let original_author_kind = original["by"]["kind"].as_str().unwrap_or("?");
    let original_author_name = original["by"]["name"].as_str().unwrap_or("?");
    let original_at = original["at"].as_str().unwrap_or("");

    let supersede_id = supersede["id"].as_str().unwrap_or("?");
    let supersede_author_kind = supersede["by"]["kind"].as_str().unwrap_or("?");
    let supersede_author_name = supersede["by"]["name"].as_str().unwrap_or("?");
    let supersede_at = supersede["at"].as_str().unwrap_or("");
    let supersede_rationale = supersede["rationale"].as_str().unwrap_or("");
    let successor = supersede["kind"]["data"]["successor"]
        .as_str()
        .unwrap_or("?");

    let cross_author_label = if original_author_name == supersede_author_name
        && original_author_kind == supersede_author_kind
    {
        ""
    } else {
        " (cross-author supersede)"
    };

    out.push_str(&format!(
        "- **Original**: `{original_id}` — {original_author_kind} `{original_author_name}` {original_kind}ed at _{original_at}_.\n"
    ));
    out.push_str(&format!(
        "  - **Superseded by**: `{supersede_id}` — {supersede_author_kind} `{supersede_author_name}` at _{supersede_at}_{cross_author_label}.\n"
    ));
    out.push_str(&format!("  - **Successor**: `{successor}`.\n"));
    if !supersede_rationale.is_empty() {
        out.push_str(&format!("  - **Rationale**: {supersede_rationale}\n"));
    }
    out.push('\n');
}

/// One lone Supersede event (unresolved or invalid). `framing` is
/// the explanatory sentence that distinguishes the two subsections.
fn render_lone_supersede(out: &mut String, supersede: &Value, framing: &str) {
    let id = supersede["id"].as_str().unwrap_or("?");
    let author_kind = supersede["by"]["kind"].as_str().unwrap_or("?");
    let author_name = supersede["by"]["name"].as_str().unwrap_or("?");
    let at = supersede["at"].as_str().unwrap_or("");
    let rationale = supersede["rationale"].as_str().unwrap_or("");
    let target_id = supersede["target"]["data"].as_str().unwrap_or("?");

    out.push_str(&format!(
        "- **Supersede event**: `{id}` — {author_kind} `{author_name}` at _{at}_.\n"
    ));
    out.push_str(&format!("  - **Targets**: event `{target_id}`.\n"));
    out.push_str(&format!("  - **Note**: {framing}.\n"));
    if !rationale.is_empty() {
        out.push_str(&format!("  - **Rationale**: {rationale}\n"));
    }
    out.push('\n');
}

fn render_gap(out: &mut String, g: &Value) {
    let desc = g["description"].as_str().unwrap_or("?");
    out.push_str(&format!("- {desc}\n"));
    if let Some(would) = g["would_satisfy"].as_array() {
        for w in would {
            if let Some(s) = w.as_str() {
                out.push_str(&format!("  - Would be satisfied by: _{s}_\n"));
            }
        }
    }
}
