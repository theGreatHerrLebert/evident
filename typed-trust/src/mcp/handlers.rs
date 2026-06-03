//! Phase 3 MCP tool handlers (sync).
//!
//! Each handler is a pure function over `ServerState` + a JSON
//! argument value. The async layer (`super::run`, `handle_tool_call`)
//! drives them via `spawn_blocking`.
//!
//! All handlers return `Result<Value, ToolError>`. `ToolError`
//! carries a tier discriminator so the async boundary can route
//! Protocol tier into a JSON-RPC error and Data tier into a tool
//! result with `isError: true`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::{json, Value};

use crate::ids::ClaimId;
use crate::loader::{load_claims_with_policy, AllowListPathPolicy, LoaderError, PathPolicy};
use crate::render::{render_augmented, supersede_projection, RenderInput};
use crate::review::ReviewEvent;
use crate::synthesize::synthesize;
use crate::translate::{
    backing_claim_for_event, translate_claim, translate_evidence, translate_review_event,
    translate_tolerances, ManifestClaim, ManifestReviewEvent, ReviewEventSidecar,
    TranslationContext,
};

/// State shared across all handlers. Cheap to clone (everything
/// behind Arc-equivalents); each `spawn_blocking` worker borrows
/// the same instance.
pub struct ServerState {
    pub policy: AllowListPathPolicy,
}

impl ServerState {
    pub fn new(policy: AllowListPathPolicy) -> Self {
        Self { policy }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ToolErrorTier {
    /// JSON-RPC protocol error: invalid input, server
    /// misconfiguration, unauthorized path.
    Protocol,
    /// Tool result with `isError: true`: corpus data error.
    Data,
}

#[derive(Debug, Clone)]
pub struct ToolError {
    pub tier: ToolErrorTier,
    pub code: i64,
    pub message: String,
}

impl ToolError {
    pub fn protocol(code: i64, message: impl Into<String>) -> Self {
        Self {
            tier: ToolErrorTier::Protocol,
            code,
            message: message.into(),
        }
    }
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::protocol(-32602, message)
    }
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::protocol(-32001, message)
    }
    pub fn data(message: impl Into<String>) -> Self {
        Self {
            tier: ToolErrorTier::Data,
            code: 0,
            message: message.into(),
        }
    }
}

impl From<LoaderError> for ToolError {
    fn from(err: LoaderError) -> Self {
        match err {
            LoaderError::PolicyDenied { .. } => {
                // The root-vs-include discrimination happens at the
                // call site: handlers pre-check the root manifest
                // path (tier 1), so a PolicyDenied that reaches
                // here arose during include resolution and is a
                // tier-2 data error.
                ToolError::data(err.to_string())
            }
            _ => ToolError::data(err.to_string()),
        }
    }
}

/// Tool dispatcher. The async layer calls this inside
/// `spawn_blocking`.
pub fn dispatch_sync(
    state: &ServerState,
    tool: &str,
    arguments: Value,
) -> Result<Value, ToolError> {
    match tool {
        "list_claims" => list_claims(state, arguments),
        "read_report" => read_report(state, arguments),
        "list_review_events" => list_review_events(state, arguments),
        "query_claims" => query_claims(state, arguments),
        "get_panel_summary" => get_panel_summary(state, arguments),
        "get_superseded_events" => get_superseded_events(state, arguments),
        "walk_backing_chain" => walk_backing_chain(state, arguments),
        "render_report" => render_report(state, arguments),
        "query_metadata" => query_metadata(state, arguments),
        "query_concordance" => query_concordance(state, arguments),
        other => Err(ToolError::protocol(
            -32601,
            format!("Unknown tool: {other}"),
        )),
    }
}

// ============================================================
// Pre-flight: gate the root manifest path through the policy and
// produce a clear tier-1 error if rejected. Tier-2 errors only
// arise during the SUBSEQUENT include resolution (handled inside
// load_claims_with_policy).
// ============================================================
fn authorize_manifest(state: &ServerState, manifest_path: &str) -> Result<(), ToolError> {
    state
        .policy
        .check(Path::new(manifest_path))
        .map(|_| ())
        .map_err(|denied| ToolError::unauthorized(denied.reason))
}

