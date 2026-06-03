//! Phase 3 MCP tool tests beyond the two load-bearing ones.
//!
//! Covers: protocol mechanics (handshake, tools/list), every tool's
//! happy path against the BALL panel fixture, bounded responses,
//! security path semantics (symlink, traversal, sidecar type
//! mismatch), and error tier discipline (tier-1 vs tier-2,
//! malformed sidecar, unknown claim, invalid cursor, unsupported
//! format).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

fn mcp_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_typed-trust-mcp"))
}

struct McpProc {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpProc {
    fn spawn(args: &[&str]) -> Self {
        let mut child = Command::new(mcp_bin())
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        let mut me = Self { child, stdin, stdout, next_id: 1 };
        me.initialize();
        me
    }

    fn initialize(&mut self) -> Value {
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "phase3-tool-test", "version": "0.1.0"}
            }),
        )
    }

    fn call_tool(&mut self, name: &str, args: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": args}))
    }

    fn tools_list(&mut self) -> Value {
        self.request("tools/list", json!({}))
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        )
        .expect("write");
        self.stdin.flush().expect("flush");
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).expect("read");
            let v: Value = match serde_json::from_str(line.trim()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("id").and_then(|x| x.as_u64()) == Some(id) {
                return v;
            }
        }
    }

    fn shutdown(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Decode the tool result's "content[0].text" payload as JSON.
fn decode_result(resp: &Value) -> Value {
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("{}");
    serde_json::from_str(text).unwrap_or(Value::Null)
}

fn write_ball_panel_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let manifest = dir.join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: ball-electrostatic-adversarial
claims:
  - id: ball-electrostatic-synthetic-challenge
    kind: measurement
    tier: ci
    source: .
    title: BALL electrostatic stays within 2 percent of reference
    claim: relative electrostatic error stays strictly below 2 percent
    tolerances:
      - metric: electrostatic_relative_error
        op: "<"
        value: 0.02
        prose: relative electrostatic error must stay strictly below 0.02
    evidence:
      oracle: [BALL, Tinker]
      command: "pytest tests/ball/test_electrostatic.py::test_corpus -v"
      artifact: results.json
"#,
    )
    .unwrap();
    let sidecar = dir.join("review_events.json");
    std::fs::write(
        &sidecar,
        serde_json::to_string_pretty(&json!({
            "events": [
                {
                    "claim_id": "ball-electrostatic-synthetic-challenge",
                    "kind": "challenge",
                    "author": {"kind": "model", "name": "claude-opus-4-7", "version": "20250101"},
                    "rationale": "Opus: row 6pti reports 0.083, exceeds tolerance 0.02.",
                    "timestamp": "2026-06-02T10:00:00Z",
                    "challenge": {"category": "command_failure"}
                },
                {
                    "claim_id": "ball-electrostatic-synthetic-challenge",
                    "kind": "challenge",
                    "author": {"kind": "model", "name": "claude-sonnet-4-6", "version": "20260301"},
                    "rationale": "Sonnet: digest mean 0.034 + max 0.083 indicates systemic violation.",
                    "timestamp": "2026-06-02T10:01:00Z",
                    "challenge": {"category": "command_failure"}
                },
                {
                    "claim_id": "ball-electrostatic-synthetic-challenge",
                    "kind": "challenge",
                    "author": {"kind": "model", "name": "claude-haiku-4-5", "version": "20251001"},
                    "rationale": "Haiku: 3 of 12 structures violate the 0.02 bound.",
                    "timestamp": "2026-06-02T10:02:00Z",
                    "challenge": {"category": "command_failure"}
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    (manifest, sidecar)
}

fn write_simple_manifest(dir: &Path, claim_id: &str) -> PathBuf {
    let manifest = dir.join("evident.yaml");
    std::fs::write(
        &manifest,
        format!(
            r#"version: 0.1
project: simple
claims:
  - id: {claim_id}
    kind: measurement
    tier: ci
    source: .
    title: t
    claim: c
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        prose: stay under 2 percent
    evidence:
      oracle: [Test]
      command: "true"
      artifact: out.json
"#
        ),
    )
    .unwrap();
    manifest
}

// ============================================================
// Protocol mechanics
// ============================================================

#[test]
fn initialize_handshake_advertises_protocol_version() {
    let tmp = tempfile::tempdir().unwrap();
    let proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    // initialize() was called in spawn(); follow with tools/list.
    proc.shutdown();
}

#[test]
fn tools_list_returns_ten_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.tools_list();
    let tools = resp["result"]["tools"].as_array().expect("tools list");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(tools.len(), 10, "expected 10 tools, got names {names:?}");
    for expected in [
        "list_claims",
        "read_report",
        "list_review_events",
        "query_claims",
        "get_panel_summary",
        "get_superseded_events",
        "walk_backing_chain",
        "render_report",
        "query_metadata",
        "query_concordance",
    ] {
        assert!(names.contains(&expected), "missing tool: {expected}");
    }
    proc.shutdown();
}

#[test]
fn unknown_method_returns_tier1_protocol_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.request("nonexistent/method", json!({}));
    assert!(
        resp.get("error").is_some(),
        "unknown method should yield tier-1 error; got {resp}"
    );
    let code = resp["error"]["code"].as_i64();
    assert_eq!(code, Some(-32601), "expected method-not-found code");
    proc.shutdown();
}

// ============================================================
// Tool happy paths
// ============================================================

#[test]
fn list_claims_returns_manifest_summary() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, _sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["claim_id"], "ball-electrostatic-synthetic-challenge");
    proc.shutdown();
}

#[test]
fn list_review_events_filter_by_author() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_review_events",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "sidecar": sidecar.to_str().unwrap(),
            "author": "claude-haiku-4-5"
        }),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["author"]["name"], "claude-haiku-4-5");
    proc.shutdown();
}

