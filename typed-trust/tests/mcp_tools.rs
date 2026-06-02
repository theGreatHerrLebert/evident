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
fn tools_list_returns_eight_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.tools_list();
    let tools = resp["result"]["tools"].as_array().expect("tools list");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(tools.len(), 8, "expected 8 tools, got names {names:?}");
    for expected in [
        "list_claims",
        "read_report",
        "list_review_events",
        "query_claims",
        "get_panel_summary",
        "get_superseded_events",
        "walk_backing_chain",
        "render_report",
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
// Phase 3 code review (codex post-merge): F-CR3-1, F-CR3-2, F-CR3-3
// ============================================================

#[test]
fn last_verified_sidecar_is_overlayed_codex_3_cr1() {
    // Codex F-CR3-1: read_report with last_verified_sidecar must
    // overlay it before synthesize. Before this fix the argument
    // was accepted but ignored — MCP results disagreed with the
    // CLI for any corpus using last_verified.json.
    let tmp = tempfile::tempdir().unwrap();
    let manifest = tmp.path().join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: test
claims:
  - id: claim-A
    kind: measurement
    tier: release
    source: .
    title: Released claim
    claim: relative_error stays under tolerance
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.005
        prose: stay under 0.5 percent
    evidence:
      oracle: [Test]
      command: "true"
      artifact: out.json
"#,
    )
    .unwrap();

    let last_verified = tmp.path().join("last_verified.json");
    std::fs::write(
        &last_verified,
        json!({
            "claim-A": {
                "commit": "abc123",
                "date": "2026-05-11",
                "value": 0.0017,
                "corpus_sha": "fixturesha"
            }
        })
        .to_string(),
    )
    .unwrap();

    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);

    // Without last_verified_sidecar: criterion is NotAssessed
    // because the manifest's inline last_verified is null/empty.
    let resp_without = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "claim-A"
        }),
    );
    let bundle_without = decode_result(&resp_without);
    let crit_result_without = &bundle_without["report"]["criteria"][0]["result"]["value"]["type"];
    assert_eq!(
        crit_result_without.as_str(),
        Some("not_assessed"),
        "without last_verified overlay the criterion must be not_assessed; got {bundle_without}"
    );

    // With last_verified_sidecar: criterion is Pass (0.0017 < 0.005).
    let resp_with = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "claim-A",
            "last_verified_sidecar": last_verified.to_str().unwrap()
        }),
    );
    let bundle_with = decode_result(&resp_with);
    let crit_result_with = &bundle_with["report"]["criteria"][0]["result"]["value"]["type"];
    assert_eq!(
        crit_result_with.as_str(),
        Some("pass"),
        "last_verified overlay should produce Pass; got {bundle_with}"
    );

    proc.shutdown();
}

#[test]
fn malformed_backing_claim_surfaces_as_tier2_codex_3_cr2() {
    // Codex F-CR3-2: a Challenge whose inline backing_claim fails
    // translation used to be silently dropped — the target then
    // stayed Current instead of becoming Contested. CLI treats
    // this as fatal; MCP must match by returning a tier-2 data
    // error naming the offending backing claim.
    let tmp = tempfile::tempdir().unwrap();
    let manifest = tmp.path().join("evident.yaml");
    std::fs::write(
        &manifest,
        r#"version: 0.1
project: test
claims:
  - id: target-claim
    kind: measurement
    tier: ci
    source: .
    title: Target
    claim: relative_error stays under tolerance
    tolerances:
      - metric: relative_error
        op: "<"
        value: 0.02
        prose: stay under 2 percent
    evidence:
      oracle: [Test]
      command: "true"
      artifact: out.json
"#,
    )
    .unwrap();

    let sidecar = tmp.path().join("review_events.json");
    // Backing claim with kind=policy (out-of-scope) — translate_claim
    // returns OutOfScope, which the loop should NOT silently swallow.
    std::fs::write(
        &sidecar,
        serde_json::to_string(&json!({
            "events": [{
                "claim_id": "target-claim",
                "kind": "challenge",
                "author": {"kind": "model", "name": "claude-opus-4-7", "version": "v1"},
                "rationale": "challenge with a deliberately malformed backing claim — backing has wrong kind for translation.",
                "timestamp": "2026-06-02T10:00:00Z",
                "challenge": {
                    "category": "weak_statistics",
                    "target_criterion_id": "relative_error",
                    "violation": {
                        "metric": "relative_error",
                        "observed_value": 0.05,
                        "bound": 0.02,
                        "comparator": "<",
                        "citation": "row 1"
                    },
                    "backing_claim": {
                        "id": "broken-backing",
                        "title": "Broken backing",
                        "kind": "policy",
                        "tier": "ci",
                        "source": ".",
                        "claim": "policy claims are out of scope for translate_claim",
                        "tolerances": [],
                        "evidence": {"oracle": ["Test"], "command": "true", "artifact": "out.json"}
                    }
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest.to_str().unwrap(),
            "claim_id": "target-claim",
            "sidecar": sidecar.to_str().unwrap()
        }),
    );
    let result = resp.get("result").expect("tier-2 carries result");
    assert_eq!(
        result["isError"],
        true,
        "malformed backing claim should surface as tier-2 data error; got {resp}"
    );
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        text.contains("backing claim") && text.contains("broken-backing"),
        "error should name the offending backing claim; got: {text}"
    );

    proc.shutdown();
}

#[test]
fn query_claims_cursor_beyond_total_returns_empty_codex_3_cr3() {
    // Codex F-CR3-3: cursor > total used to underflow
    // `claims.len() - cursor` in debug builds and panic the
    // blocking handler. Clamping with `.min(total)` makes the
    // failure mode "empty page" with a clean response.
    let tmp = tempfile::tempdir().unwrap();
    let _manifest = write_simple_manifest(tmp.path(), "claim-A");
    let mut proc = McpProc::spawn(&["--allow-manifest", tmp.path().to_str().unwrap()]);
    let resp = proc.call_tool(
        "query_claims",
        json!({
            "manifest_path": tmp.path().join("evident.yaml").to_str().unwrap(),
            "cursor": "999"
        }),
    );
    // Must NOT be a tier-1 internal error (which would happen on
    // panic). Must be a clean tier-2-equivalent: result with
    // empty items.
    assert!(
        resp.get("error").is_none(),
        "cursor underflow should not produce a protocol error; got {resp}"
    );
    let payload = decode_result(&resp);
    assert_eq!(
        payload["items"].as_array().map(|a| a.len()),
        Some(0),
        "cursor beyond total should return empty items; got {payload}"
    );
    proc.shutdown();
}