/// Validate + canonicalize a sidecar path argument. Implicit
/// acceptance: the sidecar's canonical parent equals one of the
/// allowed roots' canonical parent (i.e., the sidecar sits in or
/// under an allowed directory). The allow-list policy's
/// `check` already implements that rule, so we just call it.
fn authorize_sidecar(
    state: &ServerState,
    path: &str,
    kind_label: &str,
) -> Result<std::path::PathBuf, ToolError> {
    state
        .policy
        .check(Path::new(path))
        .map_err(|denied| ToolError::unauthorized(format!("{kind_label}: {}", denied.reason)))
}

fn arg_str(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| ToolError::invalid_params(format!("missing required field: {key}")))
}

fn arg_str_opt(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn arg_bool_opt(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}

fn arg_usize_opt(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| v.as_u64()).map(|n| n as usize)
}

// ============================================================
// Tool implementations
// ============================================================

fn list_claims(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    authorize_manifest(state, &manifest_path)?;
    let limit = arg_usize_opt(&args, "limit");
    let cursor = arg_str_opt(&args, "cursor")
        .map(|c| c.parse::<usize>().unwrap_or(0))
        .unwrap_or(0);

    let claims = load_claims_with_policy(&manifest_path, &state.policy)?;
    let total = claims.len();
    let start = cursor.min(total);
    let end = match limit {
        Some(l) => (start + l).min(total),
        None => total,
    };
    let slice = &claims[start..end];
    let items: Vec<Value> = slice
        .iter()
        .map(|c| {
            let mut item = json!({
                "claim_id": c.claim.id,
                "title": c.claim.title,
                "tier": c.claim.tier,
                "kind": c.claim.kind,
                // Phase 5: surface replay_status so consumers can
                // filter on "extracted but no replay path." Absent
                // means default (not_attempted); we project that
                // explicitly so consumers don't have to special-case.
                "replay_status": c.claim.evidence.as_ref()
                    .and_then(|e| e.replay_status.clone())
                    .unwrap_or_else(|| "not_attempted".into()),
            });
            if let Some(reason) = c.claim.evidence.as_ref()
                .and_then(|e| e.replay_reason.clone())
            {
                item.as_object_mut().unwrap().insert(
                    "replay_reason".into(),
                    json!(reason),
                );
            }
            // Phase 5 PR2: surface provenance_kind (always) and
            // source_context (when set). Consumers querying
            // "show me extracted claims whose README copies marketing
            // text" need source_context at the summary layer.
            if let Some(prov) = c.claim.provenance.as_ref() {
                item.as_object_mut().unwrap().insert(
                    "provenance_kind".into(),
                    json!(prov.effective_kind()),
                );
                // PR5c (codex F-PR5c-CR3): surface source_id so the
                // query_metadata tool description ("same audit
                // context as list_claims") is accurate. Audit
                // workflows need the extracted source id at the
                // summary tier.
                if let Some(sid) = prov.source_id() {
                    item.as_object_mut().unwrap().insert(
                        "source_id".into(),
                        json!(sid),
                    );
                }
                if let Some(sc) = prov.source_context() {
                    item.as_object_mut().unwrap().insert(
                        "source_context".into(),
                        json!(sc),
                    );
                }
            }
            // PR5c: surface the metadata block on metadata claims so
            // consumers using list_claims can spot the declaration
            // without an extra query_metadata call. Codex F-PR5c-CR2
            // (P2): gate on kind, not raw block presence — keeps the
            // measurement/metadata disjointness invariant visible at
            // this projection layer. A measurement claim that
            // accidentally carries `metadata:` is a translator-
            // rejected manifest bug; list_claims must not expose it
            // as a normal metadata claim.
            if c.claim.kind == "metadata_compatibility" {
                if let Some(md) = c.claim.metadata.as_ref() {
                    item.as_object_mut().unwrap().insert(
                        "metadata".into(),
                        json!({
                            "field": md.field,
                            "declared_value": md.declared_value,
                            "source_file": md.source_file,
                            "source_path": md.source_path,
                        }),
                    );
                }
            }
            item
        })
        .collect();
    let truncated = end < total;
    let next_cursor = if truncated {
        Some(end.to_string())
    } else {
        None
    };

    Ok(json!({
        "items": items,
        "truncated": truncated,
        "cursor": next_cursor,
        "total": total,
    }))
}

