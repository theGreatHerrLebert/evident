"""Docker invocation for per-claim replay.

Delegates to proteon's existing Docker image (built from
``proteon/evident/Dockerfile``), which ships a ``replay <claim-id>``
entrypoint that runs the manifest's ``evidence.command`` with all
oracle binaries available.

The agent never reimplements the subprocess management — that's what
the framework's ``workflow/evident.py::_run_command`` does inside the
container. We just orchestrate ``docker run`` calls and capture exit
codes.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional


@dataclass
class DockerResult:
    """Outcome of a single ``docker run`` invocation."""

    claim_id: str
    exit_code: int
    duration_s: float
    stdout_tail: str
    stderr_tail: str
    timed_out: bool = False


def build_command(
    image: str,
    claim_id: str,
    source_dir: Path,
    extra_volumes: Optional[List[str]] = None,
    network: str = "host",
) -> List[str]:
    """Construct the ``docker run`` argv for a single claim's replay.

    The source dir is mounted at ``/work`` inside the container; the
    proteon entrypoint cd's there and runs the cited command. Network
    defaults to ``host`` because some claims hit local registries or
    cached pip mirrors during execution.
    """
    cmd = [
        "docker",
        "run",
        "--rm",
        "-v",
        f"{source_dir.resolve()}:/work",
        "-w",
        "/work",
        "--network",
        network,
    ]
    for vol in extra_volumes or []:
        cmd.extend(["-v", vol])
    cmd.extend([image, "replay", claim_id])
    return cmd


def run(
    image: str,
    claim_id: str,
    source_dir: Path,
    budget_seconds: float = 600.0,
    tail_bytes: int = 2048,
    extra_volumes: Optional[List[str]] = None,
    dry_run: bool = False,
) -> DockerResult:
    """Run one claim's replay via docker.

    Returns a ``DockerResult`` with exit code, duration, and the tails
    of stdout/stderr. ``dry_run=True`` skips execution and returns a
    placeholder.
    """
    import time

    argv = build_command(image, claim_id, source_dir, extra_volumes)
    if dry_run:
        return DockerResult(
            claim_id=claim_id,
            exit_code=0,
            duration_s=0.0,
            stdout_tail=f"dry-run: {' '.join(argv)}",
            stderr_tail="",
        )

    start = time.monotonic()
    try:
        proc = subprocess.run(
            argv,
            capture_output=True,
            timeout=budget_seconds,
            text=True,
            check=False,
        )
        duration = time.monotonic() - start
        return DockerResult(
            claim_id=claim_id,
            exit_code=proc.returncode,
            duration_s=duration,
            stdout_tail=_tail(proc.stdout, tail_bytes),
            stderr_tail=_tail(proc.stderr, tail_bytes),
        )
    except subprocess.TimeoutExpired as e:
        duration = time.monotonic() - start
        stdout = e.stdout.decode("utf-8", errors="replace") if e.stdout else ""
        stderr = e.stderr.decode("utf-8", errors="replace") if e.stderr else ""
        return DockerResult(
            claim_id=claim_id,
            exit_code=124,  # conventional timeout exit code
            duration_s=duration,
            stdout_tail=_tail(stdout, tail_bytes),
            stderr_tail=_tail(stderr, tail_bytes) + f"\n[TIMEOUT after {budget_seconds}s]",
            timed_out=True,
        )


def _tail(text: str, n: int) -> str:
    if len(text) <= n:
        return text
    return "...[truncated]...\n" + text[-n:]
