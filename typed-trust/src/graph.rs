//! Mermaid graph rendering of a TrustReport's attestation graph.
//!
//! Takes the augmented JSON produced by [`render_augmented`](crate::render_augmented)
//! and emits a Mermaid diagram source string. The diagram shows:
//!
//! - The Claim node (top-level).
//! - One Criterion node per criteria entry, colored by result status.
//! - Challenge nodes (filtered from `_graph.review_events`).
//! - Backing-claim nodes for any `backed_by` references.
//! - Edges: `Claim → Criterion` (evaluates), `Challenge → target`
//!   (targets), `Challenge -. backed_by .→ Backing`.
//!
//! Mermaid markup can be embedded inside markdown (GitHub, GitLab,
//! MkDocs) or inside an HTML document with a Mermaid script tag.

use serde_json::Value;

/// Render the augmented TrustReport JSON as Mermaid graph source.
pub fn render_mermaid_graph(augmented_json: &Value) -> String {
    let mut out = String::new();
    out.push_str("graph TD\n");

    // Status-styled class definitions.
    out.push_str("    classDef pass fill:#d4edda,stroke:#155724,color:#155724\n");
    out.push_str("    classDef fail fill:#f8d7da,stroke:#721c24,color:#721c24\n");
    out.push_str("    classDef notassessed fill:#e2e3e5,stroke:#383d41,color:#383d41\n");
    out.push_str("    classDef contested fill:#fff3cd,stroke:#856404,color:#856404\n");
    out.push_str("    classDef claim fill:#cce5ff,stroke:#004085,color:#004085\n");
    out.push_str("    classDef challenge fill:#f8d7da,stroke:#721c24,color:#721c24\n");
    out.push_str("    classDef backing fill:#d1ecf1,stroke:#0c5460,color:#0c5460\n");

    let claim_id = augmented_json["claim"].as_str().unwrap_or("?");
    let status = augmented_json["status"].as_str().unwrap_or("current");
    let claim_class = match status {
        "contested" => "contested",
        "superseded" => "notassessed",
        _ => "claim",
    };

    let claim_node = sanitize_id(claim_id);
    out.push_str(&format!(
        "    {}[\"<b>Claim</b><br/>{}<br/>{}\"]:::{}\n",
        claim_node,
        escape_label(claim_id),
        status_label(status),
        claim_class,
    ));

    if let Some(criteria) = augmented_json["criteria"].as_array() {
        for (i, c) in criteria.iter().enumerate() {
            let fallback = format!("criterion_{i}");
            let cid = c["id"].as_str().unwrap_or(&fallback);
            let cnode = sanitize_id(cid);
            let name = c["name"].as_str().unwrap_or("?");
            let result_type = c["result"]["value"]["type"].as_str().unwrap_or("?");
            let crit_status = c["result"]["criterion_status"]
                .as_str()
                .unwrap_or("current");

            let class_name = match (result_type, crit_status) {
                (_, "contested") => "contested",
                (_, "superseded") => "notassessed",
                ("pass", _) => "pass",
                ("fail", _) => "fail",
                _ => "notassessed",
            };

            out.push_str(&format!(
                "    {}[\"<b>Criterion</b><br/>{}<br/>Result: {}\"]:::{}\n",
                cnode,
                escape_label(name),
                result_type,
                class_name,
            ));
            out.push_str(&format!("    {} --> {}\n", claim_node, cnode));
        }
    }

    if let Some(events) = augmented_json
        .pointer("/_graph/review_events")
        .and_then(|v| v.as_array())
    {
        for (i, e) in events.iter().enumerate() {
            if e["kind"]["type"].as_str() != Some("challenge") {
                continue;
            }

            let fallback = format!("challenge_{i}");
            let eid = e["id"].as_str().unwrap_or(&fallback);
            let enode = sanitize_id(eid);
            let category = e["kind"]["data"]["category"]["type"]
                .as_str()
                .unwrap_or("?");

            out.push_str(&format!(
                "    {}[\"<b>Challenge</b><br/>{}<br/>{}\"]:::challenge\n",
                enode,
                escape_label(eid),
                escape_label(category),
            ));

            // Target arrow.
            if let Some(target_data) = e["target"]["data"].as_str() {
                let target_node = sanitize_id(target_data);
                out.push_str(&format!(
                    "    {} -- targets --> {}\n",
                    enode, target_node
                ));
            }

            // Backing arrow.
            if let Some(backed) = e["kind"]["data"]["backed_by"].as_str() {
                let backed_node = sanitize_id(backed);
                out.push_str(&format!(
                    "    {}[\"<b>Backing</b><br/>{}\"]:::backing\n",
                    backed_node,
                    escape_label(backed),
                ));
                out.push_str(&format!(
                    "    {} -.->|backed_by| {}\n",
                    enode, backed_node
                ));
            }
        }
    }

    out
}

fn status_label(s: &str) -> &'static str {
    match s {
        "current" => "Current",
        "contested" => "Contested",
        "superseded" => "Superseded",
        _ => "Unknown",
    }
}

/// Mermaid node ids must be alphanumeric or underscore. Hyphens,
/// dots, and other characters get collapsed to underscores so
/// claim ids like `proteon-sasa-vs-biopython` produce valid Mermaid.
fn sanitize_id(id: &str) -> String {
    let mut out: String = id
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    // Mermaid ids must start with a letter or underscore — prepend one
    // if the first character is a digit.
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, 'n');
    }
    out
}

/// Mermaid labels inside quoted strings support HTML-style entities
/// for special characters and `<br/>` for line breaks.
fn escape_label(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