fn read_report(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    let claim_id = arg_str(&args, "claim_id")?;
    authorize_manifest(state, &manifest_path)?;
    let sidecar_path = arg_str_opt(&args, "sidecar");
    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");

    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
    Ok(json!({"report": report}))
}

fn list_review_events(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    let sidecar_path = arg_str(&args, "sidecar")?;
    authorize_manifest(state, &manifest_path)?;
    let _ = authorize_sidecar(state, &sidecar_path, "sidecar")?;

    let claim_filter = arg_str_opt(&args, "claim_id");
    let kind_filter = arg_str_opt(&args, "kind");
    let author_filter = arg_str_opt(&args, "author");
    let event_id_filter = arg_str_opt(&args, "event_id");
    let include_rationale = arg_bool_opt(&args, "include_rationale")
        .unwrap_or_else(|| claim_filter.is_some());
    let limit = arg_usize_opt(&args, "limit").unwrap_or(usize::MAX);
    let cursor = arg_str_opt(&args, "cursor")
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0);

    let parsed = parse_sidecar(&sidecar_path)?;
    let mut filtered: Vec<&ManifestReviewEvent> = parsed
        .events
        .iter()
        .filter(|e| {
            claim_filter
                .as_ref()
                .map(|f| &e.claim_id == f)
                .unwrap_or(true)
        })
        .filter(|e| kind_filter.as_ref().map(|k| &e.kind == k).unwrap_or(true))
        .filter(|e| {
            author_filter
                .as_ref()
                .map(|a| &e.author.name == a)
                .unwrap_or(true)
        })
        .collect();
    // Filter by event_id requires translating each entry, since the
    // canonical id might be the synthesized hash. Do it here only
    // if needed (avoids translating every event for non-id filters).
    if let Some(ref target) = event_id_filter {
        filtered.retain(|e| match translate_review_event(e) {
            Ok(ev) => ev.id.as_str() == target,
            Err(_) => false,
        });
    }
    let total = filtered.len();
    let start = cursor.min(total);
    let end = (start + limit).min(total);
    let mut items: Vec<Value> = Vec::with_capacity(end - start);
    for e in &filtered[start..end] {
        let event = translate_review_event(e).map_err(|err| ToolError::data(err.to_string()))?;
        let mut row = json!({
            "event_id": event.id.as_str(),
            "claim_id": e.claim_id,
            "kind": e.kind,
            "author": {
                "kind": e.author.kind,
                "name": e.author.name,
                "version": e.author.version,
            },
            "timestamp": e.timestamp,
        });
        if include_rationale {
            row.as_object_mut()
                .unwrap()
                .insert("rationale".into(), Value::String(e.rationale.clone()));
        }
        items.push(row);
    }
    let truncated = end < total;
    Ok(json!({
        "items": items,
        "truncated": truncated,
        "cursor": if truncated { Some(end.to_string()) } else { None },
        "total": total,
    }))
}

