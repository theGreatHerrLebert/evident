//! `--format site`: one self-contained HTML page over a whole manifest.
//!
//! The engine already synthesizes a TrustReport per claim and can render
//! each one as an HTML fragment. What a reader of a *corpus* lacks is the
//! cross-claim view: which claims exist, at which tier, validated against
//! which oracle, providing which capability, in what status. This module
//! embeds all of that as JSON in a single page and renders it client-side
//! as a filterable table, a subsystem × tier coverage matrix, and a
//! claim–oracle–capability graph, with the existing per-claim fragment as
//! the drill-down.
//!
//! Deliberately static: no build step, no server, one file. The only
//! external resources are Cytoscape (graph layout) and Mermaid (the
//! per-claim attestation graph inside fragments), both loaded from a CDN
//! and both optional — the table and matrix work without them.
//!
//! Manifest fields the translator does not consume (`subsystem`,
//! `trust_strategy`, `capabilities`, `pattern`, `review_status`,
//! `inputs`) are read directly from the YAML here, so the site can show
//! them without widening the typed surface.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::html_render::{render_html_fragment, CSS as FRAGMENT_CSS};

/// Raw manifest claims (as JSON values) keyed by claim id, in manifest
/// order, with `include:` resolved one level deep relative to the
/// manifest's directory. Returns the project name and the ordered list.
pub fn read_raw_claims(manifest_path: &str) -> Result<(String, Vec<Value>), String> {
    let root = Path::new(manifest_path);
    let text = fs::read_to_string(root).map_err(|e| format!("error reading {manifest_path}: {e}"))?;
    let doc: Value = yaml_to_json(&text).map_err(|e| format!("error parsing {manifest_path}: {e}"))?;
    let project = doc["project"].as_str().unwrap_or("").to_string();
    let mut out: Vec<Value> = Vec::new();
    push_claims(&doc, manifest_path, &mut out);
    if let Some(includes) = doc["include"].as_array() {
        let base: PathBuf = root.parent().map(Path::to_path_buf).unwrap_or_default();
        for inc in includes {
            let Some(rel) = inc.as_str() else { continue };
            let p = base.join(rel);
            let t = fs::read_to_string(&p).map_err(|e| format!("error reading include {}: {e}", p.display()))?;
            let d: Value = yaml_to_json(&t).map_err(|e| format!("error parsing include {}: {e}", p.display()))?;
            push_claims(&d, &p.display().to_string(), &mut out);
        }
    }
    Ok((project, out))
}

fn push_claims(doc: &Value, source_path: &str, out: &mut Vec<Value>) {
    if let Some(claims) = doc["claims"].as_array() {
        for c in claims {
            let mut c = c.clone();
            if let Some(obj) = c.as_object_mut() {
                obj.insert("_source_path".into(), Value::String(source_path.to_string()));
            }
            out.push(c);
        }
    }
}

fn yaml_to_json(text: &str) -> Result<Value, String> {
    let y: serde_yaml_ng::Value = serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?;
    serde_json::to_value(y).map_err(|e| e.to_string())
}

