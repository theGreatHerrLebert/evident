//! Tool definitions for the MCP server.
//!
//! Tool descriptions follow the 6-point rubric from the Phase 3
//! plan: when to use, when NOT to use, required relationship
//! between manifest_path/sidecar/allow-list, pagination behavior,
//! summary vs. full content, and enum value semantics.

use serde_json::{json, Value};

/// Return the list of tool definitions advertised on `tools/list`.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        list_claims_tool(),
        read_report_tool(),
        list_review_events_tool(),
        query_claims_tool(),
        get_panel_summary_tool(),
        get_superseded_events_tool(),
        walk_backing_chain_tool(),
        render_report_tool(),
    ]
}

fn list_claims_tool() -> Value {
    json!({
        "name": "list_claims",
        "description": "List every claim in a manifest. Use to discover what claims exist before drilling into one with read_report. Returns summary fields only (claim_id, title, tier, kind). For full report content, follow up with read_report. Supports pagination via limit + cursor.\n\n`manifest_path` must lie under an allowed root configured at server startup (--allow-manifest).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string", "description": "Filesystem path to the manifest YAML"},
                "limit": {"type": "integer", "minimum": 1, "description": "Max claims to return"},
                "cursor": {"type": "string", "description": "Opaque continuation token from a prior truncated response"}
            },
            "required": ["manifest_path"]
        }
    })
}

fn read_report_tool() -> Value {
    json!({
        "name": "read_report",
        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string", "description": "Optional review_events.json sidecar path"},
                "last_verified_sidecar": {"type": "string", "description": "Optional last_verified.json sidecar path"}
            },
            "required": ["manifest_path", "claim_id"]
        }
    })
}

fn list_review_events_tool() -> Value {
    json!({
        "name": "list_review_events",
        "description": "Inspect Endorse, Dissent, Challenge, and Supersede events. Prefer claim_id-scoped calls when you need rationale text: they're small and include_rationale defaults true. For corpus-wide scans, leave claim_id unset, set include_rationale=false, and combine filters with pagination. Returns event summaries; for the full augmented TrustReport use read_report instead.\n\nFilters compose conjunctively: kind in {endorse, dissent, challenge, supersede}; author matches identity name; event_id selects exactly one event.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string", "description": "Restrict to one claim's events"},
                "sidecar": {"type": "string", "description": "review_events.json sidecar path"},
                "author": {"type": "string", "description": "Filter by author name (matches Identity.name)"},
                "kind": {"type": "string", "enum": ["endorse", "dissent", "challenge", "supersede"]},
                "event_id": {"type": "string", "description": "Filter to one specific event id"},
                "include_rationale": {"type": "boolean", "description": "Include rationale text in each event row"},
                "limit": {"type": "integer", "minimum": 1},
                "cursor": {"type": "string"}
            },
            "required": ["manifest_path", "sidecar"]
        }
    })
}

fn query_claims_tool() -> Value {
    json!({
        "name": "query_claims",
        "description": "Return claim ids matching a conjunction of filter predicates. Use to answer corpus-level questions like 'which claims are contested?' or 'which claims has reviewer X participated in?'. Combines naturally with read_report for follow-up drilldowns.\n\nPredicates: `status` in {current, contested, superseded}; `reviewer` matches Identity.name on any event for the claim; `event_kind` in {endorse, dissent, challenge, supersede}; boolean filters has_panel_summary and has_superseded.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "sidecar": {"type": "string"},
                "status": {"type": "string", "enum": ["current", "contested", "superseded"]},
                "reviewer": {"type": "string"},
                "event_kind": {"type": "string", "enum": ["endorse", "dissent", "challenge", "supersede"]},
                "has_panel_summary": {"type": "boolean"},
                "has_superseded": {"type": "boolean"},
                "limit": {"type": "integer", "minimum": 1},
                "cursor": {"type": "string"}
            },
            "required": ["manifest_path"]
        }
    })
}

fn get_panel_summary_tool() -> Value {
    json!({
        "name": "get_panel_summary",
        "description": "Return the panel_summary block for one claim. Use when the user asks about reviewer agreement / divergence on a specific claim. Phase 2c projection reflects ACTIVE verdicts only (Phase 2d Supersede semantics applied).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string"}
            },
            "required": ["manifest_path", "claim_id", "sidecar"]
        }
    })
}

fn get_superseded_events_tool() -> Value {
    json!({
        "name": "get_superseded_events",
        "description": "Return Phase 2d audit material for one claim: the three subsections (valid superseded pairs / unresolved / invalid) used in the rendered Superseded Events section. Use when the user asks why an event was retired, or to inspect re-judgment history.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string"}
            },
            "required": ["manifest_path", "claim_id", "sidecar"]
        }
    })
}

fn walk_backing_chain_tool() -> Value {
    json!({
        "name": "walk_backing_chain",
        "description": "Walk the backing-claim graph rooted at one claim, grouped by originating Challenge event. Use to answer questions like 'why is this claim contested?' or 'what backs this challenge?'. Returns nested {challenges -> backing_claims -> children} with cycle detection and a configurable max_depth (default 4). Optional event_id filters to one branch.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string"},
                "event_id": {"type": "string", "description": "Optional: limit traversal to this Challenge event"},
                "max_depth": {"type": "integer", "minimum": 1, "default": 4}
            },
            "required": ["manifest_path", "claim_id", "sidecar"]
        }
    })
}

fn render_report_tool() -> Value {
    json!({
        "name": "render_report",
        "description": "Render the augmented TrustReport in the requested human-readable format. Use when you need a human-presentable rendering to quote or show. `format` is one of: `markdown` (PR-comment style), `html` (self-contained HTML document), or `mermaid` (just the attestation-graph source).\n\nReturns an envelope {format, content, truncated}. Mermaid output is graph text only (no prose).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string"},
                "last_verified_sidecar": {"type": "string"},
                "format": {"type": "string", "enum": ["markdown", "html", "mermaid"]}
            },
            "required": ["manifest_path", "claim_id", "format"]
        }
    })
}