fn query_claims(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    authorize_manifest(state, &manifest_path)?;
    let sidecar_path = arg_str_opt(&args, "sidecar");
    let status_filter = arg_str_opt(&args, "status");
    let reviewer_filter = arg_str_opt(&args, "reviewer");
    let event_kind_filter = arg_str_opt(&args, "event_kind");
    let has_panel_filter = arg_bool_opt(&args, "has_panel_summary");
    let has_superseded_filter = arg_bool_opt(&args, "has_superseded");
    let limit = arg_usize_opt(&args, "limit").unwrap_or(usize::MAX);
    let cursor = arg_str_opt(&args, "cursor")
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0);

    if sidecar_path.is_some() {
        authorize_sidecar(state, sidecar_path.as_deref().unwrap(), "sidecar")?;
    }

    let claims = load_claims_with_policy(&manifest_path, &state.policy)?;
    let mut matches: Vec<Value> = Vec::new();
    let mut examined = 0usize;
    for (idx, c) in claims.iter().enumerate() {
        if idx < cursor {
            continue;
        }
        examined += 1;
        let report = synthesize_for(state, &manifest_path, &c.claim.id, sidecar_path.as_deref(), None);
        let Ok(report) = report else { continue };

        if let Some(ref status) = status_filter {
            if report["status"].as_str() != Some(status.as_str()) {
                continue;
            }
        }
        if let Some(ref reviewer) = reviewer_filter {
            let any = report["_graph"]["panel_summary"]["verdicts_by_reviewer"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .any(|r| r["author"]["name"].as_str() == Some(reviewer.as_str()))
                })
                .unwrap_or(false);
            if !any {
                continue;
            }
        }
        if let Some(ref ek) = event_kind_filter {
            let count_field = match ek.as_str() {
                "endorse" => "n_endorse",
                "dissent" => "n_dissent",
                "challenge" => "n_challenge",
                "supersede" => "n_supersede_raw",
                _ => return Err(ToolError::invalid_params("invalid event_kind enum")),
            };
            let n = report["_graph"]["panel_summary"][count_field].as_u64().unwrap_or(0);
            if n == 0 {
                continue;
            }
        }
        if let Some(want_panel) = has_panel_filter {
            let has = report["_graph"]["panel_summary"]["n_reviewers"].as_u64().unwrap_or(0) > 1;
            if has != want_panel {
                continue;
            }
        }
        if let Some(want_superseded) = has_superseded_filter {
            let n = report["_graph"]["panel_summary"]["n_supersede_raw"].as_u64().unwrap_or(0);
            let has = n > 0;
            if has != want_superseded {
                continue;
            }
        }
        matches.push(json!({
            "claim_id": c.claim.id,
            "status": report["status"],
        }));
        if matches.len() >= limit {
            break;
        }
    }
    let truncated = examined < claims.len() - cursor && matches.len() >= limit;
    let next_cursor = if truncated {
        Some((cursor + examined).to_string())
    } else {
        None
    };
    Ok(json!({
        "items": matches,
        "truncated": truncated,
        "cursor": next_cursor,
    }))
}

fn get_panel_summary(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    let claim_id = arg_str(&args, "claim_id")?;
    let sidecar_path = arg_str(&args, "sidecar")?;
    authorize_manifest(state, &manifest_path)?;
    authorize_sidecar(state, &sidecar_path, "sidecar")?;
    let report = synthesize_for(state, &manifest_path, &claim_id, Some(&sidecar_path), None)?;
    Ok(report["_graph"]["panel_summary"].clone())
}

fn get_superseded_events(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    let claim_id = arg_str(&args, "claim_id")?;
    let sidecar_path = arg_str(&args, "sidecar")?;
    authorize_manifest(state, &manifest_path)?;
    authorize_sidecar(state, &sidecar_path, "sidecar")?;
    let report = synthesize_for(state, &manifest_path, &claim_id, Some(&sidecar_path), None)?;
    let panel = &report["_graph"]["panel_summary"];
    Ok(json!({
        "superseded_pairs": panel["superseded_pairs"].clone(),
        "unresolved_supersedes": panel["unresolved_supersedes"].clone(),
        "invalid_supersedes": panel["invalid_supersedes"].clone(),
    }))
}

