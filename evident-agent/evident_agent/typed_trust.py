"""Subprocess wrapper for the ``typed-trust`` CLI."""

from __future__ import annotations

import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


@dataclass
class TypedTrustResult:
    exit_code: int
    stdout: str
    stderr: str


def _agent_package_dir() -> Path:
    """Directory containing this module."""
    return Path(__file__).resolve().parent


def _candidate_typed_trust_dirs() -> list[Path]:
    """Walk up from the agent package looking for a sibling
    ``typed-trust/target/{release,debug}`` directory. This is the
    development layout where evident-agent and typed-trust live in the
    same repo. Returns an ordered list of candidate binaries.
    """
    out: list[Path] = []
    cursor = _agent_package_dir()
    for _ in range(6):  # walk up at most six levels
        target = cursor / "typed-trust" / "target"
        if target.is_dir():
            for build in ("release", "debug"):
                p = target / build / "typed-trust"
                if p.is_file():
                    out.append(p)
        cursor = cursor.parent
        if cursor == cursor.parent:
            break
    return out


def find_binary(override: Optional[str] = None) -> str:
    """Locate the typed-trust binary.

    Order:
    1. explicit override
    2. ``TYPED_TRUST_BIN`` environment variable
    3. ``typed-trust`` on PATH (``shutil.which``)
    4. sibling ``typed-trust/target/{release,debug}/typed-trust``
       found by walking up from this package's directory (development
       layout — agent and typed-trust in the same repo)
    5. fallback to the literal string ``typed-trust`` and hope subprocess
       finds it
    """
    if override:
        return override
    env_override = os.environ.get("TYPED_TRUST_BIN")
    if env_override:
        return env_override
    path_hit = shutil.which("typed-trust")
    if path_hit:
        return path_hit
    for c in _candidate_typed_trust_dirs():
        return str(c)
    return "typed-trust"


def run(
    manifest_path: Path,
    sidecar_path: Optional[Path] = None,
    format: str = "json",
    claim_filter: Optional[str] = None,
    binary: Optional[str] = None,
    extra_args: Optional[list[str]] = None,
) -> TypedTrustResult:
    """Invoke typed-trust with the populated sidecar and return the rendered output.

    ``extra_args`` are inserted after the format and last-verified
    flags but before the positional manifest path. Used by the
    Phase 2a ``review`` subcommand to thread
    ``--review-events-sidecar <path>`` through.
    """
    argv = [find_binary(binary), "--format", format]
    if sidecar_path is not None:
        argv.extend(["--last-verified-sidecar", str(sidecar_path)])
    if extra_args:
        argv.extend(extra_args)
    argv.append(str(manifest_path))
    if claim_filter is not None:
        argv.append(claim_filter)

    proc = subprocess.run(argv, capture_output=True, text=True, check=False)
    return TypedTrustResult(
        exit_code=proc.returncode,
        stdout=proc.stdout,
        stderr=proc.stderr,
    )