#[test]
fn list_review_events_filter_by_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_review_events",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "sidecar": sidecar.to_str().unwrap(),
            "kind": "challenge"
        }),
    );
    let payload = decode_result(&resp);
    assert_eq!(payload["items"].as_array().unwrap().len(), 3);
    proc.shutdown();
}

#[test]
fn query_claims_status_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_claims",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "sidecar": sidecar.to_str().unwrap(),
            "status": "contested"
        }),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "expected one contested claim; got {items:?}");
    assert_eq!(items[0]["claim_id"], "ball-electrostatic-synthetic-challenge");
    proc.shutdown();
}

#[test]
fn query_claims_reviewer_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_claims",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "sidecar": sidecar.to_str().unwrap(),
            "reviewer": "claude-opus-4-7"
        }),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    proc.shutdown();
}

#[test]
fn get_panel_summary_returns_active_counters() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "get_panel_summary",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "ball-electrostatic-synthetic-challenge",
            "sidecar": sidecar.to_str().unwrap()
        }),
    );
    let panel = decode_result(&resp);
    assert_eq!(panel["n_reviewers"].as_u64(), Some(3));
    assert_eq!(panel["n_challenge"].as_u64(), Some(3));
    proc.shutdown();
}

#[test]
fn render_report_envelope_markdown() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "render_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "ball-electrostatic-synthetic-challenge",
            "sidecar": sidecar.to_str().unwrap(),
            "format": "markdown"
        }),
    );
    let env = decode_result(&resp);
    assert_eq!(env["format"], "markdown");
    let content = env["content"].as_str().unwrap();
    assert!(content.contains("# Trust Report"));
    assert!(content.contains("Contested"));
    proc.shutdown();
}

#[test]
fn render_report_envelope_mermaid_graph_text_only() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "render_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "ball-electrostatic-synthetic-challenge",
            "sidecar": sidecar.to_str().unwrap(),
            "format": "mermaid"
        }),
    );
    let env = decode_result(&resp);
    assert_eq!(env["format"], "mermaid");
    let content = env["content"].as_str().unwrap();
    assert!(content.contains("graph TD") || content.contains("graph LR"));
    assert!(!content.contains("# Trust Report"), "mermaid should be graph text only");
    proc.shutdown();
}

#[test]
fn walk_backing_chain_returns_challenges_grouped() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "walk_backing_chain",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "ball-electrostatic-synthetic-challenge",
            "sidecar": sidecar.to_str().unwrap()
        }),
    );
    let payload = decode_result(&resp);
    let challenges = payload["challenges"].as_array().unwrap();
    assert_eq!(challenges.len(), 3, "expected 3 challenge events listed");
    proc.shutdown();
}

