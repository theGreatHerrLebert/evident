"""Tool definitions + synchronous dispatch for ``evident-agent-mcp``.

Four exec tools wrapping existing CLI logic:

- ``replay``           → :func:`evident_agent.replay.run_replay`
- ``extract_repo``     → ``extract.cli.run_extract_repo``
- ``extract_paper``    → ``extract.cli.run_extract_paper``
- ``extract_metadata`` → ``extract.metadata.run_extract_metadata``

Every path argument is authorized through the allow-list policy BEFORE
any side effect. ``replay`` (docker) and ``extract_*`` (Anthropic API)
are capability-gated: without ``--allow-docker`` / ``--allow-extract``
they refuse real execution and run dry. Errors split into two tiers per
:mod:`evident_agent.mcp.errors`.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

from .errors import ToolError
from .policy import AllowListPathPolicy, PolicyDenied


@dataclass
class ServerState:
    policy: AllowListPathPolicy
    allow_docker: bool = False
    allow_extract: bool = False


# ---------------------------------------------------------------------
# Argument helpers (mirror handlers.rs:152-169)
# ---------------------------------------------------------------------
def arg_str(args: dict, key: str) -> str:
    val = args.get(key)
    if not isinstance(val, str) or not val:
        raise ToolError.invalid_params(f"missing or non-string required arg {key!r}")
    return val


def arg_str_opt(args: dict, key: str) -> Optional[str]:
    val = args.get(key)
    if val is None:
        return None
    if not isinstance(val, str):
        raise ToolError.invalid_params(f"arg {key!r} must be a string")
    return val


def arg_bool_opt(args: dict, key: str, default: bool = False) -> bool:
    val = args.get(key)
    if val is None:
        return default
    if not isinstance(val, bool):
        raise ToolError.invalid_params(f"arg {key!r} must be a boolean")
    return val


def arg_float_opt(args: dict, key: str, default: float) -> float:
    val = args.get(key)
    if val is None:
        return default
    if isinstance(val, bool) or not isinstance(val, (int, float)):
        raise ToolError.invalid_params(f"arg {key!r} must be a number")
    return float(val)


def arg_int_opt(args: dict, key: str, default: int) -> int:
    val = args.get(key)
    if val is None:
        return default
    if isinstance(val, bool) or not isinstance(val, int):
        raise ToolError.invalid_params(f"arg {key!r} must be an integer")
    return val


def _authorize(policy: AllowListPathPolicy, path: str, *, allow_missing: bool = False) -> Path:
    try:
        return policy.check(path, allow_missing=allow_missing)
    except PolicyDenied as exc:
        raise ToolError.unauthorized(exc.reason)


def _authorize_writable_dir(policy: AllowListPathPolicy, path: str) -> Path:
    """Authorize a (possibly missing) output dir, materialize it, then
    STRICT-recheck — so the writer operates inside an already-validated,
    existing directory rather than a path that could be symlink-swapped
    after a missing-path check (Codex Critical #2). Residual TOCTOU after
    this point would require replacing a just-created real directory; for
    operator-chosen --allow-root trees that is acceptable. A hardened
    dir-fd/openat2 writer is deferred."""
    canonical = _authorize(policy, path, allow_missing=True)
    os.makedirs(canonical, exist_ok=True)
    return _authorize(policy, str(canonical))  # strict realpath recheck


def _authorize_writable_file(policy: AllowListPathPolicy, path: str) -> Path:
    """Authorize a (possibly missing) output file by materializing and
    strict-rechecking its parent directory before any write."""
    canonical = _authorize(policy, path, allow_missing=True)
    os.makedirs(canonical.parent, exist_ok=True)
    _authorize(policy, str(canonical.parent))  # strict realpath recheck
    return canonical


# ---------------------------------------------------------------------
# Tool definitions
# ---------------------------------------------------------------------
_MAX_LOG_LINES = 500


def tool_definitions() -> list[dict]:
    """Return the MCP tool definitions (name/description/inputSchema)."""
    return [
        {
            "name": "replay",
            "description": (
                "Replay measurement claims from a manifest: runs "
                "`docker run <image> replay <claim_id>`, scores the artifact, "
                "and writes the last_verified.json sidecar. SAFETY: actually "
                "executes Docker — requires the server's --allow-docker; without "
                "it the call is forced to dry-run. `manifest_path`, `sidecar`, and "
                "`source_dir` must lie under an --allow-root path. Use `dry_run` to "
                "preview the docker command; `no_execute` to score existing "
                "artifacts only. `render` invokes the server's own trusted "
                "typed-trust binary (the binary is NOT client-selectable)."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "manifest_path": {"type": "string", "description": "Path to evident.yaml"},
                    "claim": {"type": "string", "description": "Run only this claim id"},
                    "image": {"type": "string", "description": "Docker image (default proteon-evident:latest)"},
                    "source_dir": {"type": "string", "description": "Override per-claim source dir"},
                    "budget": {"type": "number", "description": "Per-claim timeout seconds (default 600)"},
                    "sidecar": {"type": "string", "description": "Sidecar path (default manifest.parent/last_verified.json)"},
                    "dry_run": {"type": "boolean"},
                    "no_execute": {"type": "boolean"},
                    "render": {"type": "string", "enum": ["json", "md", "html", "mermaid"]},
                },
                "required": ["manifest_path"],
            },
        },
        {
            "name": "extract_repo",
            "description": (
                "Extract draft (tier:research) claims from a local repo's README/"
                "docs via the Anthropic API. SAFETY: makes a model call — requires "
                "the server's --allow-extract; without it the call is forced to "
                "dry-run (walker only, no API). `repo_path` must exist under an "
                "--allow-root; `output_dir` must resolve under one."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo_path": {"type": "string"},
                    "output_dir": {"type": "string"},
                    "project": {"type": "string"},
                    "model": {"type": "string"},
                    "dry_run": {"type": "boolean"},
                    "max_tokens": {"type": "integer"},
                },
                "required": ["repo_path", "output_dir"],
            },
        },
        {
            "name": "extract_paper",
            "description": (
                "Extract draft (tier:research) claims from a paper (markdown/PDF) "
                "via the Anthropic API. SAFETY: makes a model call — requires the "
                "server's --allow-extract; without it the call is forced to dry-run. "
                "`paper_path` must exist under an --allow-root; `output_dir` under one."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "paper_path": {"type": "string"},
                    "output_dir": {"type": "string"},
                    "source_id": {"type": "string"},
                    "project": {"type": "string"},
                    "model": {"type": "string"},
                    "dry_run": {"type": "boolean"},
                    "max_tokens": {"type": "integer"},
                },
                "required": ["paper_path", "output_dir"],
            },
        },
        {
            "name": "extract_metadata",
            "description": (
                "Deterministically extract metadata_compatibility claims from a "
                "repo's pyproject.toml / Cargo.toml / package.json. No model call, "
                "no docker — always safe. `repo` must exist under an --allow-root; "
                "`output_dir` must resolve under one."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo": {"type": "string"},
                    "output_dir": {"type": "string"},
                    "project": {"type": "string"},
                },
                "required": ["repo", "output_dir"],
            },
        },
    ]


# ---------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------
def dispatch_sync(state: ServerState, name: str, arguments: Any) -> dict:
    """Run one tool synchronously; return a JSON-serializable result
    dict or raise :class:`ToolError`. Called inside a worker thread."""
    if not isinstance(arguments, dict):
        raise ToolError.invalid_params("arguments must be an object")
    if name == "replay":
        return _replay(state, arguments)
    if name == "extract_repo":
        return _extract_repo(state, arguments)
    if name == "extract_paper":
        return _extract_paper(state, arguments)
    if name == "extract_metadata":
        return _extract_metadata(state, arguments)
    raise ToolError.method_not_found(f"unknown tool {name!r}")


def _replay(state: ServerState, args: dict) -> dict:
    from .. import replay as replay_mod

    manifest_path = _authorize(state.policy, arg_str(args, "manifest_path"))
    claim = arg_str_opt(args, "claim")
    image = arg_str_opt(args, "image") or "proteon-evident:latest"
    budget = arg_float_opt(args, "budget", 600.0)
    dry_run = arg_bool_opt(args, "dry_run")
    no_execute = arg_bool_opt(args, "no_execute")
    render = arg_str_opt(args, "render")
    if render is not None and render not in ("json", "md", "html", "mermaid"):
        raise ToolError.invalid_params("render must be one of json|md|html|mermaid")

    source_dir_arg = arg_str_opt(args, "source_dir")
    source_dir = _authorize(state.policy, source_dir_arg) if source_dir_arg else None

    sidecar_arg = arg_str_opt(args, "sidecar") or str(
        manifest_path.parent / "last_verified.json"
    )
    sidecar_path = _authorize_writable_file(state.policy, sidecar_arg)

    # Capability gate (hard-off): no --allow-docker → force dry-run.
    capability_gated = False
    if not state.allow_docker and not dry_run and not no_execute:
        dry_run = True
        capability_gated = True

    log: list[str] = []

    def collect(msg, *, err: bool = False, nl: bool = True) -> None:
        if len(log) < _MAX_LOG_LINES:
            log.append(("[stderr] " if err else "") + str(msg))

    try:
        result = replay_mod.run_replay(
            manifest_path=manifest_path,
            claim_filter=claim,
            image=image,
            source_dir=source_dir,
            budget=budget,
            sidecar_path=sidecar_path,
            dry_run=dry_run,
            no_execute=no_execute,
            render=render,
            typed_trust_binary=None,  # NOT client-selectable (RCE gate, Codex Critical #1)
            on_event=collect,
        )
    except replay_mod.NoClaimsMatched as exc:
        raise ToolError.data(str(exc))
    except replay_mod.RenderFailed as exc:
        raise ToolError.data(f"typed-trust render failed (exit {exc.exit_code}): {exc.stderr}")

    payload = {
        "sidecar_path": str(result.sidecar_path) if result.sidecar_path else None,
        "dry_run": result.dry_run,
        "capability_gated": capability_gated,
        "new_count": result.new_count,
        "total_count": result.total_count,
        "claims": [
            {
                "claim_id": c.claim_id,
                "exit_code": c.exit_code,
                "duration_s": c.duration_s,
                "observed": c.observed,
                "skipped_execution": c.skipped_execution,
                "outcome": c.outcome,
            }
            for c in result.claims
        ],
        "rendered": result.rendered,
        "log": log,
    }

    # Infra/timeout outcomes are recoverable tool errors (tier-2), but we
    # still want the structured detail visible to the model.
    bad = [c for c in result.claims if c.outcome in ("infrastructure_error", "timed_out")]
    if bad:
        import json as _json

        raise ToolError.data(
            "replay hit "
            + ", ".join(sorted({c.outcome for c in bad}))
            + f" on {len(bad)} claim(s): "
            + _json.dumps(payload)
        )
    return payload


def _extract_repo(state: ServerState, args: dict) -> dict:
    from ..extract import cli as extract_cli

    repo_path = _authorize(state.policy, arg_str(args, "repo_path"))
    output_dir = _authorize_writable_dir(state.policy, arg_str(args, "output_dir"))
    project = arg_str_opt(args, "project")
    model = arg_str_opt(args, "model") or "claude-opus-4-7"
    dry_run = arg_bool_opt(args, "dry_run")
    max_tokens = arg_int_opt(args, "max_tokens", 4096)

    if not state.allow_extract:
        dry_run = True

    try:
        result = extract_cli.run_extract_repo(
            repo_path=repo_path,
            output_dir=output_dir,
            project=project,
            model=model,
            dry_run=dry_run,
            max_tokens=max_tokens,
        )
    except extract_cli.ExtractTransportError as exc:
        raise ToolError.data(f"extraction transport error: {exc}")
    except Exception as exc:  # missing SDK / API failure → recoverable
        raise ToolError.data(f"extract_repo failed: {exc}")

    return _extraction_payload(output_dir, dry_run, result, capability_gated=not state.allow_extract)


def _extract_paper(state: ServerState, args: dict) -> dict:
    from ..extract import cli as extract_cli

    paper_path = _authorize(state.policy, arg_str(args, "paper_path"))
    output_dir = _authorize_writable_dir(state.policy, arg_str(args, "output_dir"))
    source_id = arg_str_opt(args, "source_id")
    project = arg_str_opt(args, "project")
    model = arg_str_opt(args, "model") or "claude-opus-4-7"
    dry_run = arg_bool_opt(args, "dry_run")
    max_tokens = arg_int_opt(args, "max_tokens", 4096)

    if not state.allow_extract:
        dry_run = True

    try:
        result = extract_cli.run_extract_paper(
            paper_path=paper_path,
            output_dir=output_dir,
            project=project,
            source_id=source_id,
            model=model,
            dry_run=dry_run,
            max_tokens=max_tokens,
        )
    except extract_cli.PaperExtractionSkipped as exc:
        raise ToolError.data(
            f"paper walker skipped source {exc.walked.source_id!r} "
            "(pdftotext missing / empty / unsupported)"
        )
    except extract_cli.ExtractTransportError as exc:
        raise ToolError.data(f"extraction transport error: {exc}")
    except Exception as exc:
        raise ToolError.data(f"extract_paper failed: {exc}")

    return _extraction_payload(output_dir, dry_run, result, capability_gated=not state.allow_extract)


def _extract_metadata(state: ServerState, args: dict) -> dict:
    from ..extract import metadata as mdwalker

    repo_path = _authorize(state.policy, arg_str(args, "repo"))
    output_dir = _authorize_writable_dir(state.policy, arg_str(args, "output_dir"))
    project = arg_str_opt(args, "project")

    result = mdwalker.run_extract_metadata(repo_path, output_dir, project=project)
    return {
        "output_dir": str(output_dir),
        "source_id": result.source_id,
        "source_sha": result.source_sha,
        "emitted_claims": len(result.claims),
        "skipped_files": len(result.skipped_files),
    }


def _extraction_payload(output_dir: Path, dry_run: bool, result, *, capability_gated: bool) -> dict:
    """Shared shape for extract_repo / extract_paper (result is
    ExtractionResult or None on dry-run)."""
    if result is None:  # dry-run
        return {
            "output_dir": str(output_dir),
            "dry_run": True,
            "capability_gated": capability_gated,
            "source_id": None,
            "source_sha": None,
            "accepted_claims": 0,
            "rejections": 0,
        }
    return {
        "output_dir": str(output_dir),
        "dry_run": False,
        "capability_gated": capability_gated,
        "source_id": result.source_id,
        "source_sha": result.source_sha,
        "accepted_claims": len(result.claims),
        "rejections": len(result.rejections),
    }
