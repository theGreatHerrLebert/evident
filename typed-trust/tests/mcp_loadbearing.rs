//! Phase 3 load-bearing tests for the MCP server. Both written
//! BEFORE the implementation lands; both should fail until the
//! server is built; both pass once the implementation is correct.
//!
//! Test #1 exercises the happy path end-to-end (stdio handshake,
//! tool dispatch, allow-list, sidecar load, synthesize,
//! panel_summary projection, JSON serialization).
//!
//! Test #2 exercises the load-bearing security invariant: an
//! `include:` directive that escapes the allow-list MUST be denied
//! by the loader's `PathPolicy`, returning a tier-2 data error
//! while the server stays alive. This is the highest-risk Phase 3
//! behavior (codex review-2 F-3-13).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

/// Returns the cargo-built typed-trust-mcp binary path. Cargo's
/// integration test runner sets `CARGO_BIN_EXE_<name>` for each
/// binary in the crate.
fn mcp_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_typed-trust-mcp"))
}

/// One running typed-trust-mcp subprocess plus pipes for sending
/// MCP JSON-RPC frames and reading responses. JSON-RPC over stdio
/// is line-delimited (one JSON object per line).
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
            .expect("typed-trust-mcp spawn");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self { child, stdin, stdout, next_id: 1 }
    }

    fn initialize(&mut self) -> Value {
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "phase3-loadbearing-test", "version": "0.1.0"}
            }),
        )
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        )
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{}", req).expect("write frame");
        self.stdin.flush().expect("flush");
        // Read until we get a JSON line that matches our id. Skip
        // any notifications the server emits.
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).expect("read line");
            if n == 0 {
                panic!("server closed stdout before responding to id={id}");
            }
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

/// Write the recipe for the BALL adversarial fixture into `dir`:
/// the manifest YAML and a 3-reviewer review_events.json sidecar.
/// Returns (manifest_path, sidecar_path).
fn write_ball_panel_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let manifest_path = dir.join("evident.yaml");
    std::fs::write(
        &manifest_path,
        r#"version: 0.1
project: ball-electrostatic-adversarial
claims:
  - id: ball-electrostatic-synthetic-challenge
    kind: measurement
    tier: ci
    source: .
    title: BALL electrostatic stays within 2 percent of reference
    claim: relative electrostatic error stays strictly below 2 percent across the corpus.
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
    .expect("write manifest");

    let sidecar_path = dir.join("review_events.json");
    // Three model reviewers, all Challenging. Each violation tuple
    // is target-contradicting (codex F-2B). Distinct event_ids
    // (different rationales → distinct canonical hashes). No
    // backing_claim per event because typed-trust would translate
    // them too — for this happy-path test the procedural category
    // CommandFailure moves status without backing.
    let events = json!({
        "events": [
            {
                "claim_id": "ball-electrostatic-synthetic-challenge",
                "kind": "challenge",
                "author": {"kind": "model", "name": "claude-opus-4-7", "version": "20250101"},
                "rationale": "Opus: row 6pti reports electrostatic_relative_error=0.083, exceeding tolerance bound 0.02. Procedural reproducibility concern.",
                "timestamp": "2026-06-02T10:00:00Z",
                "challenge": {"category": "command_failure"}
            },
            {
                "claim_id": "ball-electrostatic-synthetic-challenge",
                "kind": "challenge",
                "author": {"kind": "model", "name": "claude-sonnet-4-6", "version": "20260301"},
                "rationale": "Sonnet: mean of 0.034 + max of 0.083 across the digest indicates systemic violation, not a transient.",
                "timestamp": "2026-06-02T10:01:00Z",
                "challenge": {"category": "command_failure"}
            },
            {
                "claim_id": "ball-electrostatic-synthetic-challenge",
                "kind": "challenge",
                "author": {"kind": "model", "name": "claude-haiku-4-5", "version": "20251001"},
                "rationale": "Haiku: 3 of 12 structures violate the 0.02 bound — corpus does not satisfy the headline claim.",
                "timestamp": "2026-06-02T10:02:00Z",
                "challenge": {"category": "command_failure"}
            }
        ]
    });
    std::fs::write(
        &sidecar_path,
        serde_json::to_string_pretty(&events).expect("serialize sidecar"),
    )
    .expect("write sidecar");

    (manifest_path, sidecar_path)
}

// ============================================================
// Load-bearing test #1: BALL happy path through MCP
// ============================================================