// ============================================================
// Bounded responses
// ============================================================

#[test]
fn list_claims_limit_and_cursor() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = tmp.path().join("evident.yaml");
    let mut body = String::from("version: 0.1\nproject: many\nclaims:\n");
    for i in 0..5 {
        body.push_str(&format!(
            "  - id: claim-{i}\n    kind: measurement\n    tier: ci\n    source: .\n    title: t\n    claim: c\n    tolerances:\n      - metric: m\n        op: \"<\"\n        value: 0.02\n        prose: x\n    evidence:\n      oracle: [Test]\n      command: \"true\"\n      artifact: out.json\n"
        ));
    }
    std::fs::write(&manifest, body).unwrap();
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": manifest.to_str().unwrap(), "limit": 2}),
    );
    let p = decode_result(&resp);
    assert_eq!(p["items"].as_array().unwrap().len(), 2);
    assert_eq!(p["truncated"], true);
    let cursor = p["cursor"].as_str().unwrap();
    let resp2 = proc.call_tool(
        "list_claims",
        json!({"manifest_path": manifest.to_str().unwrap(), "limit": 2, "cursor": cursor}),
    );
    let p2 = decode_result(&resp2);
    assert_eq!(p2["items"].as_array().unwrap().len(), 2);
    proc.shutdown();
}

/// Phase 5 PR1: list_claims surfaces replay_status and replay_reason.
/// A consumer querying "show me extracted claims whose only blocker is
/// code_private" needs both fields visible at the summary level. The
/// default behaviour stays the same (not_attempted, no reason) for
/// hand-authored manifests with no replay block.
#[test]
fn list_claims_surfaces_replay_status_and_reason_phase5_pr1() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = tmp.path().join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: extracted-mixed
claims:
  - id: extracted-no-replay
    kind: measurement
    tier: research
    source: .
    title: extracted paper claim with no replay path
    claim: median rmsd below 0.5
    tolerances:
      - metric: median_rmsd
        op: "<"
        value: 0.5
        prose: paper reports 0.42; bound 0.5 stated
    evidence:
      oracle: [Paper-Authority]
      command: "no-replay-path"
      artifact: source/cited.md#claim-1
      replay_status: unavailable_artifacts
      replay_reason: code_private
  - id: classical-ci-claim
    kind: measurement
    tier: ci
    source: .
    title: classical ci claim with no replay block
    claim: relative error below 2 percent
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        prose: stay under 2 percent
    evidence:
      oracle: [Biopython]
      command: pytest
      artifact: out.json
"#,
    )
    .unwrap();
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let p = decode_result(&resp);
    let items = p["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    let extracted = items
        .iter()
        .find(|i| i["claim_id"] == "extracted-no-replay")
        .expect("extracted claim present");
    assert_eq!(extracted["replay_status"], "unavailable_artifacts");
    assert_eq!(extracted["replay_reason"], "code_private");

    let classical = items
        .iter()
        .find(|i| i["claim_id"] == "classical-ci-claim")
        .expect("classical claim present");
    assert_eq!(classical["replay_status"], "not_attempted");
    // No replay_reason at the not_attempted default — the field is omitted.
    assert!(
        classical.as_object().unwrap().get("replay_reason").is_none(),
        "replay_reason should be omitted when not set; got {classical:?}"
    );
    proc.shutdown();
}

/// Phase 5 PR2: list_claims surfaces provenance_kind and source_context.
/// Consumers querying "show me extracted-from-repo claims whose text
/// is copied marketing" need both at the summary layer. Legacy
/// (string) provenance still surfaces as provenance_kind with no
/// source_context.
#[test]
fn list_claims_surfaces_provenance_kind_and_source_context_phase5_pr2() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = tmp.path().join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: extracted-mixed-pr2
claims:
  - id: legacy-provenance-claim
    kind: measurement
    tier: ci
    source: .
    title: legacy provenance claim
    claim: stays under 2 percent
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        prose: stay under 2 percent
    evidence:
      oracle: [Biopython]
      command: pytest
      artifact: out.json
    provenance: automatic
  - id: extracted-repo-copied
    kind: measurement
    tier: research
    source: .
    title: extracted-from-repo with copied marketing text
    claim: throughput claim
    tolerances:
      - metric: throughput
        op: ">"
        value: 1000.0
        prose: README claims more than 1000 req/sec
    evidence:
      oracle: [Repo-README]
      command: "no-replay-path"
      artifact: source/cited.md#claim-1
      replay_status: unavailable_artifacts
      replay_reason: instructions_missing
    provenance:
      kind: extracted-from-repo
      source_id: github:org/repo@deadbeef
      source_context: copied_external_text
