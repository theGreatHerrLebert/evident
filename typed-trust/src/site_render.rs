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
pub fn render_site(
    manifest_path: &str,
    project: &str,
    synthesized_at: &str,
    raw_claims: &[Value],
    reports: &[Value],
    skipped: &[Value],
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

const SITE_CSS: &str = r#"

/* ---- tokens: light on bare :root, dark via prefers-color-scheme (unless data-theme=light) and data-theme=dark ---- */
:root { --ground:#f2f5f8; --surface:#ffffff; --surface-2:#f7f9fb; --line:#d9e0e7; --ink:#1f2a33; --muted:#6a7885; --accent:#0b4f9c; --accent-ink:#ffffff; --hover:#eef3f8;
        --pass:#2e8b57; --warn:#c99700; --fail:#c8323e; --na:#9aa6b2;
        --font:"IBM Plex Sans", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; --mono:"IBM Plex Mono", "SF Mono", Menlo, Consolas, monospace; }
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --ground:#12181e; --surface:#1a222b; --surface-2:#202a34; --line:#2c3743; --ink:#e4eaf0; --muted:#94a3b3; --accent:#6fa8e6; --accent-ink:#0f1b2a; --hover:#243040; --pass:#4cbf7a; --warn:#e0b33a; --fail:#e5606a; --na:#7c8894; } }
:root[data-theme="dark"] { --ground:#12181e; --surface:#1a222b; --surface-2:#202a34; --line:#2c3743; --ink:#e4eaf0; --muted:#94a3b3; --accent:#6fa8e6; --accent-ink:#0f1b2a; --hover:#243040; --pass:#4cbf7a; --warn:#e0b33a; --fail:#e5606a; --na:#7c8894; }
/* ---- site chrome (overrides the fragment's document-level rules) ---- */
body { max-width: none; margin: 0; padding: 0; background: var(--ground); color: var(--ink); font-family: var(--font); }
#site .num, #site .crit, #site .id, #site code, #site .mono { font-variant-numeric: tabular-nums; }
#site h1 { border: none; margin: 0; font-size: 1.25rem; }
#site h2 { margin-top: 0; border: none; }
#site header {
  display: flex; align-items: baseline; gap: 1rem; flex-wrap: wrap;
  padding: 0.8rem 1.25rem; background: var(--surface); border-bottom: 1px solid var(--line);
  position: sticky; top: 0; z-index: 5;
}
#site header .meta { color: var(--muted); font-size: 0.85rem; }
#site header .meta code { font-size: 0.8rem; }
#site nav.tabs { margin-left: auto; display: flex; gap: 0.25rem; }
#site nav.tabs button {
  border: 1px solid var(--line); background: var(--surface); color: var(--ink); padding: 0.35rem 0.8rem; border-radius: 4px;
  cursor: pointer; font: inherit; font-size: 0.9rem;
}
#site nav.tabs button[aria-selected="true"] { background: var(--accent); color: var(--accent-ink); border-color: var(--accent); }
#site .layout { display: grid; grid-template-columns: 250px minmax(0, 1fr); min-height: calc(100vh - 56px); }
#site aside {
  padding: 1rem; background: var(--surface); border-right: 1px solid var(--line); font-size: 0.88rem;
  overflow-y: auto; max-height: calc(100vh - 56px); position: sticky; top: 56px;
}
#site aside h3 { margin: 0.9rem 0 0.35rem; font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
#site aside h3:first-child { margin-top: 0; }
#site aside label { display: flex; align-items: center; gap: 0.4rem; margin: 0.15rem 0; cursor: pointer; }
#site aside label .n { margin-left: auto; color: var(--muted); font-size: 0.8rem; }
#site aside input[type="search"] { width: 100%; padding: 0.4rem 0.5rem; border: 1px solid var(--line); border-radius: 4px; font: inherit; background: var(--surface); color: var(--ink); }
#site button:focus-visible, #site input:focus-visible, #site tr.row:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
#site aside button.clear { margin-top: 0.75rem; font: inherit; font-size: 0.82rem; background: none; color: var(--ink); border: 1px solid var(--line); border-radius: 4px; padding: 0.25rem 0.6rem; cursor: pointer; }
#site main { padding: 1rem 1.25rem; min-width: 0; }
#site .summary { display: flex; gap: 0.75rem; flex-wrap: wrap; margin-bottom: 1rem; }
#site .summary .tile { background: var(--surface); border: 1px solid var(--line); border-radius: 6px; padding: 0.5rem 0.9rem; min-width: 110px; }
#site .summary .tile .num { font-size: 1.4rem; font-weight: 700; line-height: 1.1; }
#site .summary .tile .lbl { color: var(--muted); font-size: 0.78rem; }
#site table.claims { width: 100%; border-collapse: collapse; background: var(--surface); font-size: 0.86rem; }
#site table.claims th, #site table.claims td { padding: 0.45rem 0.6rem; border-bottom: 1px solid var(--line); text-align: left; vertical-align: top; }
#site table.claims th { background: var(--surface-2); position: sticky; top: 56px; cursor: pointer; user-select: none; white-space: nowrap; }
#site table.claims th.sorted::after { content: " ▾"; color: var(--muted); }
#site table.claims th.sorted.asc::after { content: " ▴"; }
#site table.claims tr.row { cursor: pointer; }
#site table.claims tr.row:hover { background: var(--hover); }
#site table.claims td.title { max-width: 34rem; }
#site table.claims td.title .id { display: block; color: var(--muted); font-family: var(--mono); font-size: 0.75rem; }
#site .pill { display: inline-block; padding: 0.05rem 0.5rem; border-radius: 10px; font-size: 0.75rem; font-weight: 600; border: 1px solid transparent; white-space: nowrap; }
#site .pill.tier-ci { background: color-mix(in srgb, var(--accent) 16%, var(--surface)); color: var(--accent); }
#site .pill.tier-release { background: color-mix(in srgb, var(--pass) 16%, var(--surface)); color: var(--pass); }
#site .pill.tier-research { background: color-mix(in srgb, #7c4dbd 18%, var(--surface)); color: #7c4dbd; }
#site .pill.kind { background: var(--surface-2); color: var(--muted); }
#site .pill.status-current { background: color-mix(in srgb, var(--pass) 18%, var(--surface)); color: var(--pass); }
#site .pill.status-contested { background: var(--surface)3cd; color: #856404; }
#site .pill.status-superseded { background: var(--surface-2); color: var(--muted); }
#site .pill.status-error { background: color-mix(in srgb, var(--fail) 18%, var(--surface)); color: var(--fail); }
#site .pill.status-not_synthesized { background: var(--surface-2); color: var(--muted); }
#site .crit { font-family: var(--mono); font-size: 0.78rem; white-space: nowrap; }
#site .crit .p { color: var(--pass); } #site .crit .f { color: var(--fail); } #site .crit .n { color: var(--muted); }
@media (prefers-reduced-motion: reduce) { #site #drawer { transition: none; } }
#site .bar { display: inline-block; height: 8px; width: 90px; background: var(--line); border-radius: 4px; overflow: hidden; vertical-align: middle; margin-right: 0.4rem; }
#site .bar i { display: block; height: 100%; float: left; }
#site .bar i.p { background: var(--pass); } #site .bar i.f { background: var(--fail); } #site .bar i.n { background: var(--na); }
#site .tags { display: flex; flex-wrap: wrap; gap: 0.2rem; }
#site .tags span { background: var(--surface-2); border: 1px solid var(--line); border-radius: 3px; padding: 0 0.35rem; font-size: 0.74rem; color: var(--ink); white-space: nowrap; }
#site .empty { color: var(--muted); padding: 2rem; text-align: center; }
/* matrix */
#site table.matrix { border-collapse: collapse; background: var(--surface); font-size: 0.86rem; }
#site table.matrix th, #site table.matrix td { border: 1px solid var(--line); padding: 0.4rem 0.7rem; text-align: center; }
#site table.matrix th { background: var(--surface-2); }
#site table.matrix th.rowh { text-align: left; font-weight: 600; }
#site table.matrix td.cell { cursor: pointer; }
#site table.matrix td.cell:hover { outline: 2px solid var(--ink); }
#site table.matrix td.c0 { color: #ccc; }
#site table.matrix td.cell small { display: block; color: var(--muted); font-size: 0.72rem; }
#site .matrix-note { color: var(--muted); font-size: 0.85rem; margin: 0.5rem 0 1rem; }
/* graph */
#site #graph { height: calc(100vh - 150px); min-height: 480px; background: #ffffff; border: 1px solid var(--line); border-radius: 6px; }
#site .legend { display: flex; gap: 1rem; flex-wrap: wrap; font-size: 0.8rem; color: var(--ink); margin: 0.5rem 0; align-items: center; }
#site .legend i { display: inline-block; width: 12px; height: 12px; border-radius: 50%; margin-right: 0.3rem; vertical-align: -1px; }
#site .legend i.sq { border-radius: 2px; } #site .legend i.dia { transform: rotate(45deg); border-radius: 1px; width: 10px; height: 10px; }
#site .legend label { color: var(--ink); }
#site .legend label { display: inline-flex; gap: 0.3rem; align-items: center; margin-left: auto; }
/* detail drawer */
#site #drawer {
  position: fixed; top: 0; right: 0; height: 100vh; width: min(760px, 92vw); background: var(--surface)fff; color: var(--ink);
  border-left: 1px solid var(--line); box-shadow: -8px 0 24px rgba(0,0,0,0.08); z-index: 20;
  transform: translateX(100%); transition: transform 0.18s ease; display: flex; flex-direction: column;
}
#site #drawer.open { transform: none; }
#site #drawer .bar-top { display: flex; align-items: center; gap: 0.75rem; padding: 0.6rem 1rem; border-bottom: 1px solid #dde3e8; }
#site #drawer .bar-top button { margin-left: auto; font: inherit; border: 1px solid #dde3e8; background: var(--surface)fff; color: #2c3e50; border-radius: 4px; padding: 0.25rem 0.6rem; cursor: pointer; }
#site #drawer .body { overflow-y: auto; padding: 0 1.25rem 2rem; font-size: 0.92rem; }
#site #drawer .body h1 { font-size: 1.15rem; margin: 1rem 0 0.4rem; border-bottom: 2px solid #2c3e50; padding-bottom: 0.3rem; }
#site #drawer .body h2 { margin-top: 1.6rem; border-bottom: 1px solid #ccc; }
#site #drawer .claimtext { background: #f8f9fa; border-left: 4px solid #2c3e50; padding: 0.6rem 0.9rem; margin: 0.6rem 0; white-space: pre-wrap; }
#site #drawer dl.kv { display: grid; grid-template-columns: max-content 1fr; gap: 0.2rem 0.9rem; margin: 0.6rem 0; }
#site #drawer dl.kv dt { color: #6c757d; } #site #drawer dl.kv dd { margin: 0; }
#site #drawer .rel a { cursor: pointer; color: #0b4f9c; text-decoration: underline; }
#site #backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.15); z-index: 15; display: none; }
#site #backdrop.open { display: block; }
@media (max-width: 900px) { #site .layout { grid-template-columns: 1fr; } #site aside { position: static; max-height: none; border-right: none; border-bottom: 1px solid var(--line); } }
"#;

const SITE_BODY: &str = r#"
<div id="site">
  <header>
    <h1 id="site-title">EVIDENT</h1>
    <span class="meta">manifest <code id="manifest-path"></code> · synthesized by typed-trust</span>
    <nav class="tabs" role="tablist">
      <button role="tab" data-view="table" aria-selected="true">Claims</button>
      <button role="tab" data-view="matrix" aria-selected="false">Coverage</button>
      <button role="tab" data-view="graph" aria-selected="false">Graph</button>
    </nav>
  </header>
  <div class="layout">
    <aside id="facets"></aside>
    <main>
      <div class="summary" id="summary"></div>
      <section id="view-table"></section>
      <section id="view-matrix" hidden></section>
      <section id="view-graph" hidden>
        <div class="legend">
          <span><i style="background:#28a745"></i>claim: current</span>
          <span><i style="background:#e0b33a"></i>contested</span>
          <span><i style="background:#9aa6b2"></i>superseded / not synthesized</span>
          <span><i style="background:#c8323e"></i>error</span>
          <span><i class="dia" style="background:#0b4f9c"></i>oracle</span>
          <span><i class="sq" style="background:#5b2a86"></i>capability</span>
          <span><i class="sq" style="background:#e67e22"></i>subsystem</span>
          <label><input type="checkbox" id="g-caps" checked> capabilities</label>
          <label><input type="checkbox" id="g-subs"> subsystems</label>
        </div>
        <div id="graph"></div>
      </section>
    </main>
  </div>
  <div id="backdrop"></div>
  <div id="drawer" aria-hidden="true">
    <div class="bar-top"><strong id="drawer-title"></strong><button id="drawer-close">Close ✕</button></div>
    <div class="body" id="drawer-body"></div>
  </div>
</div>
"#;

const SITE_JS: &str = r#"
(function () {
  const DATA = JSON.parse(document.getElementById('evident-data').textContent);
  const CLAIMS = DATA.claims;
  const byId = Object.fromEntries(CLAIMS.map(c => [c.id, c]));
  const $ = (s, el) => (el || document).querySelector(s);
  const esc = s => String(s == null ? '' : s).replace(/[&<>"]/g, ch => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[ch]));
  const TIERS = ['ci', 'release', 'research'];
  const STATUS_LABEL = { current: 'Current', contested: 'Contested', superseded: 'Superseded', error: 'Error', not_synthesized: 'Not synthesized' };

  document.title = DATA.project ? `EVIDENT — ${DATA.project}` : 'EVIDENT claims';
  $('#site-title').textContent = DATA.project ? `EVIDENT · ${DATA.project}` : 'EVIDENT';
  $('#manifest-path').textContent = DATA.manifest_path;

  // ---------- facets ----------
  const FACETS = [
    { key: 'kind', label: 'Kind', get: c => [c.kind] },
    { key: 'tier', label: 'Tier', get: c => [c.tier], order: TIERS },
    { key: 'status', label: 'Status', get: c => [c.status], order: ['current', 'contested', 'superseded', 'error', 'not_synthesized'], fmt: v => STATUS_LABEL[v] || v },
    { key: 'trust_strategy', label: 'Trust strategy', get: c => c.trust_strategy || [] },
    { key: 'subsystem', label: 'Subsystem', get: c => c.subsystem ? [c.subsystem] : [] },
    { key: 'oracles', label: 'Oracle', get: c => c.oracles || [] },
    { key: 'capabilities', label: 'Capability', get: c => c.capabilities || [] },
  ];
  const state = { q: '', sel: Object.fromEntries(FACETS.map(f => [f.key, new Set()])), sort: { key: 'tier', asc: true }, view: 'table' };
  try {
    const saved = JSON.parse(localStorage.getItem('evident-site-state') || 'null');
    if (saved && saved.sel) { for (const k in saved.sel) if (state.sel[k]) state.sel[k] = new Set(saved.sel[k]); state.q = saved.q || ''; state.sort = saved.sort || state.sort; }
  } catch (e) {}
  function persist() { try { localStorage.setItem('evident-site-state', JSON.stringify({ q: state.q, sort: state.sort, sel: Object.fromEntries(Object.entries(state.sel).map(([k, v]) => [k, [...v]])) })); } catch (e) {} }

  function matches(c) {
    for (const f of FACETS) {
      const sel = state.sel[f.key];
      if (sel.size && !f.get(c).some(v => sel.has(v))) return false;
    }
    if (state.q) {
      const q = state.q.toLowerCase();
      const blob = [c.id, c.title, c.claim, c.subsystem, ...(c.oracles || []), ...(c.capabilities || [])].join(' ').toLowerCase();
      if (!blob.includes(q)) return false;
    }
    return true;
  }
  function filtered() { return CLAIMS.filter(matches); }

  function renderFacets() {
    const vis = filtered();
    let html = `<h3>Search</h3><input type="search" id="q" placeholder="id, title, oracle…" value="${esc(state.q)}">`;
    for (const f of FACETS) {
      const counts = new Map();
      for (const c of CLAIMS) for (const v of f.get(c)) counts.set(v, (counts.get(v) || 0) + 0);
      for (const c of vis) for (const v of f.get(c)) counts.set(v, (counts.get(v) || 0) + 1);
      let keys = [...counts.keys()];
      keys.sort((a, b) => f.order ? (f.order.indexOf(a) - f.order.indexOf(b)) : String(a).localeCompare(String(b)));
      if (!keys.length) continue;
      html += `<h3>${f.label}</h3>`;
      for (const k of keys) {
        const on = state.sel[f.key].has(k);
        html += `<label><input type="checkbox" data-facet="${esc(f.key)}" data-val="${esc(k)}" ${on ? 'checked' : ''}> <span>${esc(f.fmt ? f.fmt(k) : k)}</span><span class="n">${counts.get(k)}</span></label>`;
      }
    }
    html += `<button class="clear" id="clear">Clear filters</button>`;
    $('#facets').innerHTML = html;
    $('#q').addEventListener('input', e => { state.q = e.target.value; update(); });
    $('#facets').querySelectorAll('input[type=checkbox]').forEach(cb => cb.addEventListener('change', e => {
      const s = state.sel[e.target.dataset.facet]; e.target.checked ? s.add(e.target.dataset.val) : s.delete(e.target.dataset.val); update();
    }));
    $('#clear').addEventListener('click', () => { state.q = ''; for (const k in state.sel) state.sel[k].clear(); update(); });
  }

  // ---------- summary ----------
  function renderSummary(vis) {
    const n = vis.length;
    const st = k => vis.filter(c => c.status === k).length;
    const crit = vis.reduce((a, c) => { a.pass += c.criteria.pass; a.fail += c.criteria.fail; a.na += c.criteria.not_assessed; a.total += c.criteria.total; return a; }, { pass: 0, fail: 0, na: 0, total: 0 });
    const tiles = [
      [n, n === CLAIMS.length ? 'claims' : `of ${CLAIMS.length} claims`],
      [st('current'), 'current'], [st('contested'), 'contested'], [st('superseded'), 'superseded'],
      [`${crit.pass}/${crit.total}`, 'criteria passing'], [crit.fail, 'criteria failing'], [crit.na, 'not assessed'],
      [vis.filter(c => c.tier === 'release').length, 'release tier'],
    ];
    $('#summary').innerHTML = tiles.map(([v, l]) => `<div class="tile"><div class="num">${v}</div><div class="lbl">${l}</div></div>`).join('');
  }

  // ---------- table ----------
  const COLS = [
    { key: 'title', label: 'Claim', cell: c => `<td class="title">${esc(c.title)}<span class="id">${esc(c.id)}</span></td>`, val: c => c.title },
    { key: 'kind', label: 'Kind', cell: c => `<td><span class="pill kind">${esc(c.kind)}</span></td>`, val: c => c.kind },
    { key: 'tier', label: 'Tier', cell: c => `<td><span class="pill tier-${esc(c.tier)}">${esc(c.tier)}</span></td>`, val: c => TIERS.indexOf(c.tier) },
    { key: 'status', label: 'Status', cell: c => `<td><span class="pill status-${esc(c.status)}">${esc(STATUS_LABEL[c.status] || c.status)}</span></td>`, val: c => c.status },
    { key: 'criteria', label: 'Criteria', cell: c => `<td>${critCell(c)}</td>`, val: c => c.criteria.total ? c.criteria.pass / c.criteria.total : -1 },
    { key: 'subsystem', label: 'Subsystem', cell: c => `<td>${esc(c.subsystem || '')}</td>`, val: c => c.subsystem || '' },
    { key: 'strategy', label: 'Strategy', cell: c => `<td>${tags(c.trust_strategy)}</td>`, val: c => (c.trust_strategy || []).join() },
    { key: 'oracles', label: 'Oracle', cell: c => `<td>${tags(c.oracles)}</td>`, val: c => (c.oracles || []).join() },
    { key: 'capabilities', label: 'Capability', cell: c => `<td>${tags(c.capabilities)}</td>`, val: c => (c.capabilities || []).join() },
    { key: 'verified', label: 'Last verified', cell: c => `<td>${esc((c.last_verified && c.last_verified.date) || '—')}</td>`, val: c => (c.last_verified && c.last_verified.date) || '' },
  ];
  function tags(a) { return a && a.length ? `<div class="tags">${a.map(x => `<span>${esc(x)}</span>`).join('')}</div>` : ''; }
  function critCell(c) {
    const k = c.criteria; if (!k.total) return `<span class="crit n">${c.kind === 'measurement' ? '—' : 'n/a'}</span>`;
    const w = x => (100 * x / k.total).toFixed(1) + '%';
    return `<span class="bar"><i class="p" style="width:${w(k.pass)}"></i><i class="f" style="width:${w(k.fail)}"></i><i class="n" style="width:${w(k.not_assessed + k.partial + k.other)}"></i></span><span class="crit"><span class="p">${k.pass}✓</span> <span class="f">${k.fail}✗</span> <span class="n">${k.not_assessed}?</span></span>`;
  }
  function renderTable(vis) {
    const s = state.sort;
    const rows = [...vis].sort((a, b) => { const col = COLS.find(c => c.key === s.key); const va = col.val(a), vb = col.val(b); const r = va < vb ? -1 : va > vb ? 1 : a.id.localeCompare(b.id); return s.asc ? r : -r; });
    if (!rows.length) { $('#view-table').innerHTML = '<div class="empty">No claims match the current filters.</div>'; return; }
    $('#view-table').innerHTML = `<div style="overflow-x:auto"><table class="claims"><thead><tr>${COLS.map(c => `<th data-key="${c.key}" class="${s.key === c.key ? 'sorted' + (s.asc ? ' asc' : '') : ''}">${c.label}</th>`).join('')}</tr></thead><tbody>${rows.map(c => `<tr class="row" data-id="${esc(c.id)}">${COLS.map(col => col.cell(c)).join('')}</tr>`).join('')}</tbody></table></div>`;
    $('#view-table').querySelectorAll('th').forEach(th => th.addEventListener('click', () => { const k = th.dataset.key; if (state.sort.key === k) state.sort.asc = !state.sort.asc; else state.sort = { key: k, asc: true }; update(); }));
    $('#view-table').querySelectorAll('tr.row').forEach(tr => tr.addEventListener('click', () => openDetail(tr.dataset.id)));
  }

  // ---------- matrix ----------
  function renderMatrix(vis) {
    const rowsKey = c => c.subsystem || (c.kind !== 'measurement' ? `(${c.kind})` : '(no subsystem)');
    const subs = [...new Set(vis.map(rowsKey))].sort();
    const cell = (sub, tier) => vis.filter(c => rowsKey(c) === sub && c.tier === tier);
    const summ = cs => { if (!cs.length) return ''; const p = cs.reduce((a, c) => a + c.criteria.pass, 0), t = cs.reduce((a, c) => a + c.criteria.total, 0); const ct = cs.filter(c => c.status === 'contested').length; return `<small>${t ? `${p}/${t} ✓` : ''}${ct ? ` · ${ct} contested` : ''}</small>`; };
    let html = `<p class="matrix-note">Claims per subsystem and tier for the current filter. Click a cell to filter the table to it. Empty release-tier cells are where a subsystem has no release-grade evidence.</p>`;
    html += `<table class="matrix"><thead><tr><th class="rowh">Subsystem</th>${TIERS.map(t => `<th>${t}</th>`).join('')}<th>total</th></tr></thead><tbody>`;
    for (const s of subs) {
      html += `<tr><th class="rowh">${esc(s)}</th>`;
      for (const t of TIERS) { const cs = cell(s, t); html += `<td class="cell ${cs.length ? '' : 'c0'}" data-sub="${esc(s)}" data-tier="${t}">${cs.length || '·'}${summ(cs)}</td>`; }
      html += `<td>${vis.filter(c => rowsKey(c) === s).length}</td></tr>`;
    }
    html += `<tr><th class="rowh">total</th>${TIERS.map(t => `<td>${vis.filter(c => c.tier === t).length}</td>`).join('')}<td>${vis.length}</td></tr></tbody></table>`;
    $('#view-matrix').innerHTML = html;
    $('#view-matrix').querySelectorAll('td.cell').forEach(td => td.addEventListener('click', () => {
      const sub = td.dataset.sub, tier = td.dataset.tier;
      state.sel.tier = new Set([tier]);
      state.sel.subsystem = sub.startsWith('(') ? new Set() : new Set([sub]);
      setView('table'); update();
    }));
  }

  // ---------- graph ----------
  let cy = null;
  const STATUS_COLOR = { current: '#28a745', contested: '#ffc107', superseded: '#adb5bd', not_synthesized: '#adb5bd', error: '#dc3545' };
  function renderGraph(vis) {
    const el = $('#graph');
    if (typeof cytoscape !== 'function') { el.innerHTML = '<div class="empty">Graph view needs Cytoscape (loaded from cdnjs). It could not be loaded — the table and coverage views are unaffected.</div>'; return; }
    const showCaps = $('#g-caps').checked, showSubs = $('#g-subs').checked;
    const nodes = [], edges = [], seen = new Set();
    const add = (id, data) => { if (!seen.has(id)) { seen.add(id); nodes.push({ data: { id, ...data } }); } };
    for (const c of vis) {
      add('c:' + c.id, { label: c.title.length > 60 ? c.title.slice(0, 57) + '…' : c.title, type: 'claim', color: STATUS_COLOR[c.status] || '#adb5bd', tier: c.tier, cid: c.id, border: c.tier === 'release' ? 4 : c.tier === 'ci' ? 2 : 1 });
      for (const o of c.oracles || []) { add('o:' + o, { label: o, type: 'oracle' }); edges.push({ data: { id: `e${edges.length}`, source: 'c:' + c.id, target: 'o:' + o, type: 'oracle' } }); }
      if (showCaps) for (const k of c.capabilities || []) { add('k:' + k, { label: k, type: 'capability' }); edges.push({ data: { id: `e${edges.length}`, source: 'c:' + c.id, target: 'k:' + k, type: 'capability' } }); }
      if (showSubs && c.subsystem) { add('s:' + c.subsystem, { label: c.subsystem, type: 'subsystem' }); edges.push({ data: { id: `e${edges.length}`, source: 'c:' + c.id, target: 's:' + c.subsystem, type: 'subsystem' } }); }
      const rep = DATA.reports[c.id];
      if (rep && rep._graph && Array.isArray(rep._graph.review_events)) for (const ev of rep._graph.review_events) {
        const bk = ev && ev.kind && ev.kind.data && ev.kind.data.backed_by;
        if (bk && byId[bk]) { add('c:' + bk, { label: byId[bk].title, type: 'claim', color: STATUS_COLOR[byId[bk].status] || '#adb5bd', tier: byId[bk].tier, cid: bk, border: 2 }); edges.push({ data: { id: `e${edges.length}`, source: 'c:' + bk, target: 'c:' + c.id, type: 'challenge' } }); }
      }
    }
    if (cy) { cy.destroy(); cy = null; }
    el.innerHTML = '';
    if (!nodes.length) { el.innerHTML = '<div class="empty">No claims match the current filters.</div>'; return; }
    cy = cytoscape({
      container: el, elements: { nodes, edges }, wheelSensitivity: 0.2,
      style: [
        { selector: 'node', style: { 'label': 'data(label)', 'font-size': 9, 'text-wrap': 'wrap', 'text-max-width': 140, 'text-valign': 'bottom', 'text-margin-y': 4, 'color': '#2c3e50', 'width': 22, 'height': 22 } },
        { selector: 'node[type="claim"]', style: { 'background-color': 'data(color)', 'border-width': 'data(border)', 'border-color': '#2c3e50', 'shape': 'ellipse' } },
        { selector: 'node[type="oracle"]', style: { 'background-color': '#0b4f9c', 'shape': 'diamond', 'width': 26, 'height': 26, 'font-weight': 'bold' } },
        { selector: 'node[type="capability"]', style: { 'background-color': '#5b2a86', 'shape': 'round-rectangle', 'width': 26, 'height': 18 } },
        { selector: 'node[type="subsystem"]', style: { 'background-color': '#e67e22', 'shape': 'round-rectangle', 'width': 26, 'height': 18 } },
        { selector: 'edge', style: { 'width': 1, 'line-color': '#c8d0d8', 'curve-style': 'bezier' } },
        { selector: 'edge[type="capability"]', style: { 'line-color': '#d9c7ec', 'line-style': 'dashed' } },
        { selector: 'edge[type="subsystem"]', style: { 'line-color': '#f5d3b3', 'line-style': 'dotted' } },
        { selector: 'edge[type="challenge"]', style: { 'line-color': '#dc3545', 'width': 2, 'target-arrow-shape': 'triangle', 'target-arrow-color': '#dc3545' } },
        { selector: 'node:selected', style: { 'border-width': 4, 'border-color': '#000' } },
      ],
      layout: { name: 'cose', animate: false, nodeRepulsion: 9000, idealEdgeLength: 90, padding: 20 },
    });
    cy.on('tap', 'node[type="claim"]', e => openDetail(e.target.data('cid')));
    cy.on('tap', 'node[type="oracle"]', e => { state.sel.oracles = new Set([e.target.data('label')]); setView('table'); update(); });
    cy.on('tap', 'node[type="capability"]', e => { state.sel.capabilities = new Set([e.target.data('label')]); setView('table'); update(); });
    cy.on('tap', 'node[type="subsystem"]', e => { state.sel.subsystem = new Set([e.target.data('label')]); setView('table'); update(); });
  }
  $('#g-caps').addEventListener('change', () => renderGraph(filtered()));
  $('#g-subs').addEventListener('change', () => renderGraph(filtered()));

  // ---------- detail drawer ----------
  function openDetail(id) {
    const c = byId[id]; if (!c) return;
    $('#drawer-title').textContent = c.id;
    const rel = (label, vals, facet) => vals && vals.length ? `<dt>${label}</dt><dd class="rel">${vals.map(v => `<a data-facet="${facet}" data-val="${esc(v)}">${esc(v)}</a>`).join(', ')}</dd>` : '';
    const lv = c.last_verified || {};
    let html = `<h1>${esc(c.title)}</h1>
      <p><span class="pill kind">${esc(c.kind)}</span> <span class="pill tier-${esc(c.tier)}">${esc(c.tier)}</span> <span class="pill status-${esc(c.status)}">${esc(STATUS_LABEL[c.status] || c.status)}</span></p>
      <div class="claimtext">${esc(c.claim || '')}</div>
      <dl class="kv">
        ${rel('Trust strategy', c.trust_strategy, 'trust_strategy')}
        ${c.subsystem ? rel('Subsystem', [c.subsystem], 'subsystem') : ''}
        ${rel('Oracles', c.oracles, 'oracles')}
        ${rel('Capabilities', c.capabilities, 'capabilities')}
        ${c.provenance ? `<dt>Provenance</dt><dd>${esc(c.provenance)}${c.review_status ? ` · ${esc(c.review_status)}` : ''}</dd>` : ''}
        ${c.command ? `<dt>Command</dt><dd><code>${esc(c.command)}</code></dd>` : ''}
        ${c.case ? `<dt>Case</dt><dd><code>${esc(c.case)}</code></dd>` : ''}
        ${c.pattern ? `<dt>Pattern</dt><dd><code>${esc(c.pattern)}</code></dd>` : ''}
        <dt>Last verified</dt><dd>${lv.date ? `${esc(lv.date)}${lv.commit ? ` @ <code>${esc(lv.commit)}</code>` : ''}${lv.value != null ? ` · value ${esc(lv.value)}` : ''}` : '<em>never</em>'}</dd>
        <dt>Assumptions / failure modes</dt><dd>${c.n_assumptions} / ${c.n_failure_modes} recorded</dd>
        <dt>Source</dt><dd><code>${esc(c.source_path)}</code></dd>
      </dl>`;
    if (DATA.fragments[c.id]) html += `<hr>${DATA.fragments[c.id]}`;
    else if (c.skip_reason) html += `<p class="meta"><em>No trust report:</em> ${esc(c.skip_reason)}</p>`;
    const body = $('#drawer-body'); body.innerHTML = html; body.scrollTop = 0;
    body.querySelectorAll('.rel a').forEach(a => a.addEventListener('click', () => { state.sel[a.dataset.facet] = new Set([a.dataset.val]); closeDetail(); setView('table'); update(); }));
    $('#drawer').classList.add('open'); $('#drawer').setAttribute('aria-hidden', 'false'); $('#backdrop').classList.add('open');
    if (window.__mermaid) { const nodes = body.querySelectorAll('.mermaid'); if (nodes.length) window.__mermaid.run({ nodes }).catch(() => {}); }
    history.replaceState(null, '', '#' + encodeURIComponent(c.id));
  }
  function closeDetail() { $('#drawer').classList.remove('open'); $('#drawer').setAttribute('aria-hidden', 'true'); $('#backdrop').classList.remove('open'); history.replaceState(null, '', location.pathname + location.search); }
  $('#drawer-close').addEventListener('click', closeDetail);
  $('#backdrop').addEventListener('click', closeDetail);
  document.addEventListener('keydown', e => { if (e.key === 'Escape') closeDetail(); });

  // ---------- views ----------
  function setView(v) { state.view = v; document.querySelectorAll('nav.tabs button').forEach(b => b.setAttribute('aria-selected', String(b.dataset.view === v))); for (const k of ['table', 'matrix', 'graph']) $('#view-' + k).hidden = k !== v; }
  document.querySelectorAll('nav.tabs button').forEach(b => b.addEventListener('click', () => { setView(b.dataset.view); update(); }));

  function update() {
    persist();
    const vis = filtered();
    const active = document.activeElement && document.activeElement.id === 'q';
    const pos = active ? document.activeElement.selectionStart : null;
    renderFacets(); renderSummary(vis);
    if (active) { const q = $('#q'); q.focus(); q.setSelectionRange(pos, pos); }
    if (state.view === 'table') renderTable(vis);
    else if (state.view === 'matrix') renderMatrix(vis);
    else renderGraph(vis);
  }
  setView('table'); update();
  if (location.hash.length > 1) { const id = decodeURIComponent(location.hash.slice(1)); if (byId[id]) openDetail(id); }
})();
"#;