fn walk_backing_chain(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    let claim_id = arg_str(&args, "claim_id")?;
    let sidecar_path = arg_str(&args, "sidecar")?;
    authorize_manifest(state, &manifest_path)?;
    authorize_sidecar(state, &sidecar_path, "sidecar")?;
    let event_filter = arg_str_opt(&args, "event_id");
    let max_depth = arg_usize_opt(&args, "max_depth").unwrap_or(4);

    let report = synthesize_for(state, &manifest_path, &claim_id, Some(&sidecar_path), None)?;
    let challenges: Vec<Value> = report["_graph"]["review_events"]
        .as_array()
        .map(|events| {
            events
                .iter()
                .filter(|e| e["kind"]["type"].as_str() == Some("challenge"))
                .filter(|e| match &event_filter {
                    Some(f) => e["id"].as_str() == Some(f.as_str()),
                    None => true,
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({
        "claim_id": claim_id,
        "status": report["status"],
        "challenges": challenges,
        "max_depth": max_depth,
        "note": "Phase 3-i returns one level. Future phases recurse via further read_report calls.",
    }))
}

/// PR5c: structured query path for metadata_compatibility claims.
///
/// Walks the manifest, filters to `kind == metadata_compatibility`,
/// applies optional `field` / `source_file` filters (exact,
/// case-sensitive), and returns the audit-context summary plus the
/// four metadata fields per match. Self-contained: a caller can
/// route the result without a follow-up list_claims call.
fn query_metadata(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    authorize_manifest(state, &manifest_path)?;
    let field_filter = arg_str_opt(&args, "field");
    let source_file_filter = arg_str_opt(&args, "source_file");

    let claims = load_claims_with_policy(&manifest_path, &state.policy)?;
    // Codex F-PR5c-CR1 (P1): a `kind: metadata_compatibility` claim
    // without a `metadata:` block is a translator-rejected manifest
    // bug. Other MCP report-producing paths surface that as a
    // tier-2 data error; query_metadata previously silently dropped
    // such claims and made a broken manifest look like "no metadata
    // claims." Validate up front so the caller sees the bug.
    for c in claims.iter() {
        if c.claim.kind == "metadata_compatibility" && c.claim.metadata.is_none() {
            return Err(ToolError::data(format!(
                "claim {}: kind=metadata_compatibility requires a metadata block",
                c.claim.id
            )));
        }
    }
    let items: Vec<Value> = claims
        .iter()
        .filter(|c| c.claim.kind == "metadata_compatibility")
        .filter(|c| c.claim.metadata.is_some())
        .filter(|c| {
            let md = c.claim.metadata.as_ref().unwrap();
            field_filter
                .as_ref()
                .map_or(true, |f| md.field == *f)
        })
        .filter(|c| {
            let md = c.claim.metadata.as_ref().unwrap();
            source_file_filter
                .as_ref()
                .map_or(true, |f| md.source_file == *f)
        })
        .map(|c| {
            let md = c.claim.metadata.as_ref().unwrap();
            let mut item = json!({
                "claim_id": c.claim.id,
                "title": c.claim.title,
                "tier": c.claim.tier,
                "field": md.field,
                "declared_value": md.declared_value,
                "source_file": md.source_file,
                "source_path": md.source_path,
            });
            // Mirror list_claims' audit context so a metadata query
            // result is self-contained.
            if let Some(prov) = c.claim.provenance.as_ref() {
                item.as_object_mut().unwrap().insert(
                    "provenance_kind".into(),
                    json!(prov.effective_kind()),
                );
                if let Some(sid) = prov.source_id() {
                    item.as_object_mut().unwrap().insert(
                        "source_id".into(),
                        json!(sid),
                    );
                }
                if let Some(sc) = prov.source_context() {
                    item.as_object_mut().unwrap().insert(
                        "source_context".into(),
                        json!(sc),
                    );
                }
            }
            item
        })
        .collect();
    Ok(json!({"items": items}))
}

fn render_report(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    let claim_id = arg_str(&args, "claim_id")?;
    let format = arg_str(&args, "format")?;
    if !matches!(format.as_str(), "markdown" | "html" | "mermaid") {
        return Err(ToolError::invalid_params(format!(
            "format must be markdown|html|mermaid, got {format:?}"
        )));
    }
    authorize_manifest(state, &manifest_path)?;
    let sidecar_path = arg_str_opt(&args, "sidecar");
    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;

    let content = match format.as_str() {
        "markdown" => crate::human_render::render_markdown(&report),
        "html" => crate::html_render::render_html(&report),
        "mermaid" => crate::graph::render_mermaid_graph(&report),
        _ => unreachable!(),
    };
    Ok(json!({
        "format": format,
        "content": content,
        "truncated": false,
    }))
}

// ============================================================
// Shared: full synthesize pipeline for one claim
// ============================================================
fn synthesize_for(
    state: &ServerState,
    manifest_path: &str,
    claim_id: &str,
    sidecar_path: Option<&str>,
    _last_verified_path: Option<&str>,
) -> Result<Value, ToolError> {
    let claims = load_claims_with_policy(manifest_path, &state.policy)?;
    let now = "1970-01-01T00:00:00Z".to_string();

    // Find the target claim.
    let target = claims
        .iter()
        .find(|c| c.claim.id == claim_id)
        .ok_or_else(|| ToolError::data(format!("claim_id {claim_id:?} not in manifest")))?;

    let ctx = TranslationContext {
        now: now.clone(),
        manifest_path: target.source_path.clone(),
    };
    let typed_claim = translate_claim(&ctx, &target.claim, &target.span)
        .map_err(|e| ToolError::data(e.to_string()))?
        .value;
    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
    let evidence = translate_evidence(&ctx, &target.claim, &criteria)
        .map_err(|e| ToolError::data(e.to_string()))?
        .into_iter()
        .collect::<Vec<_>>();

    // Load sidecar events (if any) and Phase 2b backing claims.
    let (events_by_claim, backing_claims_by_target, raw_by_claim) = match sidecar_path {
        Some(p) => {
            authorize_sidecar(state, p, "sidecar")?;
            let parsed = parse_sidecar(p)?;
            // Reject duplicate event_ids (Phase 2d F-2D-12 parity).
            let mut seen_ids: HashSet<String> = HashSet::new();
            let mut grouped: HashMap<String, Vec<ReviewEvent>> = HashMap::new();
            let mut backing: HashMap<String, Vec<ManifestClaim>> = HashMap::new();
            let mut raw: HashMap<
                String,
                Vec<crate::translate::ManifestReviewEvent>,
            > = HashMap::new();
            for entry in &parsed.events {
                let ev = translate_review_event(entry).map_err(|e| ToolError::data(e.to_string()))?;
                if !seen_ids.insert(ev.id.as_str().to_string()) {
                    return Err(ToolError::data(format!(
                        "duplicate event_id {:?} in sidecar",
                        ev.id.as_str()
                    )));
                }
                grouped.entry(entry.claim_id.clone()).or_default().push(ev);
                raw.entry(entry.claim_id.clone())
                    .or_default()
                    .push(entry.clone());
                if let Some(bc) = backing_claim_for_event(entry) {
                    backing.entry(entry.claim_id.clone()).or_default().push(bc.clone());
                }
            }
            (grouped, backing, raw)
        }
        None => (HashMap::new(), HashMap::new(), HashMap::new()),
    };

    // Phase 5 PR3: enforce the promotion gate. An extracted claim
    // (provenance.kind = extracted-from-paper | extracted-from-repo)
    // at tier > research must have a matching PromoteFromExtracted
    // event in the sidecar for THIS specific claim. validator returns
    // Ok for non-extracted and research-tier extracted claims.
    let empty_raw: Vec<crate::translate::ManifestReviewEvent> = vec![];
    let events_for_claim = raw_by_claim.get(claim_id).unwrap_or(&empty_raw);
    crate::translate::validate_promotion_rules(&target.claim, events_for_claim)
        .map_err(|e| ToolError::data(e.to_string()))?;

    let events: &[ReviewEvent] = events_by_claim
        .get(claim_id)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    let backing_reports: Vec<crate::report::TrustReport> = backing_claims_by_target
        .get(claim_id)
        .map(|bcs| {
            let mut out = Vec::new();
            for bc in bcs {
                let bc_ctx = TranslationContext {
                    now: now.clone(),
                    manifest_path: ctx.manifest_path.clone(),
                };
                let span = "(mcp_backing)".to_string();
                if let Err(_) = translate_claim(&bc_ctx, bc, &span) {
                    continue;
                }
                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
                let bc_evidence: Vec<_> = match translate_evidence(&bc_ctx, bc, &bc_criteria) {
                    Ok(opt) => opt.into_iter().collect(),
                    Err(_) => continue,
                };
                let bc_report = synthesize(
                    ClaimId::new(&bc.id),
                    bc_criteria,
                    &bc_evidence,
                    &[],
                    &[],
                    &std::collections::HashSet::new(),
                    now.clone(),
                );
                out.push(bc_report);
            }
            out
        })
        .unwrap_or_default();

    let report = synthesize(
        ClaimId::new(claim_id),
        criteria,
        &evidence,
        events,
        &backing_reports,
        &HashSet::new(),
        now,
    );

    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &evidence,
        related_events: events,
        backing_reports: &backing_reports,
        cycle_contested: &HashSet::new(),
        metadata: typed_claim.metadata.as_ref(),
            concordance: typed_claim.concordance.as_ref(),
            concordance_result: None,
    });
    let _ = supersede_projection; // reserved for future expansion
    Ok(augmented)
}

fn parse_sidecar(path: &str) -> Result<ReviewEventSidecar, ToolError> {
    let bytes = std::fs::read_to_string(path)
        .map_err(|e| ToolError::data(format!("read sidecar {path}: {e}")))?;
    serde_json::from_str(&bytes)
        .map_err(|e| ToolError::data(format!("parse sidecar {path}: {e}")))
}

/// PR5h: structured query path for behavioral_concordance claims.
fn query_concordance(state: &ServerState, args: Value) -> Result<Value, ToolError> {
    let manifest_path = arg_str(&args, "manifest_path")?;
    authorize_manifest(state, &manifest_path)?;
    let pattern_kind_filter = arg_str_opt(&args, "pattern_kind");

    let claims = load_claims_with_policy(&manifest_path, &state.policy)?;
    for c in claims.iter() {
        if c.claim.kind == "behavioral_concordance" && c.claim.concordance.is_none() {
            return Err(ToolError::data(format!(
                "claim {}: kind=behavioral_concordance requires a concordance block",
                c.claim.id
            )));
        }
    }
    let items: Vec<Value> = claims
        .iter()
        .filter(|c| c.claim.kind == "behavioral_concordance")
        .filter(|c| c.claim.concordance.is_some())
        .filter_map(|c| {
            let block = c.claim.concordance.as_ref().unwrap();
            let pk = match &block.pattern {
                crate::translate::ManifestConcordancePattern::NumericBand { .. } => "numeric_band",
                crate::translate::ManifestConcordancePattern::RelativeBand { .. } => "relative_band",
                crate::translate::ManifestConcordancePattern::SameOrderOfMagnitude { .. } => "same_order_of_magnitude",
                crate::translate::ManifestConcordancePattern::OrdinalMatch { .. } => "ordinal_match",
                crate::translate::ManifestConcordancePattern::MonotoneWith { .. } => "monotone_with",
            };
            if let Some(ref filt) = pattern_kind_filter {
                if pk != filt.as_str() {
                    return None;
                }
            }
            let mut item = json!({
                "claim_id": c.claim.id,
                "title": c.claim.title,
                "tier": c.claim.tier,
                "pattern_kind": pk,
                "paper_locator": block.paper_locator,
                "prior_source_id": block.prior_binding.source_id,
            });
            if let Some(prov) = c.claim.provenance.as_ref() {
                item.as_object_mut().unwrap().insert(
                    "provenance_kind".into(),
                    json!(prov.effective_kind()),
                );
                if let Some(sid) = prov.source_id() {
                    item.as_object_mut().unwrap().insert(
                        "source_id".into(),
                        json!(sid),
                    );
                }
                if let Some(sc) = prov.source_context() {
                    item.as_object_mut().unwrap().insert(
                        "source_context".into(),
                        json!(sc),
                    );
                }
            }
            Some(item)
        })
        .collect();
    Ok(json!({"items": items}))
}