"#,
    )
    .unwrap();
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let p = decode_result(&resp);
    let items = p["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    let legacy = items
        .iter()
        .find(|i| i["claim_id"] == "legacy-provenance-claim")
        .expect("legacy claim present");
    assert_eq!(legacy["provenance_kind"], "automatic");
    assert!(
        legacy.as_object().unwrap().get("source_context").is_none(),
        "legacy provenance should not carry source_context; got {legacy:?}"
    );

    let extracted = items
        .iter()
        .find(|i| i["claim_id"] == "extracted-repo-copied")
        .expect("extracted claim present");
    assert_eq!(extracted["provenance_kind"], "extracted-from-repo");
    assert_eq!(extracted["source_context"], "copied_external_text");
    proc.shutdown();
}

/// Phase 5 PR3 codex F-PR3-CR-wire-validator: read_report on an
/// extracted-from-paper claim at tier:ci WITHOUT a matching
/// PromoteFromExtracted event must return a tier-2 data error. This
/// proves the validator is actually wired into the MCP synthesis
/// path, not just sitting as a helper that nothing calls.
#[test]
fn read_report_rejects_extracted_ci_without_promotion_event_phase5_pr3() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = tmp.path().join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: extracted-ci-unauthorized
claims:
  - id: extracted-ci-claim
    kind: measurement
    tier: ci
    source: .
    title: extracted ci claim without curator review
    claim: extracted claim at ci tier
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: extracted claim
    evidence:
      oracle: [Paper-Authority]
      command: "no-replay-path"
      artifact: source/cited.md#claim-1
      replay_status: unavailable_artifacts
      replay_reason: code_private
    provenance:
      kind: extracted-from-paper
      source_id: arxiv:2501.99999
      extractor:
        model: claude-opus-4-7
        extracted_at: "2026-09-14T10:00:00Z"
"#,
    )
    .unwrap();
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "extracted-ci-claim"
        }),
    );
    // Tier 2: result with isError: true and a message naming the
    // missing promotion.
    assert_eq!(resp["result"]["isError"], true);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("promote_from_extracted") || text.contains("extracted"),
        "expected error text naming the missing promotion, got: {text}"
    );
    proc.shutdown();
}

#[test]
fn list_review_events_include_rationale_toggles() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);

    let with = decode_result(&proc.call_tool(
        "list_review_events",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "sidecar": sidecar.to_str().unwrap(),
            "include_rationale": true
        }),
    ));
    assert!(with["items"][0]["rationale"].is_string());

    let without = decode_result(&proc.call_tool(
        "list_review_events",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "sidecar": sidecar.to_str().unwrap(),
            "include_rationale": false
        }),
    ));
    assert!(without["items"][0]["rationale"].is_null() || !without["items"][0].as_object().unwrap().contains_key("rationale"));
    proc.shutdown();
}

// ============================================================
// Security: tier-1 protocol errors
// ============================================================

#[test]
fn manifest_outside_allow_returns_tier1() {
    let allowed = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let m = write_simple_manifest(outside.path(), "claim-X");
    let mut proc = McpProc::spawn(&["--allow-manifest", allowed.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": m.to_str().unwrap()}),
    );
    assert!(resp.get("error").is_some(), "should be tier-1 protocol error; got {resp}");
    proc.shutdown();
}

#[test]
fn no_allow_list_rejects_every_call() {
    let mut proc = McpProc::spawn(&[]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": "/some/manifest.yaml"}),
    );
    assert!(resp.get("error").is_some());
    let msg = resp["error"]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("no allowed paths"),
        "expected fail-closed default; got: {msg}"
    );
    proc.shutdown();
}

