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

    if let Some(events) = augmented_json
        .get("_graph")
        .and_then(|g| g.get("review_events"))
        .and_then(|v| v.as_array())
    {
        // Filter to Challenge events only — endorsements, dissents,
        // and supersedes share `_graph.review_events` but rendering
        // them under "Active Challenges" would mis-represent normal
        // review activity as objections.
        let challenges: Vec<&Value> = events
            .iter()
            .filter(|e| e["kind"]["type"].as_str() == Some("challenge"))
            .collect();
        if !challenges.is_empty() {
            out.push_str("## Active Challenges\n\n");
            for e in challenges {
                render_challenge_event(&mut out, e);
            }
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