/// Build the site. `reports` are the augmented TrustReport JSON values
/// produced by the CLI (measurement-class claims only); `skipped` are
/// `{id, reason, fatal}` objects; `raw_claims` is every claim in the
/// manifest including policy/reference ones.
/// `only`: the CLI's positional claim-id filter, if any — the page then
/// shows just that claim. `last_verified_overlay`: sidecar entries the
/// CLI already applied before synthesis (claim id → last_verified JSON),
/// so the page shows the same observations the reports were built from.
pub fn render_site(
    manifest_path: &str,
    project: &str,
    synthesized_at: &str,
    raw_claims: &[Value],
    reports: &[Value],
    skipped: &[Value],
    only: Option<&str>,
    last_verified_overlay: &HashMap<String, Value>,
) -> String {
    let mut report_by_id: HashMap<String, &Value> = HashMap::new();
    for r in reports {
        if let Some(id) = r["claim"].as_str() {
            report_by_id.insert(id.to_string(), r);
        }
    }
    let mut skipped_by_id: HashMap<String, &Value> = HashMap::new();
    for s in skipped {
        if let Some(id) = s["id"].as_str() {
            skipped_by_id.insert(id.to_string(), s);
        }
    }

    let mut claims_out: Vec<Value> = Vec::with_capacity(raw_claims.len());
    let mut fragments: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut reports_out: serde_json::Map<String, Value> = serde_json::Map::new();

    for c in raw_claims {
        let id = c["id"].as_str().unwrap_or("").to_string();
        if let Some(f) = only {
            if id != f {
                continue;
            }
        }
        let mut c = c.clone();
        if let Some(lv) = last_verified_overlay.get(&id) {
            if let Some(obj) = c.as_object_mut() {
                obj.insert("last_verified".into(), lv.clone());
            }
        }
        let c = &c;
        let report = report_by_id.get(&id).copied();
        let skip = skipped_by_id.get(&id).copied();

        let mut counts = json!({"pass": 0, "fail": 0, "not_assessed": 0, "partial": 0, "other": 0, "total": 0});
        if let Some(r) = report {
            if let Some(cs) = r["criteria"].as_array() {
                for cr in cs {
                    let t = cr["result"]["value"]["type"].as_str().unwrap_or("other");
                    let key = match t {
                        "pass" => "pass",
                        "fail" => "fail",
                        "not_assessed" => "not_assessed",
                        "partial" => "partial",
                        _ => "other",
                    };
                    counts[key] = json!(counts[key].as_u64().unwrap_or(0) + 1);
                    counts["total"] = json!(counts["total"].as_u64().unwrap_or(0) + 1);
                }
            }
            fragments.insert(id.clone(), Value::String(render_html_fragment(r)));
            reports_out.insert(id.clone(), (*r).clone());
        }

        let status = report
            .and_then(|r| r["status"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| {
                if skip.map(|s| s["fatal"].as_bool().unwrap_or(false)).unwrap_or(false) {
                    "error".into()
                } else {
                    "not_synthesized".into()
                }
            });

        let oracles: Vec<Value> = c["evidence"]["oracle"].as_array().cloned().unwrap_or_default();
        let provenance_kind = match &c["provenance"] {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o.get("kind").and_then(Value::as_str).map(str::to_string),
            _ => None,
        };

        claims_out.push(json!({
            "id": id,
            "title": c["title"],
            "kind": c["kind"].as_str().unwrap_or("measurement"),
            "tier": c["tier"],
            "subsystem": c["subsystem"],
            "trust_strategy": c["trust_strategy"].as_array().cloned().unwrap_or_default(),
            "capabilities": c["capabilities"].as_array().cloned().unwrap_or_default(),
            "oracles": oracles,
            "case": c["case"],
            "pattern": c["pattern"],
            "claim": c["claim"],
            "provenance": provenance_kind,
            "review_status": c["review_status"],
            "inputs": c["inputs"],
            "last_verified": c["last_verified"],
            "n_tolerances": c["tolerances"].as_array().map(Vec::len).unwrap_or(0),
            "n_assumptions": c["assumptions"].as_array().map(Vec::len).unwrap_or(0),
            "n_failure_modes": c["failure_modes"].as_array().map(Vec::len).unwrap_or(0),
            "command": c["evidence"]["command"],
            "source_path": c["_source_path"],
            "status": status,
            "criteria": counts,
            "skip_reason": skip.map(|s| s["reason"].clone()).unwrap_or(Value::Null),
            "n_challenges": report.and_then(|r| r["challenges"].as_array().map(Vec::len)).unwrap_or(0),
        }));
    }

    let data = json!({
        "manifest_path": manifest_path,
        "project": project,
        "synthesized_at": synthesized_at,
        "claims": claims_out,
        "reports": reports_out,
        "fragments": fragments,
        "skipped": skipped,
    });
    // `</script>` inside embedded JSON would terminate the data block.
    let data_json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".into()).replace("</", "<\\/");

    let title = if project.is_empty() { "EVIDENT claims".to_string() } else { format!("EVIDENT — {project}") };

    let mut out = String::with_capacity(data_json.len() + 40_000);
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(&title)));
    out.push_str("<link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;700&family=IBM+Plex+Mono:wght@400;500&display=swap\">\n");
    out.push_str("<style>\n");
    out.push_str(FRAGMENT_CSS);
    out.push_str(SITE_CSS);
    out.push_str("</style>\n");
    out.push_str("<script src=\"https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.30.4/cytoscape.min.js\"></script>\n");
    out.push_str("<script type=\"module\">\n  import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.esm.min.mjs';\n  mermaid.initialize({ startOnLoad: false, theme: 'default' });\n  window.__mermaid = mermaid;\n</script>\n");
    out.push_str("</head>\n<body>\n");
    out.push_str(&format!("<script id=\"evident-data\" type=\"application/json\">{data_json}</script>\n"));
    out.push_str(SITE_BODY);
    out.push_str("<script>\n");
    out.push_str(SITE_JS);
    out.push_str("</script>\n</body>\n</html>\n");
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const SITE_CSS: &str = r##"
/* ---- tokens: light on bare :root, dark via prefers-color-scheme (unless data-theme=light) and data-theme=dark ---- */
:root { --ground:#f2f5f8; --surface:#ffffff; --surface-2:#f7f9fb; --line:#d9e0e7; --ink:#1f2a33; --muted:#6a7885; --accent:#0b4f9c; --accent-ink:#ffffff; --hover:#eef3f8;
        --pass:#2e8b57; --warn:#c99700; --warn-ink:#7a5a00; --fail:#c8323e; --na:#9aa6b2; --backdrop:rgba(20,30,40,0.45);
        --font:"IBM Plex Sans", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; --mono:"IBM Plex Mono", "SF Mono", Menlo, Consolas, monospace; }
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --ground:#12181e; --surface:#1a222b; --surface-2:#202a34; --line:#2c3743; --ink:#e4eaf0; --muted:#94a3b3; --accent:#6fa8e6; --accent-ink:#0f1b2a; --hover:#243040; --pass:#4cbf7a; --warn:#e0b33a; --warn-ink:#e0b33a; --fail:#e5606a; --na:#7c8894; --backdrop:rgba(0,0,0,0.6); } }
:root[data-theme="dark"] { --ground:#12181e; --surface:#1a222b; --surface-2:#202a34; --line:#2c3743; --ink:#e4eaf0; --muted:#94a3b3; --accent:#6fa8e6; --accent-ink:#0f1b2a; --hover:#243040; --pass:#4cbf7a; --warn:#e0b33a; --warn-ink:#e0b33a; --fail:#e5606a; --na:#7c8894; --backdrop:rgba(0,0,0,0.6); }

/* ---- site chrome (overrides the fragment's document-level rules) ---- */
body { max-width: none; margin: 0; padding: 0; background: var(--ground); color: var(--ink); font-family: var(--font); }
#site [hidden] { display: none !important; }
#site .num, #site .crit, #site .id, #site code { font-variant-numeric: tabular-nums; }
#site h1 { border: none; margin: 0; font-size: 1.2rem; }
#site h2 { margin-top: 0; border: none; }
#site header { display: flex; align-items: baseline; gap: 1rem; flex-wrap: wrap; padding: 0.8rem 1.25rem; background: var(--surface); border-bottom: 1px solid var(--line); }
#site header .meta { color: var(--muted); font-size: 0.85rem; }
#site header .meta code { font-size: 0.8rem; background: var(--surface-2); color: var(--ink); }
#site nav.tabs { margin-left: auto; display: flex; gap: 0.25rem; }
#site nav.tabs button { border: 1px solid var(--line); background: var(--surface); color: var(--ink); padding: 0.35rem 0.8rem; border-radius: 4px; cursor: pointer; font: inherit; font-size: 0.9rem; }
#site nav.tabs button[aria-selected="true"] { background: var(--accent); color: var(--accent-ink); border-color: var(--accent); }
#site button:focus-visible, #site input:focus-visible, #site a:focus-visible, #site tr.row:focus-visible, #site td.cell:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
#site .layout { display: grid; grid-template-columns: 260px minmax(0, 1fr); align-items: start; }
#site aside { padding: 1rem; background: var(--surface); border-right: 1px solid var(--line); font-size: 0.88rem; overflow-y: auto; max-height: 100vh; position: sticky; top: 0; align-self: start; }
#site aside h3 { margin: 1rem 0 0.15rem; font-size: 0.76rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--muted); }
#site aside h3:first-child { margin-top: 0; }
#site aside .help { color: var(--muted); font-size: 0.78rem; line-height: 1.35; margin: 0 0 0.35rem; }
#site aside.terse .help { display: none; }
#site aside label { display: flex; align-items: center; gap: 0.4rem; margin: 0.15rem 0; cursor: pointer; }
#site aside label .n { margin-left: auto; color: var(--muted); font-size: 0.8rem; }
#site aside input[type="search"] { width: 100%; padding: 0.4rem 0.5rem; border: 1px solid var(--line); border-radius: 4px; font: inherit; background: var(--surface); color: var(--ink); }
#site aside .aside-tools { display: flex; gap: 0.4rem; margin-top: 0.9rem; flex-wrap: wrap; }
#site aside .aside-tools button { font: inherit; font-size: 0.8rem; background: none; color: var(--ink); border: 1px solid var(--line); border-radius: 4px; padding: 0.25rem 0.6rem; cursor: pointer; }
#site main { padding: 1rem 1.25rem; min-width: 0; }
#site .hint { background: var(--surface); border: 1px solid var(--line); border-left: 4px solid var(--accent); border-radius: 6px; padding: 0.7rem 1rem; margin: 0 0 1rem; font-size: 0.9rem; display: flex; gap: 1rem; align-items: center; }
#site .hint button { margin-left: auto; font: inherit; font-size: 0.82rem; background: none; color: var(--ink); border: 1px solid var(--line); border-radius: 4px; padding: 0.2rem 0.55rem; cursor: pointer; white-space: nowrap; }
#site .hint a { color: var(--accent); cursor: pointer; text-decoration: underline; }
#site .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 0.6rem; margin-bottom: 1rem; }
#site .summary .tile { background: var(--surface); border: 1px solid var(--line); border-radius: 6px; padding: 0.55rem 0.8rem; }
#site .summary .tile .num { font-size: 1.35rem; font-weight: 700; line-height: 1.1; }
#site .summary .tile .lbl { font-weight: 600; font-size: 0.82rem; margin-top: 0.1rem; }
#site .summary .tile .sub { color: var(--muted); font-size: 0.75rem; line-height: 1.3; margin-top: 0.15rem; }
#site .summary .tile.good .num { color: var(--pass); } #site .summary .tile.bad .num { color: var(--fail); } #site .summary .tile.warn .num { color: var(--warn-ink); } #site .summary .tile.na .num { color: var(--muted); }
#site table.claims { width: 100%; border-collapse: collapse; background: var(--surface); font-size: 0.86rem; }
#site table.claims th, #site table.claims td { padding: 0.45rem 0.6rem; border-bottom: 1px solid var(--line); text-align: left; vertical-align: top; }
#site table.claims th { background: var(--surface-2); cursor: pointer; user-select: none; white-space: nowrap; }
#site table.claims th.sorted::after { content: " ▾"; color: var(--muted); }
#site table.claims th.sorted.asc::after { content: " ▴"; }
#site table.claims tr.row { cursor: pointer; }
#site table.claims tr.row:hover { background: var(--hover); }
#site table.claims td.title { max-width: 34rem; }
#site table.claims td.title .id { display: block; color: var(--muted); font-family: var(--mono); font-size: 0.74rem; }
#site .pill { display: inline-block; padding: 0.05rem 0.5rem; border-radius: 10px; font-size: 0.75rem; font-weight: 600; border: 1px solid transparent; white-space: nowrap; }
#site .pill.tier-ci { background: color-mix(in srgb, var(--accent) 16%, var(--surface)); color: var(--accent); }
#site .pill.tier-release { background: color-mix(in srgb, var(--pass) 16%, var(--surface)); color: var(--pass); }
#site .pill.tier-research { background: color-mix(in srgb, #7c4dbd 18%, var(--surface)); color: #7c4dbd; }
#site .pill.kind { background: var(--surface-2); color: var(--muted); }
#site .pill.status-current { background: color-mix(in srgb, var(--pass) 18%, var(--surface)); color: var(--pass); }
#site .pill.status-contested { background: color-mix(in srgb, var(--warn) 22%, var(--surface)); color: var(--warn-ink); }
#site .pill.status-superseded { background: var(--surface-2); color: var(--muted); }
#site .pill.status-error { background: color-mix(in srgb, var(--fail) 18%, var(--surface)); color: var(--fail); }
#site .pill.status-not_synthesized { background: var(--surface-2); color: var(--muted); }
#site .crit { font-family: var(--mono); font-size: 0.78rem; white-space: nowrap; }
#site .crit .p { color: var(--pass); } #site .crit .f { color: var(--fail); } #site .crit .n { color: var(--muted); }
#site .bar { display: inline-block; height: 8px; width: 90px; background: var(--line); border-radius: 4px; overflow: hidden; vertical-align: middle; margin-right: 0.4rem; }
#site .bar i { display: block; height: 100%; float: left; }
#site .bar i.p { background: var(--pass); } #site .bar i.f { background: var(--fail); } #site .bar i.n { background: var(--na); }
#site .tags { display: flex; flex-wrap: wrap; gap: 0.2rem; }
#site .tags span { background: var(--surface-2); border: 1px solid var(--line); border-radius: 3px; padding: 0 0.35rem; font-size: 0.74rem; color: var(--ink); white-space: nowrap; }
#site .empty { color: var(--muted); padding: 2rem; text-align: center; }
/* ? help buttons + popover */
#site button.q { display: inline-grid; place-items: center; width: 1.1rem; height: 1.1rem; margin-left: 0.3rem; padding: 0; border-radius: 50%; border: 1px solid var(--muted); background: var(--surface); color: var(--muted); font: inherit; font-size: 0.7rem; font-weight: 700; line-height: 1; cursor: pointer; vertical-align: 1px; }
#site button.q:hover, #site button.q[aria-expanded="true"] { border-color: var(--accent); color: var(--accent); }
#site { position: relative; }
#site #pop { position: absolute; z-index: 30; max-width: 26rem; background: var(--surface); color: var(--ink); border: 1px solid var(--line); border-radius: 6px; box-shadow: 0 8px 28px rgba(0,0,0,0.18); padding: 0.75rem 0.9rem; font-size: 0.85rem; line-height: 1.45; }
#site #pop h4 { margin: 0 0 0.3rem; font-size: 0.92rem; }
#site #pop p { margin: 0 0 0.5rem; }
#site #pop dl { display: grid; grid-template-columns: max-content 1fr; gap: 0.3rem 0.7rem; margin: 0.4rem 0 0; }
#site #pop dt { font-weight: 600; white-space: nowrap; } #site #pop dd { margin: 0; }
#site #pop dt .pill { font-size: 0.72rem; }
#site #pop .pop-x { position: absolute; top: 0.3rem; right: 0.4rem; border: none; background: none; color: var(--muted); font: inherit; cursor: pointer; }
#site #pop a { color: var(--accent); }
#site #drawer button.q { border-color: #6c757d; color: #6c757d; background: #fff; }
#site #drawer #pop { color: #2c3e50; }
#site .summary .tile .lbl button.q { vertical-align: 0; }
/* full-screen attestation graph */
#site #drawer .graph-tools { display: flex; gap: 0.5rem; align-items: center; margin: 0.4rem 0 -0.4rem; }
#site #drawer .graph-tools button, #site #big .tools button { font: inherit; font-size: 0.82rem; border: 1px solid #cfd6dd; background: #ffffff; color: #2c3e50; border-radius: 4px; padding: 0.25rem 0.6rem; cursor: pointer; }
#site #drawer .graph-tools .hintt { color: #6c757d; font-size: 0.8rem; }
#site #big { position: fixed; inset: 0; z-index: 40; background: #ffffff; display: flex; flex-direction: column; }
#site #big .tools { display: flex; gap: 0.5rem; align-items: center; padding: 0.5rem 1rem; border-bottom: 1px solid #dde3e8; background: #f8f9fa; color: #2c3e50; font-size: 0.9rem; }
#site #big .tools .sp { flex: 1; color: #6c757d; font-size: 0.82rem; }
#site #big .stage { flex: 1; overflow: hidden; position: relative; cursor: grab; }
#site #big .stage.dragging { cursor: grabbing; }
#site #big .stage svg { position: absolute; left: 0; top: 0; transform-origin: 0 0; max-width: none !important; height: auto !important; }
/* about */
#site .about { max-width: 56rem; margin: 0.5rem auto 3rem; font-size: 0.97rem; line-height: 1.6; }
#site .layout.no-rail { grid-template-columns: minmax(0, 1fr); }
#site .about .lead { font-size: 1.05rem; }
#site .about h2 { font-size: 1.05rem; margin: 1.6rem 0 0.4rem; padding-bottom: 0.2rem; border-bottom: 1px solid var(--line); }
#site .about ol.journey { counter-reset: step; list-style: none; padding: 0; display: grid; gap: 0.6rem; }
#site .about ol.journey li { background: var(--surface); border: 1px solid var(--line); border-radius: 6px; padding: 0.7rem 0.9rem 0.7rem 3rem; position: relative; }
#site .about ol.journey li::before { counter-increment: step; content: counter(step); position: absolute; left: 0.8rem; top: 0.65rem; width: 1.6rem; height: 1.6rem; border-radius: 50%; background: var(--accent); color: var(--accent-ink); font-weight: 700; display: grid; place-items: center; font-size: 0.85rem; }
#site .about ol.journey li a { color: var(--accent); cursor: pointer; text-decoration: underline; }
#site .about dl.gloss { display: grid; grid-template-columns: max-content 1fr; gap: 0.35rem 1rem; }
#site .about dl.gloss dt { font-weight: 600; white-space: nowrap; }
#site .about dl.gloss dd { margin: 0; color: var(--ink); }
#site .about dl.gloss dd .ex { color: var(--muted); }
#site .about .card { background: var(--surface); border: 1px solid var(--line); border-radius: 6px; padding: 0.8rem 1rem; margin: 0.6rem 0; }
#site .about code { background: var(--surface-2); color: var(--ink); }
/* matrix */
#site table.matrix { border-collapse: collapse; background: var(--surface); font-size: 0.86rem; }
#site table.matrix th, #site table.matrix td { border: 1px solid var(--line); padding: 0.4rem 0.7rem; text-align: center; }
#site table.matrix th { background: var(--surface-2); }
#site table.matrix th.rowh { text-align: left; font-weight: 600; }
#site table.matrix td.cell { cursor: pointer; }
#site table.matrix td.cell:hover { outline: 2px solid var(--accent); }
#site table.matrix td.c0 { color: var(--na); }
#site table.matrix td.cell small { display: block; color: var(--muted); font-size: 0.72rem; }
#site .note { color: var(--muted); font-size: 0.85rem; margin: 0.5rem 0 1rem; max-width: 60rem; }
/* graph */
#site #graph { height: 72vh; min-height: 480px; background: #ffffff; border: 1px solid var(--line); border-radius: 6px; }
#site .legend { display: flex; gap: 0.9rem; flex-wrap: wrap; font-size: 0.8rem; color: var(--ink); margin: 0.5rem 0; align-items: center; }
#site .legend span[title] { cursor: help; border-bottom: 1px dotted var(--muted); }
#site .legend i { display: inline-block; width: 12px; height: 12px; border-radius: 50%; margin-right: 0.3rem; vertical-align: -1px; }
#site .legend i.sq { border-radius: 2px; } #site .legend i.dia { transform: rotate(45deg); border-radius: 1px; width: 10px; height: 10px; }
#site .legend label { display: inline-flex; gap: 0.3rem; align-items: center; margin-left: auto; color: var(--ink); }
#site .legend label + label { margin-left: 0; }
/* detail drawer — a light card in both themes, because the per-claim fragment CSS is light-only */
#site #drawer { position: fixed; top: 0; right: 0; height: 100vh; width: min(780px, 94vw); background: #ffffff; color: #2c3e50; border-left: 1px solid #dde3e8; box-shadow: -12px 0 32px rgba(0,0,0,0.18); z-index: 20; transform: translateX(100%); transition: transform 0.18s ease; display: flex; flex-direction: column; }
#site #drawer.open { transform: none; }
@media (prefers-reduced-motion: reduce) { #site #drawer { transition: none; } }
#site #drawer .bar-top { display: flex; align-items: center; gap: 0.75rem; padding: 0.6rem 1rem; border-bottom: 1px solid #dde3e8; background: #f8f9fa; }
#site #drawer .bar-top code { background: #eef1f4; color: #2c3e50; }
#site #drawer .bar-top button { margin-left: auto; font: inherit; border: 1px solid #cfd6dd; background: #ffffff; color: #2c3e50; border-radius: 4px; padding: 0.25rem 0.6rem; cursor: pointer; }
#site #drawer .body { overflow-y: auto; padding: 0 1.25rem 2rem; font-size: 0.92rem; background: #ffffff; position: relative; }
#site #drawer .body h1 { font-size: 1.15rem; margin: 1rem 0 0.4rem; border-bottom: 2px solid #2c3e50; padding-bottom: 0.3rem; color: #2c3e50; }
#site #drawer .body h2 { margin-top: 1.6rem; border-bottom: 1px solid #ccc; color: #2c3e50; }
#site #drawer .body .pill.kind { background: #f1f3f5; color: #495057; }
#site #drawer .body .pill.tier-ci { background: #e7f1ff; color: #0b4f9c; } #site #drawer .body .pill.tier-release { background: #e6f4ea; color: #146c2e; } #site #drawer .body .pill.tier-research { background: #f3e8ff; color: #5b2a86; }
#site #drawer .body .pill.status-current { background: #d4edda; color: #155724; } #site #drawer .body .pill.status-contested { background: #fff3cd; color: #856404; } #site #drawer .body .pill.status-superseded, #site #drawer .body .pill.status-not_synthesized { background: #e2e3e5; color: #383d41; } #site #drawer .body .pill.status-error { background: #f8d7da; color: #721c24; }
#site #drawer .lead-in { color: #6c757d; font-size: 0.85rem; margin: 0 0 0.4rem; }
#site #drawer .claimtext { background: #f8f9fa; border-left: 4px solid #2c3e50; padding: 0.6rem 0.9rem; margin: 0.6rem 0; white-space: pre-wrap; }
#site #drawer dl.kv { display: grid; grid-template-columns: max-content 1fr; gap: 0.25rem 0.9rem; margin: 0.6rem 0; }
#site #drawer dl.kv dt { color: #6c757d; } #site #drawer dl.kv dd { margin: 0; }
#site #drawer .rel a { cursor: pointer; color: #0b4f9c; text-decoration: underline; }
#site #drawer .section-help { background: #eef3f8; border-radius: 4px; padding: 0.5rem 0.8rem; font-size: 0.85rem; color: #2c3e50; margin: 0.8rem 0; }
#site #backdrop { position: fixed; inset: 0; background: var(--backdrop); z-index: 15; display: none; }
#site #backdrop.open { display: block; }
@media (max-width: 900px) { #site .layout { grid-template-columns: 1fr; } #site aside { position: static; max-height: none; border-right: none; border-bottom: 1px solid var(--line); } }
"##;

const SITE_BODY: &str = r##"
<div id="site">
  <header>
    <h1 id="site-title">EVIDENT</h1>
    <span class="meta">manifest <code id="manifest-path"></code></span>
    <nav class="tabs" role="tablist">
      <button role="tab" data-view="about" aria-selected="false">Start here</button>
      <button role="tab" data-view="table" aria-selected="true">Claims</button>
      <button role="tab" data-view="matrix" aria-selected="false">Coverage</button>
      <button role="tab" data-view="graph" aria-selected="false">Graph</button>
    </nav>
  </header>
  <div class="layout">
    <aside id="facets"></aside>
    <main>
      <div id="hint"></div>
      <div class="summary" id="summary"></div>
      <section id="view-about" hidden></section>
      <section id="view-table"></section>
      <section id="view-matrix" hidden></section>
      <section id="view-graph" hidden>
        <p class="note" id="graph-note"></p>
        <div class="legend" id="legend"></div>
        <div id="graph"></div>
      </section>
    </main>
  </div>
  <div id="pop" role="dialog" aria-modal="false" hidden></div>
  <div id="big" role="dialog" aria-modal="true" aria-label="Attestation graph, full screen" hidden></div>
  <div id="backdrop"></div>
  <div id="drawer" aria-hidden="true">
    <div class="bar-top"><code id="drawer-title"></code><button id="drawer-close">Close ✕</button></div>
    <div class="body" id="drawer-body"></div>
  </div>
</div>
"##;

const SITE_JS: &str = r##"
(function () {
  const DATA = JSON.parse(document.getElementById('evident-data').textContent);
  const CLAIMS = DATA.claims;
  const byId = Object.fromEntries(CLAIMS.map(c => [c.id, c]));
  const $ = (s, el) => (el || document).querySelector(s);
  const esc = s => String(s == null ? '' : s).replace(/[&<>"]/g, ch => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[ch]));
  const TIERS = ['ci', 'release', 'research'];
  const PROJECT = DATA.project || 'this project';

  // ---------- glossary: one source of truth for every explanation on the page ----------
  const G = {
    claim: `A claim is one specific, checkable statement about ${PROJECT} — for example "the SASA value on this structure agrees with Biopython within 1%". EVIDENT does not ask whether the code looks right; it asks what is claimed, what evidence supports it, and what would falsify it.`,
    kind: {
      _: 'What sort of statement the claim is. Most are measurements; a few are rules or placeholders.',
      measurement: 'A numeric result is compared against an independent reference (the oracle) and must land inside a stated tolerance. This is the kind that gets a pass/fail trust report.',
      policy: 'A rule about how evidence should be gathered (e.g. "release claims must pin their corpus"). It is a prescription, not a fact, so it has no trust report.',
      reference: 'A documented gap: something the project knows it should check but has no asserting test for yet. Listed so the absence is visible, not hidden.',
      implementation: 'A component behaves according to a specification.',
      pipeline: 'A workflow turns inputs into outputs reproducibly.',
      scientific: 'The outputs support an interpretation under stated assumptions.',
      performance: 'A speed or resource claim.',
      release_gate: 'A claim that must hold before a release ships.',
      metadata_compatibility: 'A declared compatibility (versions, formats) checked deterministically from project metadata.',
      behavioral_concordance: 'A result agrees with a previously published value.',
      third_party_observation: 'A value observed in an external source (a paper), recorded without re-measuring it here.',
    },
    tier: {
      _: 'How heavy the evidence is and how often it runs. A tier says nothing about how important the claim is — only how expensive checking it is.',
      ci: 'Cheap enough to run on every change. Small fixtures, no GPUs, no large datasets.',
      release: 'Heavy checks run before a release: large corpora, pinned versions, hashed inputs, a persisted artifact someone else can replay.',
      research: 'Exploratory or in-flight. The only tier where a tolerance may still be prose instead of a number.',
    },
    status: {
      _: 'What the trust engine concluded after looking at the evidence and any review events. It is computed, never typed in by hand.',
      current: 'No open objection. The evidence stands as recorded.',
      contested: 'Someone filed a challenge that is backed by evidence (or a procedural problem such as a missing artifact), and it has not been resolved.',
      superseded: 'A newer claim or attestation replaces this one.',
      not_synthesized: 'Policy and reference claims are not measurements, so there is nothing to compute a trust report from.',
      error: 'The claim could not be translated — a manifest problem to fix, not a scientific finding.',
    },
    strategy: {
      _: 'Why the project believes the claim. Three complementary routes to trust; the less we understand a component, the stronger the validation must be.',
      understanding: 'Someone can explain why the code should work (they read it, derived it, or reimplemented it).',
      validation: 'The code was shown to behave correctly against a reference — the workhorse for AI-assisted or ported code.',
      proof: 'A property is guaranteed under stated assumptions (an invariant, a bound, a proof).',
    },
    subsystem: `The part of ${PROJECT} the claim is about, in the project's own vocabulary (e.g. an I/O layer, a force field, an alignment routine). Useful for asking "how well is this area covered?".`,
    capability: 'The property a downstream user can rely on if the claim holds — phrased as a parity or a guarantee (e.g. "alignment TM-score parity"). Several claims can back the same capability at different tiers.',
    oracle: 'The independent reference the output is compared against: an established tool, an analytic solution, or simulated ground truth. An oracle can itself be wrong; the manifest records which one was used so that can be argued about.',
    criteria: 'Each tolerance on a claim becomes one criterion. ✓ the observed value met the tolerance · ✗ it did not · ? no observed value was available in this render — the check exists but this page was built without its replay results.',
    verified: 'When the claim’s command was last re-run and the observed value recorded. Empty means the observation is not in the manifest; the project may keep it in a release bundle instead.',
    provenance: 'Who or what authored the claim (a person, an automated extraction from a paper or repo) and whether it has been reviewed.',
  };

  // ---------- "?" help: one popover, many triggers ----------
  const STATUS_LABEL = { current: 'Current', contested: 'Contested', superseded: 'Superseded', error: 'Error', not_synthesized: 'Not synthesized' };
  const present = getter => [...new Set(CLAIMS.flatMap(getter))];
  const HELP = {
    claim:      () => ({ title: 'What is a claim?', body: G.claim, more: 'about' }),
    kind:       () => ({ title: 'Kind', body: G.kind._, items: present(c => [c.kind]).map(k => [k, G.kind[k], 'pill kind']) }),
    tier:       () => ({ title: 'Tier', body: G.tier._, items: TIERS.map(t => [t, G.tier[t], 'pill tier-' + t]) }),
    status:     () => ({ title: 'Status', body: G.status._, items: ['current', 'contested', 'superseded', 'not_synthesized', 'error'].map(k => [STATUS_LABEL[k], G.status[k], 'pill status-' + k]) }),
    strategy:   () => ({ title: 'Trust strategy', body: G.strategy._, items: ['understanding', 'validation', 'proof'].map(k => [k, G.strategy[k]]) }),
    subsystem:  () => ({ title: 'Subsystem', body: G.subsystem }),
    oracle:     () => ({ title: 'Oracle', body: G.oracle }),
    capability: () => ({ title: 'Capability', body: G.capability }),
    criteria:   () => ({ title: 'Checks (criteria)', body: G.criteria, items: [['✓ passed', 'The observed value met its tolerance.'], ['✗ failed', 'The observed value was outside its tolerance.'], ['? not assessed', 'The check is defined, but no observed value was available when this page was built.']] }),
    verified:   () => ({ title: 'Last verified', body: G.verified }),
    provenance: () => ({ title: 'Provenance', body: G.provenance }),
    graph:      () => ({ title: 'Reading the graph', body: 'Circles are claims, coloured by status, with a thick border at release tier. Diamonds are oracles; small rectangles are capabilities (dashed links) and subsystems (dotted links). Red arrows are challenges backed by another claim.', items: [['claim', G.claim], ['oracle', G.oracle], ['capability', G.capability], ['subsystem', G.subsystem]] }),
  };
  const qbtn = (topic, label) => `<button class="q" type="button" data-help="${topic}" aria-haspopup="dialog" aria-expanded="false" aria-label="What is ${esc(label || topic)}?" title="What is ${esc(label || topic)}?">?</button>`;
  const pop = document.getElementById('pop');
  let popTrigger = null;
  function closePop() {
    if (pop.hidden) return;
    // Only hand focus back to the trigger when focus is still inside the popover
    // (Esc, its ✕). If the user clicked something else — a row that opened the
    // drawer, say — that element owns focus now and must keep it.
    const restore = popTrigger && (pop.contains(document.activeElement) || document.activeElement === document.body);
    pop.hidden = true; pop.innerHTML = '';
    if (popTrigger) { popTrigger.setAttribute('aria-expanded', 'false'); if (restore) popTrigger.focus(); }
    popTrigger = null;
  }
  function openPop(btn) {
    const spec = HELP[btn.dataset.help]; if (!spec) return;
    const h = spec();
    if (popTrigger === btn) { closePop(); return; }
    closePop();
    pop.innerHTML = `<button class="pop-x" type="button" aria-label="Close">✕</button><h4>${esc(h.title)}</h4><p>${esc(h.body)}</p>` +
      (h.items ? `<dl>${h.items.map(([k, v, cls]) => `<dt>${cls ? `<span class="${cls}">${esc(k)}</span>` : esc(k)}</dt><dd>${esc(v)}</dd>`).join('')}</dl>` : '') +
      (h.more ? `<p style="margin-top:0.5rem"><a href="#" data-go="${h.more}">Read the full introduction →</a></p>` : '');
    // Mount inside the drawer when the trigger lives there, so it scrolls and stacks with it.
    const site = document.getElementById('site');
    const host = btn.closest('#drawer .body') || site;
    if (pop.parentElement !== host) host.appendChild(pop);
    pop.hidden = false; popTrigger = btn; btn.setAttribute('aria-expanded', 'true');
    const r = btn.getBoundingClientRect(); const hostR = host === site ? { left: 0, top: 0 } : host.getBoundingClientRect();
    const scrollX = host === site ? window.scrollX : host.scrollLeft, scrollY = host === site ? window.scrollY : host.scrollTop;
    const w = pop.offsetWidth, vw = host === site ? window.innerWidth : host.clientWidth;
    let left = r.left - hostR.left + scrollX; if (left + w > scrollX + vw - 12) left = Math.max(8, scrollX + vw - w - 12);
    pop.style.left = left + 'px'; pop.style.top = (r.bottom - hostR.top + scrollY + 6) + 'px';
    pop.querySelector('.pop-x').addEventListener('click', closePop);
    const go = pop.querySelector('a[data-go]'); if (go) go.addEventListener('click', e => { e.preventDefault(); closePop(); closeDetail(); setView(go.dataset.go); update(); });
    pop.querySelector('.pop-x').focus();
  }
  document.addEventListener('click', e => {
    const b = e.target.closest('button.q');
    if (b) { e.stopPropagation(); openPop(b); return; }
    if (!pop.hidden && !e.target.closest('#pop')) closePop();
  });
  document.addEventListener('keydown', e => { if (e.key === 'Escape' && !pop.hidden) { e.stopImmediatePropagation(); closePop(); } }, true);

  document.title = DATA.project ? `EVIDENT — ${DATA.project}` : 'EVIDENT claims';
  $('#site-title').textContent = DATA.project ? `EVIDENT · ${DATA.project}` : 'EVIDENT';
  $('#manifest-path').textContent = DATA.manifest_path;

  // ---------- state ----------
  const FACETS = [
    { key: 'kind', label: 'Kind', topic: 'kind', help: G.kind._, get: c => [c.kind], tip: v => G.kind[v] },
    { key: 'tier', label: 'Tier', topic: 'tier', help: G.tier._, get: c => [c.tier], order: TIERS, tip: v => G.tier[v] },
    { key: 'status', label: 'Status', topic: 'status', help: G.status._, get: c => [c.status], order: ['current', 'contested', 'superseded', 'error', 'not_synthesized'], fmt: v => STATUS_LABEL[v] || v, tip: v => G.status[v] },
    { key: 'trust_strategy', label: 'Trust strategy', topic: 'strategy', help: G.strategy._, get: c => c.trust_strategy || [], tip: v => G.strategy[v] },
    { key: 'subsystem', label: 'Subsystem', topic: 'subsystem', help: G.subsystem, get: c => c.subsystem ? [c.subsystem] : [] },
    { key: 'oracles', label: 'Oracle', topic: 'oracle', help: G.oracle, get: c => c.oracles || [] },
    { key: 'capabilities', label: 'Capability', topic: 'capability', help: G.capability, get: c => c.capabilities || [] },
  ];
  const state = { q: '', sel: Object.fromEntries(FACETS.map(f => [f.key, new Set()])), sort: { key: 'tier', asc: true }, view: 'table', terse: false, seen: false, hintDismissed: false };
  try {
    const saved = JSON.parse(localStorage.getItem('evident-site-state') || 'null');
    if (saved) { if (saved.sel) for (const k in saved.sel) if (state.sel[k]) state.sel[k] = new Set(saved.sel[k]); state.q = saved.q || ''; state.sort = saved.sort || state.sort; state.terse = !!saved.terse; state.seen = !!saved.seen; state.hintDismissed = !!saved.hintDismissed; }
  } catch (e) {}
  function persist() { try { localStorage.setItem('evident-site-state', JSON.stringify({ q: state.q, sort: state.sort, terse: state.terse, seen: state.seen, hintDismissed: state.hintDismissed, sel: Object.fromEntries(Object.entries(state.sel).map(([k, v]) => [k, [...v]])) })); } catch (e) {} }

  function matches(c) {
    for (const f of FACETS) { const sel = state.sel[f.key]; if (sel.size && !f.get(c).some(v => sel.has(v))) return false; }
    if (state.q) { const q = state.q.toLowerCase(); const blob = [c.id, c.title, c.claim, c.subsystem, ...(c.oracles || []), ...(c.capabilities || [])].join(' ').toLowerCase(); if (!blob.includes(q)) return false; }
    return true;
  }
  function filtered() { return CLAIMS.filter(matches); }
  function anyFilter() { return !!state.q || Object.values(state.sel).some(s => s.size); }

  // ---------- facets ----------
  function renderFacets() {
    const vis = filtered();
    let html = `<h3>Search</h3><input type="search" id="q" placeholder="id, title, oracle…" value="${esc(state.q)}" aria-label="Search claims">`;
    for (const f of FACETS) {
      const counts = new Map();
      for (const c of CLAIMS) for (const v of f.get(c)) counts.set(v, (counts.get(v) || 0) + 0);
      for (const c of vis) for (const v of f.get(c)) counts.set(v, (counts.get(v) || 0) + 1);
      let keys = [...counts.keys()];
      keys.sort((a, b) => f.order ? (f.order.indexOf(a) - f.order.indexOf(b)) : String(a).localeCompare(String(b)));
      if (!keys.length) continue;
      html += `<h3>${f.label}${qbtn(f.topic, f.label)}</h3><p class="help">${esc(f.help)}</p>`;
      for (const k of keys) {
        const on = state.sel[f.key].has(k); const tip = f.tip ? f.tip(k) : '';
        html += `<label ${tip ? `title="${esc(tip)}"` : ''}><input type="checkbox" data-facet="${esc(f.key)}" data-val="${esc(k)}" ${on ? 'checked' : ''}> <span>${esc(f.fmt ? f.fmt(k) : k)}</span><span class="n">${counts.get(k)}</span></label>`;
      }
    }
    html += `<div class="aside-tools"><button id="clear" ${anyFilter() ? '' : 'disabled'}>Clear filters</button><button id="terse">${state.terse ? 'Show explanations' : 'Hide explanations'}</button></div>`;
    const aside = $('#facets'); aside.innerHTML = html; aside.classList.toggle('terse', state.terse);
    $('#q').addEventListener('input', e => { state.q = e.target.value; update(); });
    aside.querySelectorAll('input[type=checkbox]').forEach(cb => cb.addEventListener('change', e => { const s = state.sel[e.target.dataset.facet]; e.target.checked ? s.add(e.target.dataset.val) : s.delete(e.target.dataset.val); update(); }));
    $('#clear').addEventListener('click', () => { state.q = ''; for (const k in state.sel) state.sel[k].clear(); update(); });
    $('#terse').addEventListener('click', () => { state.terse = !state.terse; update(); });
  }

  // ---------- hint + summary ----------
  function renderHint() {
    const el = $('#hint');
    if (state.view === 'about' || state.hintDismissed) { el.innerHTML = ''; return; }
    el.innerHTML = `<div class="hint"><span>New to EVIDENT? <a href="#" data-go="about">Start here</a> explains what a claim is, what the tiers and statuses mean, and a 4-step way to read this page.</span><button id="hint-x">Got it</button></div>`;
    $('#hint a').addEventListener('click', e => { e.preventDefault(); setView('about'); update(); });
    $('#hint-x').addEventListener('click', () => { state.hintDismissed = true; update(); });
  }
  function renderSummary(vis) {
    const n = vis.length, all = CLAIMS.length;
    const st = k => vis.filter(c => c.status === k).length;
    const crit = vis.reduce((a, c) => { a.pass += c.criteria.pass; a.fail += c.criteria.fail; a.na += c.criteria.not_assessed; a.total += c.criteria.total; return a; }, { pass: 0, fail: 0, na: 0, total: 0 });
    const scope = n === all ? `in ${PROJECT}` : `matching the current filter (of ${all})`;
    const tiles = [
      { v: n, l: n === 1 ? 'claim' : 'claims', s: `checkable statements ${scope}`, q: 'claim' },
      { v: st('current'), l: 'current', s: 'no open objection', q: 'status', cls: 'good' },
      { v: st('contested'), l: 'contested', s: 'challenged, unresolved', q: 'status', cls: st('contested') ? 'warn' : '' },
      { v: st('superseded'), l: 'superseded', s: 'replaced by a newer claim', q: 'status' },
      { v: `${crit.pass} / ${crit.total}`, l: 'checks passed', s: 'observed value met its tolerance', q: 'criteria', cls: crit.pass ? 'good' : '' },
      { v: crit.fail, l: 'checks failed', s: 'observed value outside tolerance', q: 'criteria', cls: crit.fail ? 'bad' : '' },
      { v: crit.na, l: 'not assessed', s: 'check exists, no observation in this render', q: 'criteria', cls: 'na' },
      { v: vis.filter(c => c.tier === 'release').length, l: 'release-tier', s: 'heavy, pinned, replayable evidence', q: 'tier' },
    ];
    $('#summary').innerHTML = tiles.map(t => `<div class="tile ${t.cls || ''}"><div class="num">${t.v}</div><div class="lbl">${t.l}${qbtn(t.q, t.l)}</div><div class="sub">${esc(t.s)}</div></div>`).join('');
  }

  // ---------- about ----------
  function renderAbout() {
    const kinds = [...new Set(CLAIMS.map(c => c.kind))];
    const strategies = [...new Set(CLAIMS.flatMap(c => c.trust_strategy || []))];
    const tiers = TIERS.filter(t => CLAIMS.some(c => c.tier === t));
    const statuses = [...new Set(CLAIMS.map(c => c.status))];
    const nOr = new Set(CLAIMS.flatMap(c => c.oracles || [])).size, nCap = new Set(CLAIMS.flatMap(c => c.capabilities || [])).size, nSub = new Set(CLAIMS.map(c => c.subsystem).filter(Boolean)).size;
    const gl = (obj, keys) => `<dl class="gloss">${keys.map(k => `<dt>${esc(STATUS_LABEL[k] || k)}</dt><dd>${esc(obj[k] || '')}</dd>`).join('')}</dl>`;
    $('#view-about').innerHTML = `<div class="about">
      <p class="lead">${esc(G.claim)}</p>
      <p>This page is the current evidence record for <strong>${esc(PROJECT)}</strong>: ${CLAIMS.length} claims, checked against ${nOr} independent references, covering ${nSub} subsystems and ${nCap} capabilities. It was generated by the EVIDENT trust engine (<code>typed-trust</code>) from the project's claim manifest — nothing on it is written by hand, and no model was involved in computing any status.</p>
      <h2>How to read it in four steps</h2>
      <ol class="journey">
        <li><strong>See where evidence is thick and where it is thin.</strong> Open <a href="#" data-go="matrix">Coverage</a>: rows are subsystems, columns are tiers. An empty <em>release</em> cell means that part of the code has no release-grade evidence yet — that is a finding, not a bug in the page.</li>
        <li><strong>Narrow to what you care about.</strong> In <a href="#" data-go="table">Claims</a>, tick a subsystem, an oracle, or a tier in the left rail. Counts update as you go; every explanation there can be hidden once you know the vocabulary.</li>
        <li><strong>Open one claim and read it top to bottom.</strong> The statement says what is claimed; the tolerances say exactly how it is checked and against which oracle; the criteria say what happened; assumptions and failure modes say what would make the claim wrong.</li>
        <li><strong>Follow the connections.</strong> In the <a href="#" data-go="graph">Graph</a>, a claim is linked to the oracles it is checked against and the capabilities it backs. Click an oracle to see everything that depends on it.</li>
      </ol>
      <h2>The vocabulary</h2>
      <div class="card"><strong>Kind</strong> — ${esc(G.kind._)}${gl(G.kind, kinds)}</div>
      <div class="card"><strong>Tier</strong> — ${esc(G.tier._)}${gl(G.tier, tiers)}</div>
      <div class="card"><strong>Trust strategy</strong> — ${esc(G.strategy._)}${gl(G.strategy, strategies)}</div>
      <div class="card"><strong>Status</strong> — ${esc(G.status._)}${gl(G.status, statuses)}</div>
      <div class="card"><strong>Subsystem</strong> — ${esc(G.subsystem)}</div>
      <div class="card"><strong>Capability</strong> — ${esc(G.capability)}</div>
      <div class="card"><strong>Oracle</strong> — ${esc(G.oracle)}</div>
      <div class="card"><strong>Checks (criteria)</strong> — ${esc(G.criteria)}</div>
      <div class="card"><strong>Last verified</strong> — ${esc(G.verified)}</div>
      <h2>Where this comes from</h2>
      <p>The project keeps its claims in a manifest (<code>${esc(DATA.manifest_path)}</code>). Every claim there carries a trust strategy, an oracle, a structured tolerance, a reproducible command, an artifact, assumptions and failure modes; a validator refuses prose-only tolerances above research tier. The trust engine turns each measurement claim into a report and records <em>how</em> each value was established — run by a procedure, judged by a person, or sought and not found — so a fact and an interpretation can never be confused. The engine itself is deterministic; a model never decides whether a claim holds.</p>
      <p>What this page cannot tell you: whether a tolerance is scientifically meaningful, or whether an oracle is right. It makes those choices visible so they can be argued about.</p>
      <p><a href="#" data-go="table">Go to the claims →</a></p>
    </div>`;
    $('#view-about').querySelectorAll('a[data-go]').forEach(a => a.addEventListener('click', e => { e.preventDefault(); setView(a.dataset.go); update(); }));
  }

  // ---------- table ----------
  const COLS = [
    { key: 'title', label: 'Claim', q: 'claim', tip: G.claim, cell: c => `<td class="title">${esc(c.title)}<span class="id">${esc(c.id)}</span></td>`, val: c => c.title },
    { key: 'kind', label: 'Kind', q: 'kind', tip: G.kind._, cell: c => `<td><span class="pill kind" title="${esc(G.kind[c.kind] || '')}">${esc(c.kind)}</span></td>`, val: c => c.kind },
    { key: 'tier', label: 'Tier', q: 'tier', tip: G.tier._, cell: c => `<td><span class="pill tier-${esc(c.tier)}" title="${esc(G.tier[c.tier] || '')}">${esc(c.tier)}</span></td>`, val: c => TIERS.indexOf(c.tier) },
    { key: 'status', label: 'Status', q: 'status', tip: G.status._, cell: c => `<td><span class="pill status-${esc(c.status)}" title="${esc(G.status[c.status] || '')}">${esc(STATUS_LABEL[c.status] || c.status)}</span></td>`, val: c => c.status },
    { key: 'criteria', label: 'Checks', q: 'criteria', tip: G.criteria, cell: c => `<td title="${esc(G.criteria)}">${critCell(c)}</td>`, val: c => c.criteria.total ? c.criteria.pass / c.criteria.total : -1 },
    { key: 'subsystem', label: 'Subsystem', q: 'subsystem', tip: G.subsystem, cell: c => `<td>${esc(c.subsystem || '')}</td>`, val: c => c.subsystem || '' },
    { key: 'strategy', label: 'Strategy', q: 'strategy', tip: G.strategy._, cell: c => `<td>${tags(c.trust_strategy, v => G.strategy[v])}</td>`, val: c => (c.trust_strategy || []).join() },
    { key: 'oracles', label: 'Oracle', q: 'oracle', tip: G.oracle, cell: c => `<td>${tags(c.oracles)}</td>`, val: c => (c.oracles || []).join() },
    { key: 'capabilities', label: 'Capability', q: 'capability', tip: G.capability, cell: c => `<td>${tags(c.capabilities)}</td>`, val: c => (c.capabilities || []).join() },
    { key: 'verified', label: 'Last verified', q: 'verified', tip: G.verified, cell: c => `<td>${esc((c.last_verified && c.last_verified.date) || '—')}</td>`, val: c => (c.last_verified && c.last_verified.date) || '' },
  ];
  function tags(a, tip) { return a && a.length ? `<div class="tags">${a.map(x => `<span ${tip && tip(x) ? `title="${esc(tip(x))}"` : ''}>${esc(x)}</span>`).join('')}</div>` : ''; }
  function critCell(c) {
    const k = c.criteria; if (!k.total) return `<span class="crit n">${c.kind === 'measurement' ? '—' : 'n/a'}</span>`;
    const w = x => (100 * x / k.total).toFixed(1) + '%';
    return `<span class="bar"><i class="p" style="width:${w(k.pass)}"></i><i class="f" style="width:${w(k.fail)}"></i><i class="n" style="width:${w(k.not_assessed + k.partial + k.other)}"></i></span><span class="crit"><span class="p">${k.pass}✓</span> <span class="f">${k.fail}✗</span> <span class="n">${k.not_assessed}?</span></span>`;
  }
  function renderTable(vis) {
    const s = state.sort;
    const rows = [...vis].sort((a, b) => { const col = COLS.find(c => c.key === s.key); const va = col.val(a), vb = col.val(b); const r = va < vb ? -1 : va > vb ? 1 : a.id.localeCompare(b.id); return s.asc ? r : -r; });
    if (!rows.length) { $('#view-table').innerHTML = '<div class="empty">No claims match the current filters.</div>'; return; }
    $('#view-table').innerHTML = `<div style="overflow-x:auto"><table class="claims"><thead><tr>${COLS.map(c => `<th data-key="${c.key}" title="Sort by ${esc(c.label)}" class="${s.key === c.key ? 'sorted' + (s.asc ? ' asc' : '') : ''}">${c.label}${qbtn(c.q, c.label)}</th>`).join('')}</tr></thead><tbody>${rows.map(c => `<tr class="row" tabindex="0" data-id="${esc(c.id)}">${COLS.map(col => col.cell(c)).join('')}</tr>`).join('')}</tbody></table></div>`;
    $('#view-table').querySelectorAll('th').forEach(th => th.addEventListener('click', e => { if (e.target.closest('button.q')) return; const k = th.dataset.key; if (state.sort.key === k) state.sort.asc = !state.sort.asc; else state.sort = { key: k, asc: true }; update(); }));
    $('#view-table').querySelectorAll('tr.row').forEach(tr => { tr.addEventListener('click', () => openDetail(tr.dataset.id)); tr.addEventListener('keydown', e => { if (e.key === 'Enter') openDetail(tr.dataset.id); }); });
  }

  // ---------- matrix ----------
  function renderMatrix(vis) {
    const rowsKey = c => c.subsystem || (c.kind !== 'measurement' ? `(${c.kind})` : '(no subsystem)');
    const subs = [...new Set(vis.map(rowsKey))].sort();
    const cell = (sub, tier) => vis.filter(c => rowsKey(c) === sub && c.tier === tier);
    const summ = cs => { if (!cs.length) return ''; const p = cs.reduce((a, c) => a + c.criteria.pass, 0), t = cs.reduce((a, c) => a + c.criteria.total, 0); const ct = cs.filter(c => c.status === 'contested').length; return `<small>${t ? `${p}/${t} checks ✓` : ''}${ct ? ` · ${ct} contested` : ''}</small>`; };
    let html = `<p class="note">Each cell counts the claims about one <strong>subsystem</strong> (${esc(G.subsystem)}) at one <strong>tier</strong> (${esc(G.tier._)}). Click a cell to see those claims. An empty <em>release</em> cell means that subsystem has no release-grade evidence yet.</p>`;
    html += `<table class="matrix"><thead><tr><th class="rowh">Subsystem${qbtn('subsystem', 'a subsystem')}</th>${TIERS.map(t => `<th title="${esc(G.tier[t])}">${t}${qbtn('tier', 'a tier')}</th>`).join('')}<th>total</th></tr></thead><tbody>`;
    for (const s of subs) {
      html += `<tr><th class="rowh">${esc(s)}</th>`;
      for (const t of TIERS) { const cs = cell(s, t); html += `<td class="cell ${cs.length ? '' : 'c0'}" tabindex="0" data-sub="${esc(s)}" data-tier="${t}">${cs.length || '·'}${summ(cs)}</td>`; }
      html += `<td>${vis.filter(c => rowsKey(c) === s).length}</td></tr>`;
    }
    html += `<tr><th class="rowh">total</th>${TIERS.map(t => `<td>${vis.filter(c => c.tier === t).length}</td>`).join('')}<td>${vis.length}</td></tr></tbody></table>`;
    $('#view-matrix').innerHTML = html;
    $('#view-matrix').querySelectorAll('td.cell').forEach(td => { const go = () => { state.sel.tier = new Set([td.dataset.tier]); state.sel.subsystem = td.dataset.sub.startsWith('(') ? new Set() : new Set([td.dataset.sub]); setView('table'); update(); }; td.addEventListener('click', go); td.addEventListener('keydown', e => { if (e.key === 'Enter') go(); }); });
  }

  // ---------- graph ----------
  let cy = null;
  const STATUS_COLOR = { current: '#28a745', contested: '#e0b33a', superseded: '#9aa6b2', not_synthesized: '#9aa6b2', error: '#c8323e' };
  function renderLegend() {
    $('#graph-note').innerHTML = `Every <strong>claim</strong> (circle) is linked to the <strong>oracles</strong> it is checked against and the <strong>capabilities</strong> it backs. Drag to pan, scroll to zoom, click a claim for its report, click an oracle or capability to list the claims that depend on it. Colour is the claim's status; a thick border marks release tier.`;
    $('#legend').innerHTML = `
      ${qbtn('graph', 'this graph')}
      <span title="${esc(G.status.current)}"><i style="background:#28a745"></i>current</span>
      <span title="${esc(G.status.contested)}"><i style="background:#e0b33a"></i>contested</span>
      <span title="${esc(G.status.superseded + ' ' + G.status.not_synthesized)}"><i style="background:#9aa6b2"></i>superseded / not synthesized</span>
      <span title="${esc(G.status.error)}"><i style="background:#c8323e"></i>error</span>
      <span title="${esc(G.oracle)}"><i class="dia" style="background:#0b4f9c"></i>oracle</span>
      <span title="${esc(G.capability)}"><i class="sq" style="background:#5b2a86"></i>capability</span>
      <span title="${esc(G.subsystem)}"><i class="sq" style="background:#e67e22"></i>subsystem</span>
      <label title="${esc(G.capability)}"><input type="checkbox" id="g-caps" checked> show capabilities</label>
      <label title="${esc(G.subsystem)}"><input type="checkbox" id="g-subs"> show subsystems</label>
      <label><input type="checkbox" id="g-labels" checked> claim labels</label>`;
    ['g-caps', 'g-subs', 'g-labels'].forEach(id => $('#' + id).addEventListener('change', () => renderGraph(filtered())));
  }
  function renderGraph(vis) {
    const el = $('#graph');
    if (typeof cytoscape !== 'function') { el.innerHTML = '<div class="empty">The graph needs the Cytoscape library (loaded from cdnjs) and it could not be loaded. The Claims and Coverage views do not depend on it.</div>'; return; }
    const showCaps = $('#g-caps').checked, showSubs = $('#g-subs').checked, showLabels = $('#g-labels').checked;
    const nodes = [], edges = [], seen = new Set();
    const add = (id, data) => { if (!seen.has(id)) { seen.add(id); nodes.push({ data: { id, ...data } }); } };
    const short = t => t.length > 42 ? t.slice(0, 40) + '…' : t;
    for (const c of vis) {
      add('c:' + c.id, { label: showLabels ? short(c.title) : '', full: c.title, type: 'claim', color: STATUS_COLOR[c.status] || '#9aa6b2', cid: c.id, border: c.tier === 'release' ? 5 : c.tier === 'ci' ? 2 : 1, size: c.tier === 'release' ? 30 : 22 });
      for (const o of c.oracles || []) { add('o:' + o, { label: o, type: 'oracle' }); edges.push({ data: { id: `e${edges.length}`, source: 'c:' + c.id, target: 'o:' + o, type: 'oracle' } }); }
      if (showCaps) for (const k of c.capabilities || []) { add('k:' + k, { label: k, type: 'capability' }); edges.push({ data: { id: `e${edges.length}`, source: 'c:' + c.id, target: 'k:' + k, type: 'capability' } }); }
      if (showSubs && c.subsystem) { add('s:' + c.subsystem, { label: c.subsystem, type: 'subsystem' }); edges.push({ data: { id: `e${edges.length}`, source: 'c:' + c.id, target: 's:' + c.subsystem, type: 'subsystem' } }); }
      const rep = DATA.reports[c.id];
      if (rep && rep._graph && Array.isArray(rep._graph.review_events)) for (const ev of rep._graph.review_events) {
        const bk = ev && ev.kind && ev.kind.data && ev.kind.data.backed_by;
        if (bk && byId[bk]) { add('c:' + bk, { label: showLabels ? short(byId[bk].title) : '', full: byId[bk].title, type: 'claim', color: STATUS_COLOR[byId[bk].status] || '#9aa6b2', cid: bk, border: 2, size: 22 }); edges.push({ data: { id: `e${edges.length}`, source: 'c:' + bk, target: 'c:' + c.id, type: 'challenge' } }); }
      }
    }
    if (cy) { cy.destroy(); cy = null; }
    el.innerHTML = '';
    if (!nodes.length) { el.innerHTML = '<div class="empty">No claims match the current filters.</div>'; return; }
    cy = cytoscape({
      container: el, elements: { nodes, edges }, wheelSensitivity: 0.2, minZoom: 0.2, maxZoom: 3,
      style: [
        { selector: 'node', style: { 'label': 'data(label)', 'font-family': 'IBM Plex Sans, sans-serif', 'font-size': 10, 'text-wrap': 'wrap', 'text-max-width': 150, 'text-valign': 'bottom', 'text-margin-y': 5, 'color': '#1f2a33', 'text-background-color': '#ffffff', 'text-background-opacity': 0.9, 'text-background-padding': 2, 'text-background-shape': 'roundrectangle', 'width': 'data(size)', 'height': 'data(size)', 'min-zoomed-font-size': 7 } },
        { selector: 'node[type="claim"]', style: { 'background-color': 'data(color)', 'border-width': 'data(border)', 'border-color': '#1f2a33', 'shape': 'ellipse' } },
        { selector: 'node[type="oracle"]', style: { 'background-color': '#0b4f9c', 'shape': 'diamond', 'width': 34, 'height': 34, 'font-weight': 'bold', 'font-size': 12, 'text-valign': 'top', 'text-margin-y': -5 } },
        { selector: 'node[type="capability"]', style: { 'background-color': '#5b2a86', 'shape': 'round-rectangle', 'width': 26, 'height': 16, 'font-size': 9, 'color': '#5b2a86' } },
        { selector: 'node[type="subsystem"]', style: { 'background-color': '#e67e22', 'shape': 'round-rectangle', 'width': 26, 'height': 16, 'font-size': 9, 'color': '#a35b12' } },
        { selector: 'edge', style: { 'width': 1.2, 'line-color': '#c8d0d8', 'curve-style': 'straight' } },
        { selector: 'edge[type="capability"]', style: { 'line-color': '#d9c7ec', 'line-style': 'dashed' } },
        { selector: 'edge[type="subsystem"]', style: { 'line-color': '#f5d3b3', 'line-style': 'dotted' } },
        { selector: 'edge[type="challenge"]', style: { 'line-color': '#c8323e', 'width': 2, 'target-arrow-shape': 'triangle', 'target-arrow-color': '#c8323e', 'curve-style': 'bezier' } },
        { selector: 'node:selected', style: { 'border-width': 4, 'border-color': '#000' } },
        { selector: '.dim', style: { 'opacity': 0.15 } },
      ],
      layout: { name: 'cose', animate: false, nodeRepulsion: () => 600000, idealEdgeLength: () => 130, edgeElasticity: () => 60, gravity: 0.15, numIter: 1500, nodeOverlap: 30, componentSpacing: 80, padding: 30, randomize: true },
    });
    cy.fit(undefined, 30);
    cy.on('tap', 'node[type="claim"]', e => openDetail(e.target.data('cid')));
    cy.on('tap', 'node[type="oracle"]', e => { state.sel.oracles = new Set([e.target.data('label')]); setView('table'); update(); });
    cy.on('tap', 'node[type="capability"]', e => { state.sel.capabilities = new Set([e.target.data('label')]); setView('table'); update(); });
    cy.on('tap', 'node[type="subsystem"]', e => { state.sel.subsystem = new Set([e.target.data('label')]); setView('table'); update(); });
    cy.on('mouseover', 'node', e => { const n = e.target; cy.elements().addClass('dim'); n.closedNeighborhood().removeClass('dim'); if (n.data('full')) n.data('label', short(n.data('full')) === n.data('label') ? n.data('full') : n.data('label')); });
    cy.on('mouseout', 'node', e => { cy.elements().removeClass('dim'); const n = e.target; if (n.data('full') && showLabels) n.data('label', short(n.data('full'))); });
  }

  // ---------- detail drawer ----------
  let opener = null;
  function openDetail(id) {
    const c = byId[id]; if (!c) return;
    if (!$('#drawer').classList.contains('open')) opener = document.activeElement;
    $('#drawer-title').textContent = c.id;
    const QT = { 'Trust strategy': 'strategy', 'Subsystem': 'subsystem', 'Oracles': 'oracle', 'Capabilities': 'capability' };
    const rel = (label, vals, facet, tip) => vals && vals.length ? `<dt title="${esc(tip || '')}">${label}${qbtn(QT[label], label)}</dt><dd class="rel">${vals.map(v => `<a href="#" data-facet="${facet}" data-val="${esc(v)}" title="Show all claims with this ${label.toLowerCase()}">${esc(v)}</a>`).join(', ')}</dd>` : '';
    const lv = c.last_verified || {};
    let html = `<h1>${esc(c.title)}</h1>
      <p><span class="pill kind" title="${esc(G.kind[c.kind] || '')}">${esc(c.kind)}</span>${qbtn('kind', 'kind')} <span class="pill tier-${esc(c.tier)}" title="${esc(G.tier[c.tier] || '')}">${esc(c.tier)} tier</span>${qbtn('tier', 'tier')} <span class="pill status-${esc(c.status)}" title="${esc(G.status[c.status] || '')}">${esc(STATUS_LABEL[c.status] || c.status)}</span>${qbtn('status', 'status')}</p>
      <p class="lead-in">What is claimed</p>
      <div class="claimtext">${esc(c.claim || '')}</div>
      <p class="lead-in">How it is checked, and by whom</p>
      <dl class="kv">
        ${rel('Trust strategy', c.trust_strategy, 'trust_strategy', G.strategy._)}
        ${c.subsystem ? rel('Subsystem', [c.subsystem], 'subsystem', G.subsystem) : ''}
        ${rel('Oracles', c.oracles, 'oracles', G.oracle)}
        ${rel('Capabilities', c.capabilities, 'capabilities', G.capability)}
        ${c.provenance ? `<dt title="${esc(G.provenance)}">Provenance${qbtn('provenance', 'provenance')}</dt><dd>${esc(c.provenance)}${c.review_status ? ` · ${esc(c.review_status)}` : ''}</dd>` : ''}
        ${c.command ? `<dt>Command</dt><dd><code>${esc(c.command)}</code></dd>` : ''}
        ${c.case ? `<dt>Case notes</dt><dd><code>${esc(c.case)}</code></dd>` : ''}
        ${c.pattern ? `<dt>Pattern</dt><dd><code>${esc(c.pattern)}</code></dd>` : ''}
        <dt title="${esc(G.verified)}">Last verified${qbtn('verified', 'last verified')}</dt><dd>${lv.date ? `${esc(lv.date)}${lv.commit ? ` @ <code>${esc(lv.commit)}</code>` : ''}${lv.value != null ? ` · observed ${esc(lv.value)}` : ''}` : '<em>no observation recorded in the manifest</em>'}</dd>
        <dt>Assumptions / failure modes</dt><dd>${c.n_assumptions} / ${c.n_failure_modes} recorded in the manifest</dd>
        <dt>Source</dt><dd><code>${esc(c.source_path)}</code></dd>
      </dl>`;
    if (DATA.fragments[c.id]) html += `<div class="section-help"><strong>Below: the trust report</strong> computed by the engine. Each tolerance is one criterion; <em>Not assessed</em> means the check is defined but no observed value was available when this page was built. The attestation graph shows how the claim, its evidence and each criterion are linked.</div>${DATA.fragments[c.id]}`;
    else if (c.skip_reason) html += `<div class="section-help"><strong>No trust report.</strong> ${esc(G.status[c.status] || '')} <span style="color:#6c757d">(${esc(c.skip_reason)})</span></div>`;
    const body = $('#drawer-body'); body.innerHTML = html; body.scrollTop = 0;
    body.querySelectorAll('.rel a').forEach(a => a.addEventListener('click', e => { e.preventDefault(); state.sel[a.dataset.facet] = new Set([a.dataset.val]); closeDetail(); setView('table'); update(); }));
    $('#drawer').classList.add('open'); $('#drawer').setAttribute('aria-hidden', 'false'); $('#backdrop').classList.add('open');
    $('#drawer-close').focus();
    if (window.__mermaid) { const nodes = body.querySelectorAll('.mermaid'); if (nodes.length) window.__mermaid.run({ nodes }).then(() => addGraphTools(body)).catch(() => {}); }
    history.replaceState(null, '', '#' + encodeURIComponent(c.id));
  }
  // ---------- full-screen attestation graph ----------
  const big = document.getElementById('big');
  function addGraphTools(body) {
    body.querySelectorAll('.mermaid').forEach(pre => {
      const svg = pre.querySelector('svg'); if (!svg || pre.previousElementSibling && pre.previousElementSibling.classList.contains('graph-tools')) return;
      const bar = document.createElement('div'); bar.className = 'graph-tools';
      bar.innerHTML = `<button type="button" class="expand">⤢ Expand to full screen</button><span class="hintt">The graph links the claim, its evidence, each criterion and any review events. Full screen lets you pan and zoom.</span>`;
      pre.parentNode.insertBefore(bar, pre);
      bar.querySelector('.expand').addEventListener('click', () => openBig(svg));
    });
  }
  let bigOpener = null, bigResize = null;
  function openBig(svg) {
    bigOpener = document.activeElement;
    big.innerHTML = `<div class="tools"><strong>Attestation graph</strong><span class="sp">Drag to pan · scroll to zoom · Esc to close</span><button type="button" class="fit">Fit</button><button type="button" class="zin">+</button><button type="button" class="zout">−</button><button type="button" class="close">Close ✕</button></div><div class="stage"></div>`;
    const stage = big.querySelector('.stage'); const clone = svg.cloneNode(true); clone.removeAttribute('width'); clone.removeAttribute('height'); clone.style.width = ''; stage.appendChild(clone);
    big.hidden = false;
    let sc = 1, tx = 0, ty = 0;
    const vb = (clone.viewBox && clone.viewBox.baseVal && clone.viewBox.baseVal.width) ? clone.viewBox.baseVal : null;
    const natural = vb ? { w: vb.width, h: vb.height } : { w: clone.getBoundingClientRect().width || 800, h: clone.getBoundingClientRect().height || 400 };
    clone.setAttribute('width', natural.w); clone.setAttribute('height', natural.h);
    const apply = () => { clone.style.transform = `translate(${tx}px, ${ty}px) scale(${sc})`; };
    const fit = () => { const W = stage.clientWidth, H = stage.clientHeight; sc = Math.min(W / natural.w, H / natural.h) * 0.95; tx = (W - natural.w * sc) / 2; ty = (H - natural.h * sc) / 2; apply(); };
    const zoomAt = (f, cx, cy) => { const ns = Math.min(8, Math.max(0.1, sc * f)); tx = cx - (cx - tx) * (ns / sc); ty = cy - (cy - ty) * (ns / sc); sc = ns; apply(); };
    fit();
    stage.addEventListener('wheel', e => { e.preventDefault(); const r = stage.getBoundingClientRect(); zoomAt(e.deltaY < 0 ? 1.15 : 1 / 1.15, e.clientX - r.left, e.clientY - r.top); }, { passive: false });
    let drag = null;
    stage.addEventListener('pointerdown', e => { drag = { x: e.clientX - tx, y: e.clientY - ty }; stage.classList.add('dragging'); stage.setPointerCapture(e.pointerId); });
    stage.addEventListener('pointermove', e => { if (!drag) return; tx = e.clientX - drag.x; ty = e.clientY - drag.y; apply(); });
    const endDrag = () => { drag = null; stage.classList.remove('dragging'); };
    stage.addEventListener('pointerup', endDrag); stage.addEventListener('pointercancel', endDrag);
    big.querySelector('.fit').addEventListener('click', fit);
    big.querySelector('.zin').addEventListener('click', () => zoomAt(1.25, stage.clientWidth / 2, stage.clientHeight / 2));
    big.querySelector('.zout').addEventListener('click', () => zoomAt(1 / 1.25, stage.clientWidth / 2, stage.clientHeight / 2));
    big.querySelector('.close').addEventListener('click', closeBig);
    if (bigResize) window.removeEventListener('resize', bigResize);
    bigResize = fit; window.addEventListener('resize', bigResize);
    big.querySelector('.close').focus();
  }
  function closeBig() {
    if (big.hidden) return;
    if (bigResize) { window.removeEventListener('resize', bigResize); bigResize = null; }
    big.hidden = true; big.innerHTML = '';
    if (bigOpener && document.contains(bigOpener)) bigOpener.focus(); bigOpener = null;
  }
  document.addEventListener('keydown', e => { if (e.key === 'Escape' && !big.hidden) { e.stopImmediatePropagation(); closeBig(); } }, true);

  function closeDetail() {
    if (!$('#drawer').classList.contains('open')) return;
    closeBig();
    closePop(); if (pop.parentElement !== document.getElementById('site')) document.getElementById('site').appendChild(pop);
    $('#drawer').classList.remove('open'); $('#drawer').setAttribute('aria-hidden', 'true'); $('#backdrop').classList.remove('open');
    history.replaceState(null, '', location.pathname + location.search);
    if (opener && document.contains(opener) && typeof opener.focus === 'function') opener.focus(); else $('nav.tabs button[aria-selected="true"]').focus();
    opener = null;
  }
  $('#drawer-close').addEventListener('click', closeDetail);
  $('#backdrop').addEventListener('click', closeDetail);
  document.addEventListener('keydown', e => { if (e.key === 'Escape') closeDetail(); });

  // ---------- views ----------
  function setView(v) { state.view = v; document.querySelectorAll('nav.tabs button').forEach(b => b.setAttribute('aria-selected', String(b.dataset.view === v))); for (const k of ['about', 'table', 'matrix', 'graph']) $('#view-' + k).hidden = k !== v; $('#summary').hidden = v === 'about'; $('#facets').hidden = v === 'about'; $('.layout').classList.toggle('no-rail', v === 'about'); }
  document.querySelectorAll('nav.tabs button').forEach(b => b.addEventListener('click', () => { setView(b.dataset.view); update(); }));

  function update() {
    persist(); closePop();
    const vis = filtered();
    const active = document.activeElement && document.activeElement.id === 'q';
    const pos = active ? document.activeElement.selectionStart : null;
    renderFacets(); renderHint(); renderSummary(vis);
    if (active) { const q = $('#q'); q.focus(); q.setSelectionRange(pos, pos); }
    if (state.view === 'about') renderAbout();
    else if (state.view === 'table') renderTable(vis);
    else if (state.view === 'matrix') renderMatrix(vis);
    else renderGraph(vis);
  }
  renderLegend();
  const deepLink = location.hash.length > 1 ? decodeURIComponent(location.hash.slice(1)) : null;
  setView(state.seen || deepLink ? 'table' : 'about'); state.seen = true; update();
  if (deepLink && byId[deepLink]) openDetail(deepLink);
})();
"##;