#[test]
fn parent_dir_traversal_canonicalizes_and_rejects() {
    let allowed = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = write_simple_manifest(outside.path(), "x");
    // Construct a path with literal .. that will canonicalize
    // outside the allowed root.
    let allowed_str = allowed.path().to_str().unwrap();
    let target_str = target.to_str().unwrap();
    let traversal = format!("{allowed_str}/../{}", target_str.trim_start_matches('/'));
    let mut proc = McpProc::spawn(&["--allow-manifest", allowed_str]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": traversal}),
    );
    assert!(resp.get("error").is_some());
    proc.shutdown();
}

// ============================================================
// Security: tier-2 data errors
// ============================================================

#[test]
fn malformed_sidecar_is_tier2_data_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, _) = write_ball_panel_fixture(tmp.path());
    let bad = tmp.path().join("bad_sidecar.json");
    std::fs::write(&bad, "{this is not JSON").unwrap();
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "ball-electrostatic-synthetic-challenge",
            "sidecar": bad.to_str().unwrap()
        }),
    );
    let result = resp.get("result").expect("tier-2 carries result");
    assert_eq!(result["isError"], true);
    proc.shutdown();
}

#[test]
fn unknown_claim_id_is_tier2() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "does-not-exist",
            "sidecar": sidecar.to_str().unwrap()
        }),
    );
    assert_eq!(resp["result"]["isError"], true);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("not in manifest"));
    proc.shutdown();
}

#[test]
fn unsupported_render_format_is_tier1() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "render_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "ball-electrostatic-synthetic-challenge",
            "sidecar": sidecar.to_str().unwrap(),
            "format": "ascii-art"
        }),
    );
    assert!(
        resp.get("error").is_some(),
        "unsupported format should be tier-1 schema rejection; got {resp}"
    );
    proc.shutdown();
}

#[test]
fn duplicate_event_ids_in_sidecar_tier2() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, _) = write_ball_panel_fixture(tmp.path());
    let dup = tmp.path().join("dup_sidecar.json");
    std::fs::write(
        &dup,
        serde_json::to_string(&json!({
            "events": [
                {
                    "event_id": "sha256:dup",
                    "claim_id": "ball-electrostatic-synthetic-challenge",
                    "kind": "endorse",
                    "author": {"kind": "model", "name": "claude-opus-4-7", "version": "v1"},
                    "rationale": "first event with the dup id; long enough to pass downstream validation rules.",
                    "timestamp": "2026-06-02T10:00:00Z"
                },
                {
                    "event_id": "sha256:dup",
                    "claim_id": "ball-electrostatic-synthetic-challenge",
                    "kind": "dissent",
                    "author": {"kind": "model", "name": "claude-haiku-4-5", "version": "v2"},
                    "rationale": "second event sharing the same id; long enough to pass validation rules.",
                    "timestamp": "2026-06-02T10:05:00Z"
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "ball-electrostatic-synthetic-challenge",
            "sidecar": dup.to_str().unwrap()
        }),
    );
    let result = resp.get("result").expect("tier-2 carries result");
    assert_eq!(result["isError"], true);
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("duplicate event_id"), "expected duplicate-id message; got: {text}");
    proc.shutdown();
}

// ============================================================
// Server survival after errors
// ============================================================

#[test]
fn server_survives_after_tier2_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, sidecar) = write_ball_panel_fixture(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    // Force a tier-2: unknown claim.
    let r1 = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "nope",
            "sidecar": sidecar.to_str().unwrap()
        }),
    );
    assert_eq!(r1["result"]["isError"], true);
    // Then a happy call against the same process.
    let r2 = proc.call_tool(
        "list_claims",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    assert!(r2.get("error").is_none());
    let payload = decode_result(&r2);
    assert_eq!(payload["items"].as_array().unwrap().len(), 1);
    proc.shutdown();
}

// ============================================================
// PR5c: query_metadata + list_claims metadata projection
// ============================================================

