"""Docker invocation for per-claim replay.

Delegates to proteon's existing Docker image (built from
``proteon/evident/Dockerfile``), which ships a ``replay <claim-id>``
entrypoint that runs the manifest's ``evidence.command`` with all
oracle binaries available.

The agent never reimplements the subprocess management — the image's
``replay`` entrypoint runs the command inside the container. We just
orchestrate ``docker run`` calls and capture exit codes.
"""

from __future__ import annotations

import re
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


# A docker image reference: [registry[:port]/]name[/name...][:tag][@sha256:digest].
# Anything else, in particular a value starting with "-", would be parsed by
# `docker run` as an option (Codex MCP review, High #3).
_COMPONENT = r"[a-z0-9]+(?:(?:[._]|__|-+)[a-z0-9]+)*"
_IMAGE_RE = re.compile(
    rf"^(?:{_COMPONENT}(?::[0-9]+)?/)?{_COMPONENT}(?:/{_COMPONENT})*"
    r"(?::[A-Za-z0-9_][A-Za-z0-9_.-]{0,127})?(?:@sha256:[0-9a-f]{64})?$"
)


# Network modes replay may use. "host" is the historical default (some claims
# reach local registries or pip mirrors); "none" isolates the container.
NETWORK_MODES = ("host", "bridge", "none")


def validate_image(image: str) -> str:
    """Return ``image`` if it is a plain docker image reference, else raise."""
    if not isinstance(image, str) or not _IMAGE_RE.match(image):
        raise ValueError(f"not a valid docker image reference: {image!r}")
    return image


def build_command(
    image: str,
    claim_id: str,
    source_dir: Path,
    extra_volumes: Optional[List[str]] = None,
    network: str = "host",
    name: Optional[str] = None,
) -> List[str]:
    """Construct the ``docker run`` argv for a single claim's replay.

    The source dir is mounted at ``/work`` inside the container; the
    proteon entrypoint cd's there and runs the cited command. Network
    defaults to ``host`` because some claims hit local registries or
    cached pip mirrors during execution.
    """
    validate_image(image)
    if network not in NETWORK_MODES:
        raise ValueError(f"network must be one of {NETWORK_MODES}, got {network!r}")
    cmd = [
        "docker",
        "run",
        "--rm",
        # Least privilege that does not change what a replay can do: no
        # setuid escalation inside the container, bounded process count.
        "--security-opt",
        "no-new-privileges",
        "--pids-limit",
        "4096",
        *(["--name", name] if name else []),
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
    network: str = "host",
) -> DockerResult:
    """Run one claim's replay via docker.

    Returns a ``DockerResult`` with exit code, duration, and the tails
    of stdout/stderr. ``dry_run=True`` skips execution and returns a
    placeholder.

    Output is spooled to temporary files and only the tails are read
    back, so a container that floods stdout cannot exhaust memory. Each
    container gets a unique name; on timeout the docker client is killed
    *and* the container is force-removed, since killing the client does
    not stop a daemon-managed container (Codex MCP review, High #5).
    """
    import tempfile
    import time
    import uuid

    name = f"evident-replay-{uuid.uuid4().hex[:12]}"
    argv = build_command(image, claim_id, source_dir, extra_volumes, network=network, name=name)
    if dry_run:
        return DockerResult(
            claim_id=claim_id,
            exit_code=0,
            duration_s=0.0,
            stdout_tail=f"dry-run: {' '.join(argv)}",
            stderr_tail="",
        )

    start = time.monotonic()
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        proc = subprocess.Popen(argv, stdout=out, stderr=err, stdin=subprocess.DEVNULL)
        timed_out = False
        try:
            exit_code = proc.wait(timeout=budget_seconds)
        except subprocess.TimeoutExpired:
            timed_out = True
            proc.kill()
            proc.wait()
            _remove_container(name)
            exit_code = 124  # conventional timeout exit code
        duration = time.monotonic() - start
        stdout_tail = _file_tail(out, tail_bytes)
        stderr_tail = _file_tail(err, tail_bytes)
    if timed_out:
        stderr_tail += f"\n[TIMEOUT after {budget_seconds}s; container {name} removed]"
    return DockerResult(
        claim_id=claim_id,
        exit_code=exit_code,
        duration_s=duration,
        stdout_tail=stdout_tail,
        stderr_tail=stderr_tail,
        timed_out=timed_out,
    )


def _remove_container(name: str) -> None:
    """Best-effort ``docker rm -f``; failure must not mask the timeout."""
    try:
        subprocess.run(
            ["docker", "rm", "-f", name],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=60,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        pass


def _file_tail(handle, n: int) -> str:
    """Read at most the last ``n`` bytes of a spooled output file."""
    handle.seek(0, 2)
    size = handle.tell()
    handle.seek(max(0, size - n))
    text = handle.read().decode("utf-8", errors="replace")
    return ("...[truncated]...\n" + text) if size > n else text


def _tail(text: str, n: int) -> str:
    if len(text) <= n:
        return text
    return "...[truncated]...\n" + text[-n:]
