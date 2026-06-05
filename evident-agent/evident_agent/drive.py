"""The ``evident-agent drive`` launcher.

Wires the two MCP servers (read: ``typed-trust-mcp``; exec:
``evident-agent-mcp``) and the canonical driver prompt into the chosen
runtime — Claude Code or Codex — then hands off the native terminal.

Design: **invocation-local, zero persistent global state** (Codex review).
- Claude: an ephemeral ``--mcp-config`` file + ``--strict-mcp-config`` +
  ``--append-system-prompt-file`` (no user/project MCP leakage).
- Codex: ``codex -C <session> --ignore-user-config -c 'mcp_servers...'`` with
  the rendered prompt as ``<session>/AGENTS.md`` — never the user's tree, never
  ``codex mcp add`` (which mutates ``~/.codex/config.toml``). Codex runs the
  **interactive** TUI (not ``codex exec``), because MCP tool calls there go
  through a human approval prompt — the intended safety gate.

Pure builders (`prepare_claude`, `prepare_codex`, `compose_prompt`,
`resolve_servers`) are separated from the spawn so they can be unit-tested.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import List, Optional, Tuple


class DriveError(Exception):
    """A launch precondition failed (missing binary/flag/prompt)."""


# ---------------------------------------------------------------------
# Resolution
# ---------------------------------------------------------------------
def find_typed_trust_mcp() -> Optional[str]:
    """Locate the ``typed-trust-mcp`` binary: PATH first, then a sibling
    ``typed-trust/target/{release,debug}`` build (dev layout)."""
    hit = shutil.which("typed-trust-mcp")
    if hit:
        return hit
    cursor = Path(__file__).resolve().parent
    for _ in range(6):
        for build in ("release", "debug"):
            p = cursor / "typed-trust" / "target" / build / "typed-trust-mcp"
            if p.is_file():
                return str(p)
        if cursor == cursor.parent:
            break
        cursor = cursor.parent
    return None


def find_driver_prompt() -> Optional[Path]:
    """Locate the canonical ``EVIDENT_DRIVER.md`` by walking up from the
    package directory (repo root in the dev layout)."""
    cursor = Path(__file__).resolve().parent
    for _ in range(6):
        p = cursor / "EVIDENT_DRIVER.md"
        if p.is_file():
            return p
        if cursor == cursor.parent:
            break
        cursor = cursor.parent
    return None


def resolve_servers(root: Path) -> Tuple[str, List[str], str, List[str]]:
    """Return (typed_trust_cmd, typed_trust_args, exec_cmd, exec_base_args).

    The exec server runs via ``python -m evident_agent.mcp`` (always present,
    no console-script install required)."""
    tt = find_typed_trust_mcp()
    if tt is None:
        raise DriveError(
            "typed-trust-mcp not found. Build it (cd typed-trust && cargo build "
            "--release) or put it on PATH."
        )
    tt_args = ["--allow-manifest", str(root)]
    exec_cmd = sys.executable
    exec_args = ["-m", "evident_agent.mcp"]
    return tt, tt_args, exec_cmd, exec_args


def _exec_allow_args(root: Path, allow_docker: bool, allow_extract: bool) -> List[str]:
    args = ["--allow-root", str(root)]
    if allow_docker:
        args.append("--allow-docker")
    if allow_extract:
        args.append("--allow-extract")
    return args


# ---------------------------------------------------------------------
# Prompt composition (canonical prompt + runtime context)
# ---------------------------------------------------------------------
def compose_prompt(
    driver_text: str, root: Path, allow_docker: bool, allow_extract: bool
) -> str:
    """Prepend a small runtime-context header so the agent knows the corpus
    root and which capabilities are live. The canonical prompt is otherwise
    verbatim."""
    header = (
        "# Runtime context\n\n"
        f"- **EVIDENT corpus root:** `{root}` — manifests live under this path; "
        "pass absolute paths to the MCP tools.\n"
        "- **Read tools:** `typed-trust-mcp`. **Exec tools:** `evident-agent-mcp`.\n"
        f"- **Execution capabilities:** docker={'on' if allow_docker else 'off'}, "
        f"extract={'on' if allow_extract else 'off'}. When off, `replay`/`extract_*` "
        "return a dry-run (Inconclusive), not evidence.\n\n"
        "---\n\n"
    )
    return header + driver_text


# ---------------------------------------------------------------------
# Per-runtime argv builders (write config into `session`, return argv)
# ---------------------------------------------------------------------
def prepare_claude(
    session: Path,
    *,
    tt_cmd: str,
    tt_args: List[str],
    exec_cmd: str,
    exec_args: List[str],
    prompt_text: str,
) -> List[str]:
    mcp_config = {
        "mcpServers": {
            "typed-trust": {"command": tt_cmd, "args": tt_args},
            "evident-exec": {"command": exec_cmd, "args": exec_args},
        }
    }
    cfg_path = session / "mcp.json"
    cfg_path.write_text(json.dumps(mcp_config, indent=2), encoding="utf-8")
    prompt_path = session / "EVIDENT_DRIVER.md"
    prompt_path.write_text(prompt_text, encoding="utf-8")
    return [
        "claude",
        "--mcp-config",
        str(cfg_path),
        "--strict-mcp-config",
        "--append-system-prompt-file",
        str(prompt_path),
    ]


def prepare_codex(
    session: Path,
    *,
    tt_cmd: str,
    tt_args: List[str],
    exec_cmd: str,
    exec_args: List[str],
    prompt_text: str,
) -> List[str]:
    # AGENTS.md in the ephemeral session dir (read via `-C`), never the user's tree.
    (session / "AGENTS.md").write_text(prompt_text, encoding="utf-8")
    return [
        "codex",
        "-C",
        str(session),
        "--ignore-user-config",
        "-c",
        f"mcp_servers.typed_trust.command={json.dumps(tt_cmd)}",
        "-c",
        f"mcp_servers.typed_trust.args={json.dumps(tt_args)}",
        "-c",
        f"mcp_servers.evident_exec.command={json.dumps(exec_cmd)}",
        "-c",
        f"mcp_servers.evident_exec.args={json.dumps(exec_args)}",
    ]


def _feature_check(model: str) -> None:
    """Confirm the runtime binary exists and supports the flags we depend on;
    fail clearly on an unsupported version."""
    if shutil.which(model) is None:
        raise DriveError(f"{model!r} is not on PATH.")
    try:
        help_text = subprocess.run(
            [model, "--help"], capture_output=True, text=True, timeout=20
        ).stdout
    except Exception as exc:  # noqa: BLE001
        raise DriveError(f"could not run `{model} --help`: {exc}")
    required = {
        "claude": ["--mcp-config", "--strict-mcp-config", "--append-system-prompt-file"],
        "codex": ["--ignore-user-config", "-c", "-C"],
    }[model]
    missing = [f for f in required if f not in help_text]
    if missing:
        raise DriveError(
            f"{model} is missing required flag(s) {missing}; update the CLI."
        )


def run_drive(
    *,
    model: str,
    root: Path,
    driver_prompt: Optional[Path] = None,
    allow_docker: bool = False,
    allow_extract: bool = False,
) -> int:
    """Compose the invocation-local config + prompt and hand off to the chosen
    runtime's interactive terminal. Returns the child's exit code."""
    if model not in ("claude", "codex"):
        raise DriveError(f"unknown model {model!r} (expected claude|codex)")
    root = root.resolve()
    if not root.is_dir():
        raise DriveError(f"corpus root {root} is not a directory")

    prompt_path = driver_prompt or find_driver_prompt()
    if prompt_path is None or not Path(prompt_path).is_file():
        raise DriveError(
            "EVIDENT_DRIVER.md not found; pass --driver-prompt explicitly."
        )

    _feature_check(model)
    tt_cmd, tt_args, exec_cmd, exec_base = resolve_servers(root)
    exec_args = exec_base + _exec_allow_args(root, allow_docker, allow_extract)
    prompt_text = compose_prompt(
        Path(prompt_path).read_text(encoding="utf-8"), root, allow_docker, allow_extract
    )

    with tempfile.TemporaryDirectory(prefix="evident-drive-") as tmp:
        session = Path(tmp)
        if model == "claude":
            argv = prepare_claude(
                session,
                tt_cmd=tt_cmd,
                tt_args=tt_args,
                exec_cmd=exec_cmd,
                exec_args=exec_args,
                prompt_text=prompt_text,
            )
        else:
            argv = prepare_codex(
                session,
                tt_cmd=tt_cmd,
                tt_args=tt_args,
                exec_cmd=exec_cmd,
                exec_args=exec_args,
                prompt_text=prompt_text,
            )
        # Inherit the terminal so the native TUI works; clean up on exit.
        return subprocess.run(argv).returncode