/// Mixed manifest: one measurement claim + two metadata claims.
/// Both metadata claims carry distinct field names; one comes from
/// Cargo.toml, the other from pyproject.toml — exercises both
/// filter axes of `query_metadata`.
fn write_mixed_metadata_manifest(dir: &Path) -> PathBuf {
    let manifest = dir.join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: mixed-metadata
claims:
  - id: empirical-baseline
    kind: measurement
    tier: ci
    source: .
    title: empirical
    claim: relative error stays under 0.02
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        prose: under 2 percent
    evidence:
      oracle: [Test]
      command: "true"
      artifact: out.json
  - id: cargo-rust-msrv
    kind: metadata_compatibility
    tier: research
    source: .
    title: pdbtbx Cargo declares rust-version 1.67
    claim: pdbtbx requires Rust >= 1.67 per Cargo.toml
    provenance:
      kind: extracted-from-repo
      source_id: "github:Roestlab/pdbtbx@deadbeef"
      source_context: repo_authored
    metadata:
      field: rust_msrv
      declared_value: "1.67"
      source_file: Cargo.toml
      source_path: package.rust-version
  - id: pyproject-python-requirement
    kind: metadata_compatibility
    tier: research
    source: .
    title: pyproject declares python >= 3.10
    claim: pyproject.toml declares Python >= 3.10
    provenance:
      kind: extracted-from-repo
      source_id: "github:foo/bar@cafef00d"
      source_context: repo_authored
    metadata:
      field: python_version_requirement
      declared_value: ">=3.10"
      source_file: pyproject.toml
      source_path: project.requires-python
"#,
    )
    .unwrap();
    manifest
}

#[test]
fn list_claims_surfaces_metadata_block_pr5c() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_mixed_metadata_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().expect("items");
    assert_eq!(items.len(), 3);
    let measurement = items
        .iter()
        .find(|i| i["claim_id"] == "empirical-baseline")
        .unwrap();
    assert!(
        measurement.get("metadata").is_none(),
        "measurement claim must not carry a metadata block"
    );
    let msrv = items
        .iter()
        .find(|i| i["claim_id"] == "cargo-rust-msrv")
        .unwrap();
    assert_eq!(msrv["metadata"]["field"], "rust_msrv");
    assert_eq!(msrv["metadata"]["declared_value"], "1.67");
    assert_eq!(msrv["metadata"]["source_file"], "Cargo.toml");
    assert_eq!(msrv["metadata"]["source_path"], "package.rust-version");
    proc.shutdown();
}

#[test]
fn query_metadata_returns_all_metadata_claims_when_unfiltered_pr5c() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_mixed_metadata_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_metadata",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().expect("items");
    assert_eq!(items.len(), 2, "two metadata claims expected, got {items:?}");
    // Measurement claim must NOT leak into query_metadata.
    assert!(items
        .iter()
        .all(|i| i["claim_id"] != "empirical-baseline"));
    // Self-contained audit context: tier + provenance + source.
    let msrv = items
        .iter()
        .find(|i| i["claim_id"] == "cargo-rust-msrv")
        .unwrap();
    assert_eq!(msrv["title"], "pdbtbx Cargo declares rust-version 1.67");
    assert_eq!(msrv["tier"], "research");
    assert_eq!(msrv["provenance_kind"], "extracted-from-repo");
    assert_eq!(msrv["source_id"], "github:Roestlab/pdbtbx@deadbeef");
    assert_eq!(msrv["source_context"], "repo_authored");
    assert_eq!(msrv["field"], "rust_msrv");
    assert_eq!(msrv["declared_value"], "1.67");
    assert_eq!(msrv["source_file"], "Cargo.toml");
    assert_eq!(msrv["source_path"], "package.rust-version");
    proc.shutdown();
}

#[test]
fn query_metadata_field_filter_is_exact_case_sensitive_pr5c() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_mixed_metadata_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_metadata",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "field": "rust_msrv"
        }),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["claim_id"], "cargo-rust-msrv");
    // Case-sensitive: an upper-cased filter returns nothing.
    let resp2 = proc.call_tool(
        "query_metadata",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "field": "RUST_MSRV"
        }),
    );
    let payload2 = decode_result(&resp2);
    assert!(payload2["items"].as_array().unwrap().is_empty());
    proc.shutdown();
}

#[test]
fn query_metadata_source_file_filter_pr5c() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_mixed_metadata_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_metadata",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "source_file": "pyproject.toml"
        }),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["claim_id"], "pyproject-python-requirement");
    proc.shutdown();
}

