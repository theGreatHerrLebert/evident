"""Load-bearing tests for the ``evident-agent-mcp`` stdio server.

Python analog of typed-trust's ``tests/mcp_loadbearing.rs``: spawn the
server as a real subprocess, keep stdin open (the EOF-pipe race only
bites when stdin closes mid-call), drive JSON-RPC over stdio, and assert
the security + capability invariants.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from textwrap import dedent
from typing import Any, Optional

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURE_PYPROJECT = (
    REPO_ROOT / "tests" / "fixtures" / "extract" / "metadata" / "pyproject_repo"
)


class McpProc:
    """Persistent-stdin JSON-RPC driver for the exec-MCP server."""

    def __init__(self, args: list[str], env: Optional[dict] = None):
        self.proc = subprocess.Popen(
            [sys.executable, "-m", "evident_agent.mcp", *args],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
            env=env,
        )
        self._id = 0
        self.stdout_lines: list[str] = []

    def _send(self, obj: dict) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(json.dumps(obj) + "\n")
        self.proc.stdin.flush()

    def _recv(self) -> dict:
        assert self.proc.stdout is not None
        line = self.proc.stdout.readline()
        assert line, "server closed stdout before responding"
        self.stdout_lines.append(line)
        return json.loads(line)  # asserts stdout purity: every frame is JSON

    def initialize(self) -> dict:
        self._id += 1
        self._send(
            {
                "jsonrpc": "2.0",
                "id": self._id,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "loadbearing", "version": "0"},
                },
            }
        )
        resp = self._recv()
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        return resp

    def call(self, name: str, arguments: dict) -> dict:
        self._id += 1
        rid = self._id
        self._send(
            {
                "jsonrpc": "2.0",
                "id": rid,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments},
            }
        )
        return self._recv()

    def send_call_async(self, name: str, arguments: dict) -> int:
        """Send a tools/call without waiting; return its id."""
        self._id += 1
        rid = self._id
        self._send(
            {
                "jsonrpc": "2.0",
                "id": rid,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments},
            }
        )
        return rid

    def recv(self) -> dict:
        return self._recv()

    def close(self) -> None:
        try:
            if self.proc.stdin:
                self.proc.stdin.close()
            self.proc.wait(timeout=10)
        except Exception:
            self.proc.kill()


def _result_payload(frame: dict) -> Any:
    """Parse the structured payload out of a (non-error) tool result."""
    return json.loads(frame["result"]["content"][0]["text"])


def _claim_block(cid: str) -> str:
    return dedent(
        f"""
          - id: {cid}
            kind: measurement
            tier: ci
            source: .
            title: test
            claim: test
            tolerances:
              - metric: relative_error
                op: "<"
                value: 0.01
                prose: x
            evidence:
              oracle: [Test]
              command: "true"
              artifact: out.json
        """
    ).rstrip()


def _write_measurement_manifest(tmp_path: Path, claim_ids=("claim-A",)) -> Path:
    manifest = tmp_path / "evident.yaml"
    body = "version: 0.1\nproject: test\nclaims:\n"
    body += "\n".join(_claim_block(c) for c in claim_ids) + "\n"
    manifest.write_text(body)
    return manifest


# ---------------------------------------------------------------------
# Happy path
# ---------------------------------------------------------------------
def test_extract_metadata_happy(tmp_path: Path) -> None:
    out = tmp_path / "gen"
    proc = McpProc(
        ["--allow-root", str(FIXTURE_PYPROJECT), "--allow-root", str(tmp_path)]
    )
    try:
        proc.initialize()
        frame = proc.call(
            "extract_metadata",
            {"repo": str(FIXTURE_PYPROJECT), "output_dir": str(out)},
        )
        assert frame["result"]["isError"] is False
        payload = _result_payload(frame)
        assert payload["emitted_claims"] >= 1
        assert payload["output_dir"] == str(out)
        assert (out / "evident.yaml").is_file()
    finally:
        proc.close()


# ---------------------------------------------------------------------
# Security: unauthorized path is a tier-1 JSON-RPC error and the server
# stays alive afterwards.
# ---------------------------------------------------------------------
def test_unauthorized_path_tier1_and_survives(tmp_path: Path) -> None:
    out = tmp_path / "gen"
    proc = McpProc(["--allow-root", str(tmp_path)])  # NOT allowing the fixture
    try:
        proc.initialize()
        frame = proc.call(
            "extract_metadata",
            {"repo": str(FIXTURE_PYPROJECT), "output_dir": str(out)},
        )
        assert "error" in frame, frame
        assert frame["error"]["code"] == -32001

        # Server MUST stay alive for a subsequent valid call.
        inside = tmp_path / "repo"
        inside.mkdir()
        (inside / "pyproject.toml").write_text(
            '[project]\nname = "x"\nrequires-python = ">=3.10"\n'
        )
        ok = proc.call(
            "extract_metadata",
            {"repo": str(inside), "output_dir": str(out)},
        )
        assert ok["result"]["isError"] is False
    finally:
        proc.close()


def test_symlink_escape_denied(tmp_path: Path) -> None:
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    outside = tmp_path / "outside_repo"
    outside.mkdir()
    (outside / "pyproject.toml").write_text('[project]\nname = "x"\n')
    link = allowed / "link"
    link.symlink_to(outside)

    proc = McpProc(["--allow-root", str(allowed)])
    try:
        proc.initialize()
        frame = proc.call(
            "extract_metadata",
            {"repo": str(link), "output_dir": str(allowed / "gen")},
        )
        assert "error" in frame, frame
        assert frame["error"]["code"] == -32001
    finally:
        proc.close()


# ---------------------------------------------------------------------
# Capability gate + replay dry-run
# ---------------------------------------------------------------------
def test_replay_capability_gate_forces_dry_run(tmp_path: Path) -> None:
    manifest = _write_measurement_manifest(tmp_path)
    proc = McpProc(["--allow-root", str(tmp_path)])  # no --allow-docker
    try:
        proc.initialize()
        frame = proc.call(
            "replay", {"manifest_path": str(manifest), "claim": "claim-A"}
        )
        assert frame["result"]["isError"] is False
        payload = _result_payload(frame)
        assert payload["capability_gated"] is True
        assert payload["dry_run"] is True
        assert payload["sidecar_path"] is None
        assert len(payload["claims"]) == 1
        assert not (tmp_path / "last_verified.json").exists()
    finally:
        proc.close()


def test_replay_dry_run_explicit(tmp_path: Path) -> None:
    manifest = _write_measurement_manifest(tmp_path)
    proc = McpProc(["--allow-root", str(tmp_path), "--allow-docker"])
    try:
        proc.initialize()
        frame = proc.call(
            "replay",
            {"manifest_path": str(manifest), "claim": "claim-A", "dry_run": True},
        )
        assert frame["result"]["isError"] is False
        payload = _result_payload(frame)
        assert payload["dry_run"] is True
        assert payload["capability_gated"] is False
        assert payload["sidecar_path"] is None
        assert len(payload["claims"]) == 1
    finally:
        proc.close()


# ---------------------------------------------------------------------
# Docker infrastructure error → tier-2 (server stays alive)
# ---------------------------------------------------------------------
def test_replay_docker_infra_error_tier2(tmp_path: Path) -> None:
    manifest = _write_measurement_manifest(tmp_path)
    # Strip PATH so the `docker` binary cannot be found → FileNotFoundError
    # inside docker.run → infrastructure_error → tier-2 isError.
    empty_bin = tmp_path / "emptybin"
    empty_bin.mkdir()
    env = dict(os.environ)
    env["PATH"] = str(empty_bin)
    proc = McpProc(
        ["--allow-root", str(tmp_path), "--allow-docker"], env=env
    )
    try:
        proc.initialize()
        frame = proc.call(
            "replay", {"manifest_path": str(manifest), "claim": "claim-A"}
        )
        # Recoverable tool error, not a protocol error or a crash.
        assert "result" in frame, frame
        assert frame["result"]["isError"] is True
        assert "infrastructure_error" in frame["result"]["content"][0]["text"]
    finally:
        proc.close()


# ---------------------------------------------------------------------
# Concurrency: two replays writing the same sidecar keep it valid JSON.
# ---------------------------------------------------------------------
def test_concurrent_replay_sidecar_integrity(tmp_path: Path) -> None:
    manifest = _write_measurement_manifest(tmp_path)
    sidecar = tmp_path / "last_verified.json"
    proc = McpProc(["--allow-root", str(tmp_path)])
    try:
        proc.initialize()
        id1 = proc.send_call_async(
            "replay",
            {
                "manifest_path": str(manifest),
                "claim": "claim-A",
                "no_execute": True,
                "sidecar": str(sidecar),
            },
        )
        id2 = proc.send_call_async(
            "replay",
            {
                "manifest_path": str(manifest),
                "claim": "claim-A",
                "no_execute": True,
                "sidecar": str(sidecar),
            },
        )
        frames = {}
        for _ in range(2):
            f = proc.recv()
            frames[f["id"]] = f
        for rid in (id1, id2):
            assert frames[rid]["result"]["isError"] is False, frames[rid]
        # Atomic write ⇒ the sidecar is always parseable.
        assert json.loads(sidecar.read_text())
    finally:
        proc.close()


def test_concurrent_different_claims_both_survive(tmp_path: Path) -> None:
    """flock + re-read-under-lock ⇒ no lost update across claims."""
    manifest = _write_measurement_manifest(tmp_path, claim_ids=("claim-A", "claim-B"))
    sidecar = tmp_path / "last_verified.json"
    proc = McpProc(["--allow-root", str(tmp_path)])
    try:
        proc.initialize()
        ids = [
            proc.send_call_async(
                "replay",
                {
                    "manifest_path": str(manifest),
                    "claim": cid,
                    "no_execute": True,
                    "sidecar": str(sidecar),
                },
            )
            for cid in ("claim-A", "claim-B")
        ]
        frames = {}
        for _ in ids:
            f = proc.recv()
            frames[f["id"]] = f
        for rid in ids:
            assert frames[rid]["result"]["isError"] is False, frames[rid]
        data = json.loads(sidecar.read_text())
        assert set(data.keys()) == {"claim-A", "claim-B"}, data
    finally:
        proc.close()


def test_output_dir_symlink_escape_denied_and_no_write(tmp_path: Path) -> None:
    """An output_dir whose existing ancestor is a symlink escaping the
    allow-root is rejected BEFORE any directory is created outside."""
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    repo = allowed / "repo"
    repo.mkdir()
    (repo / "pyproject.toml").write_text('[project]\nname = "x"\n')
    outside = tmp_path / "outside"
    outside.mkdir()
    (allowed / "escape").symlink_to(outside)  # existing symlink component

    proc = McpProc(["--allow-root", str(allowed)])
    try:
        proc.initialize()
        frame = proc.call(
            "extract_metadata",
            {"repo": str(repo), "output_dir": str(allowed / "escape" / "gen")},
        )
        assert "error" in frame, frame
        assert frame["error"]["code"] == -32001
        # Nothing must have been written through the symlink.
        assert not (outside / "gen").exists()
    finally:
        proc.close()


def test_replay_rejects_client_binary_override(tmp_path: Path) -> None:
    """typed_trust_binary must NOT be honoured from tool input (RCE gate):
    it is absent from the schema and ignored if injected."""
    manifest = _write_measurement_manifest(tmp_path)
    proc = McpProc(["--allow-root", str(tmp_path)])
    try:
        proc.initialize()
        # schema check
        proc._id += 1
        proc._send({"jsonrpc": "2.0", "id": proc._id, "method": "tools/list", "params": {}})
        listed = proc.recv()
        replay_tool = next(t for t in listed["result"]["tools"] if t["name"] == "replay")
        assert "typed_trust_binary" not in replay_tool["inputSchema"]["properties"]

        # injecting it does not execute anything — dry-run succeeds, arg ignored
        frame = proc.call(
            "replay",
            {
                "manifest_path": str(manifest),
                "claim": "claim-A",
                "dry_run": True,
                "typed_trust_binary": "/bin/definitely-not-a-real-binary",
            },
        )
        assert frame["result"]["isError"] is False
        assert _result_payload(frame)["dry_run"] is True
    finally:
        proc.close()
