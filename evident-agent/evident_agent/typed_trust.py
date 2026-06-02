"""Subprocess wrapper for the ``typed-trust`` CLI."""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


@dataclass
class TypedTrustResult:
    exit_code: int
    stdout: str
    stderr: str


def find_binary(override: Optional[str] = None) -> str:
    """Locate the typed-trust binary.

    Order: explicit override → PATH → repo-relative debug build.
    Returns the resolved path or "typed-trust" as a last resort.
    """
    if override:
        return override
    # Look for a debug build next to the agent's source tree.
    candidates = [
        Path("/scratch/TMAlign/evident/typed-trust/target/debug/typed-trust"),
        Path("/scratch/TMAlign/evident/typed-trust/target/release/typed-trust"),
    ]
    for c in candidates:
        if c.is_file():
            return str(c)
    return "typed-trust"


def run(
    manifest_path: Path,
    sidecar_path: Optional[Path] = None,
    format: str = "json",
    claim_filter: Optional[str] = None,
    binary: Optional[str] = None,
) -> TypedTrustResult:
    """Invoke typed-trust with the populated sidecar and return the rendered output."""
    argv = [find_binary(binary), "--format", format]
    if sidecar_path is not None:
        argv.extend(["--last-verified-sidecar", str(sidecar_path)])
    argv.append(str(manifest_path))
    if claim_filter is not None:
        argv.append(claim_filter)

    proc = subprocess.run(argv, capture_output=True, text=True, check=False)
    return TypedTrustResult(
        exit_code=proc.returncode,
        stdout=proc.stdout,
        stderr=proc.stderr,
    )