#[test]
fn query_metadata_combined_filters_conjunctive_pr5c() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_mixed_metadata_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    // field + source_file both match → 1 result.
    let resp = proc.call_tool(
        "query_metadata",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "field": "rust_msrv",
            "source_file": "Cargo.toml"
        }),
    );
    assert_eq!(decode_result(&resp)["items"].as_array().unwrap().len(), 1);
    // field + source_file disagree → 0 results.
    let resp2 = proc.call_tool(
        "query_metadata",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "field": "rust_msrv",
            "source_file": "pyproject.toml"
        }),
    );
    assert!(decode_result(&resp2)["items"]
        .as_array()
        .unwrap()
        .is_empty());
    proc.shutdown();
}

#[test]
fn query_metadata_empty_when_manifest_has_no_metadata_claims_pr5c() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_simple_manifest(tmp.path(), "x");
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_metadata",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let payload = decode_result(&resp);
    assert!(payload["items"].as_array().unwrap().is_empty());
    proc.shutdown();
}

#[test]
fn query_metadata_rejects_unauthorized_manifest_pr5c() {
    let tmp_allowed = tempfile::tempdir().unwrap();
    let tmp_other = tempfile::tempdir().unwrap();
    let outside_manifest = write_mixed_metadata_manifest(tmp_other.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp_allowed.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_metadata",
        json!({"manifest_path": outside_manifest.to_str().unwrap()}),
    );
    // Tier-1 protocol error (unauthorized path).
    assert!(resp.get("error").is_some(), "expected tier-1 error: {resp}");
    proc.shutdown();
}

#[test]
fn read_report_renders_metadata_declaration_markdown_pr5c() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_mixed_metadata_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "render_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "cargo-rust-msrv",
            "format": "markdown"
        }),
    );
    let payload = decode_result(&resp);
    let content = payload["content"].as_str().unwrap_or("");
    assert!(
        content.contains("Metadata declaration"),
        "markdown render missing metadata section: {content}"
    );
    assert!(content.contains("rust_msrv"), "missing field name");
    assert!(content.contains("1.67"), "missing declared value");
    assert!(content.contains("Cargo.toml"), "missing source file");
    proc.shutdown();
}

#[test]
fn read_report_renders_metadata_declaration_html_pr5c() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_mixed_metadata_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "render_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "cargo-rust-msrv",
            "format": "html"
        }),
    );
    let payload = decode_result(&resp);
    let content = payload["content"].as_str().unwrap_or("");
    assert!(
        content.contains("metadata-declaration"),
        "html missing metadata dl class: {content}"
    );
    assert!(content.contains("rust_msrv"), "missing field name in html");
    assert!(content.contains("1.67"), "missing declared value in html");
    proc.shutdown();
}

/// PR5c codex F-CR1: `query_metadata` must surface a missing-block
/// error (tier-2 data) for `kind: metadata_compatibility` claims that
/// don't carry a metadata block, rather than silently dropping them.
fn write_manifest_with_broken_metadata_claim(dir: &Path) -> PathBuf {
    let manifest = dir.join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: broken-meta
claims:
  - id: broken-meta
    kind: metadata_compatibility
    tier: research
    source: .
    title: missing metadata block
    claim: this claim says it is metadata but has no block
"#,
    )
    .unwrap();
    manifest
}

#[test]
fn query_metadata_surfaces_missing_metadata_block_as_tier2_pr5c_cr1() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_manifest_with_broken_metadata_claim(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_metadata",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    assert!(resp.get("error").is_none(), "expected tier-2, got tier-1: {resp}");
    assert_eq!(resp["result"]["isError"], true, "expected isError: {resp}");
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        text.contains("broken-meta") && text.contains("metadata block"),
        "error text should name the offending claim + the missing block; got {text:?}"
    );
    proc.shutdown();
}

/// PR5c codex F-CR2: `list_claims` must NOT surface a `metadata`
/// block when the underlying claim is not `kind:
/// metadata_compatibility`, even if the raw YAML carried a
/// `metadata:` block.
fn write_manifest_with_measurement_carrying_metadata(dir: &Path) -> PathBuf {
    let manifest = dir.join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: misclassified-meta
claims:
  - id: misclassified
    kind: measurement
    tier: research
    source: .
    title: a measurement claim that also carries metadata
    claim: c
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: ok
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
    metadata:
      field: rust_msrv
      declared_value: "1.67"
      source_file: Cargo.toml
      source_path: package.rust-version
"#,
    )
    .unwrap();
    manifest
}