#[test]
fn loadbearing_phase3_ball_panel_through_mcp() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let (manifest_path, sidecar_path) = write_ball_panel_fixture(tmp.path());

    let mut proc = McpProc::spawn(&[
        "--allow-manifest",
        tmp.path().to_str().expect("tmp str"),
    ]);
    let init = proc.initialize();
    assert!(
        init.get("error").is_none(),
        "initialize should succeed; got {init}"
    );

    let resp = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest_path.to_str().unwrap(),
            "claim_id": "ball-electrostatic-synthetic-challenge",
            "sidecar": sidecar_path.to_str().unwrap(),
        }),
    );

    // Tier-1 protocol errors land under "error"; tier-2 data
    // errors land in result with ok=false. Happy path: no error,
    // result.ok==true, content has the typed-trust report.
    assert!(
        resp.get("error").is_none(),
        "read_report should not return a protocol error; got {resp}"
    );
    let result = resp.get("result").expect("result");
    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .expect("content text");
    let bundle: Value = serde_json::from_str(content).expect("content parses as JSON");
    let report = bundle
        .get("report")
        .or_else(|| bundle.get("data"))
        .expect("report block");

    // Target is contested because three procedural Challenges
    // move status without requiring backing claims.
    assert_eq!(
        report["status"].as_str(),
        Some("contested"),
        "expected target Contested after 3 procedural Challenges; bundle: {bundle}"
    );
    // Phase 2c panel_summary should report 3 reviewers.
    assert_eq!(
        report["_graph"]["panel_summary"]["n_reviewers"].as_u64(),
        Some(3),
        "expected n_reviewers==3; panel_summary: {}",
        report["_graph"]["panel_summary"]
    );

    proc.shutdown();
}

// ============================================================
// Load-bearing test #2: include-escape is denied (security)
// ============================================================

#[test]
fn loadbearing_phase3_include_escape_denied_security() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let allowed = tmp.path().join("allowed");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&allowed).expect("mkdir allowed");
    std::fs::create_dir_all(&outside).expect("mkdir outside");

    // The "secret" outside the allowed dir. A real attack would
    // try to coerce the server into reading /etc/passwd; here we
    // use a sibling temp file that we can synthesize specific
    // content into.
    let secret = outside.join("secret.yaml");
    std::fs::write(&secret, "claims:\n  - id: NOT-ALLOWED\n").expect("write secret");

    // Manifest INSIDE the allowed dir whose `include:` escapes.
    let manifest_path = allowed.join("evident.yaml");
    std::fs::write(
        &manifest_path,
        format!(
            r#"version: 0.1
project: include-escape-test
include:
  - {}
"#,
            // Use the relative path so the manifest is a realistic
            // attack vector (a Phase 3 attacker can't write
            // absolute system paths into an allowed manifest
            // without first writing into the allowed dir, which
            // implies they already have local write access — but
            // relative `..` escapes are the canonical path-
            // traversal pattern).
            // Use a forward path that canonicalizes outside the
            // allowed dir.
            secret.to_str().unwrap()
        ),
    )
    .expect("write manifest");

    let mut proc = McpProc::spawn(&[
        "--allow-manifest",
        allowed.to_str().expect("allowed str"),
    ]);
    proc.initialize();

    let resp = proc.call_tool(
        "read_report",
        json!({
            "manifest_path": manifest_path.to_str().unwrap(),
            "claim_id": "anything",
        }),
    );

    // The root manifest path is INSIDE the allow set, so the
    // request reaches the loader. The loader's PathPolicy must
    // reject the include resolution. That's a TIER-2 data error
    // (ok: false) — the root path was authorized but the manifest
    // contents are now broken.
    let result = resp.get("result").expect("response should carry a result");
    let is_error = result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
    assert!(
        is_error,
        "include-escape should produce isError=true tool result; got {resp}"
    );
    let content_str = result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    assert!(
        content_str.contains("include") || content_str.contains("allow"),
        "error message should mention include / allow-list policy; got: {content_str}"
    );

    // The server MUST stay alive. Issue a benign call against a
    // fully-allowed manifest and assert the response is healthy.
    let benign_dir = tmp.path().join("benign");
    std::fs::create_dir_all(&benign_dir).expect("mkdir benign");
    let benign_manifest = benign_dir.join("evident.yaml");
    std::fs::write(
        &benign_manifest,
        r#"version: 0.1
project: post-escape-survival
claims:
  - id: claim-A
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
"#,
    )
    .expect("write benign manifest");

    // Restart the proc with both directories allowed (the
    // attacker scenario already exited above; here we re-prove
    // server survival by talking to the still-running attacker
    // proc first, then this fresh one if needed). To make the
    // "process survived" assertion concrete, we call a tool on
    // the SAME process against a different allowed manifest.
    // The allowed dir was the outer tmp/allowed; for survival we
    // need a path inside that allow set.
    let survival_manifest = allowed.join("survival.yaml");
    std::fs::write(
        &survival_manifest,
        r#"version: 0.1
project: post-escape-survival
claims:
  - id: survivor
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
"#,
    )
    .expect("write survival manifest");

    let survival_resp = proc.call_tool(
        "list_claims",
        json!({"manifest_path": survival_manifest.to_str().unwrap()}),
    );
    assert!(
        survival_resp.get("error").is_none(),
        "server should be alive after the include-escape denial; survival call: {survival_resp}"
    );

    let _ = benign_manifest;  // future smoke if needed
    proc.shutdown();
}