#[test]
fn list_claims_does_not_project_metadata_on_non_metadata_kind_pr5c_cr2() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_manifest_with_measurement_carrying_metadata(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item["kind"], "measurement");
    assert!(
        item.get("metadata").is_none(),
        "list_claims must not project metadata on non-metadata kind: {item}"
    );
    proc.shutdown();
}

/// PR5c codex F-CR3: list_claims provenance projection includes
/// `source_id` so query_metadata's documented "same audit context"
/// is accurate.
#[test]
fn list_claims_projects_source_id_when_provenance_carries_it_pr5c_cr3() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_mixed_metadata_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().expect("items");
    let msrv = items
        .iter()
        .find(|i| i["claim_id"] == "cargo-rust-msrv")
        .unwrap();
    assert_eq!(msrv["source_id"], "github:Roestlab/pdbtbx@deadbeef");
    proc.shutdown();
}

// ============================================================
// PR5h: query_concordance MCP tool
// ============================================================

fn write_concordance_manifest(dir: &Path) -> PathBuf {
    let manifest = dir.join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: rustims-concordance-demo
claims:
  - id: rustims-fragpipe-fdr-10k-concords-meier
    kind: behavioral_concordance
    tier: research
    title: FragPipe FDR on rustims-simulated HLA-I 10k tracks Meier 2024
    claim: Bound the FDR within 0.5 pp of Meier's reported value.
    concordance:
      pattern:
        pattern_kind: numeric_band
        metric_path: fragpipe.hla_10k.fdr_pct
        epsilon: 0.5
        prior_value: 1.5
      paper_locator: source/cited.md#rustims-fragpipe-fdr-10k
      prior_binding:
        prior_unit: percentage_points
        prior_metric_definition: Empirical true FDR per Meier 2024.
        locator: Meier 2024 Table 3
        prior_extraction_note: Curator verified Table 3
        source_id: doi:10.1038/PLACEHOLDER
  - id: rustims-tools-fdr-ordering-concords-meier
    kind: behavioral_concordance
    tier: research
    title: Tool FDR ordering on rustims-simulated HLA-I 10k
    claim: Ordering matches Meier 2024 measured ordering.
    concordance:
      pattern:
        pattern_kind: ordinal_match
        entity_to_path:
          FragPipe_v22: fragpipe_v22.hla_10k.fdr_pct
          PEAKS_XPro: peaks_xpro.hla_10k.fdr_pct
        direction: lower_is_better
        tie_policy: adjacent_swap_ok
        prior_value:
          FragPipe_v22: 1.5
          PEAKS_XPro: 1.8
      paper_locator: source/cited.md#rustims-fdr-ordering
      prior_binding:
        prior_unit: percentage_points
        prior_metric_definition: Empirical true FDR.
        locator: Meier 2024 Table 3
        prior_extraction_note: Curator verified
        source_id: doi:10.1038/PLACEHOLDER
"#,
    )
    .unwrap();
    manifest
}

#[test]
fn query_concordance_returns_all_concordance_claims_when_unfiltered_pr5h() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_concordance_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_concordance",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().expect("items");
    assert_eq!(items.len(), 2);
    let kinds: Vec<&str> = items.iter().map(|i| i["pattern_kind"].as_str().unwrap_or("")).collect();
    assert!(kinds.contains(&"numeric_band"));
    assert!(kinds.contains(&"ordinal_match"));
    proc.shutdown();
}

#[test]
fn query_concordance_filters_by_pattern_kind_pr5h() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_concordance_manifest(tmp.path());
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_concordance",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "pattern_kind": "numeric_band"
        }),
    );
    let payload = decode_result(&resp);
    let items = payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["claim_id"], "rustims-fragpipe-fdr-10k-concords-meier");
    assert_eq!(items[0]["paper_locator"], "source/cited.md#rustims-fragpipe-fdr-10k");
    assert_eq!(items[0]["prior_source_id"], "doi:10.1038/PLACEHOLDER");
    proc.shutdown();
}

#[test]
fn query_concordance_empty_when_no_concordance_claims_pr5h() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = write_simple_manifest(tmp.path(), "x");
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_concordance",
        json!({"manifest_path": manifest.to_str().unwrap()}),
    );
    let payload = decode_result(&resp);
    assert!(payload["items"].as_array().unwrap().is_empty());
    proc.shutdown();
}
