"""Phase 5 PR5: repo walker.

Reads a local git repo's README/CHANGELOG/docs files, redacts
external citations (DOIs, arXiv links, preprint URLs, bibliography
sections, post-bibliography inline citation markers), and assembles
the redacted text for the model.

The walker is the load-bearing layer for the **transitive-source
rule**: a repo README citing a paper must not cause the extractor
to claim things FROM that paper. We enforce this by removing the
citations from `assembled_text_for_model` rather than relying on
the system prompt alone.

See ``EVIDENT_PHASE5_PR5_DRAFT.md`` v3.

PR6 note: citation redaction moved into ``extract/redaction.py``
so the paper walker can share it. ``redact`` / ``Redaction`` and
the ``REDACTION_*`` constants are re-exported below for PR5-era
imports.
"""

from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

# Re-export the redaction surface so PR5-era callers keep working.
# Codex F-PR6-CR3 (P2): preserve import compatibility when shared
# code moves into a new module.
from .redaction import (  # noqa: F401
    Redaction,
    redact,
    REDACTION_ARXIV,
    REDACTION_BIBLIOGRAPHY,
    REDACTION_DOI,
    REDACTION_INLINE,
    REDACTION_PREPRINT,
)


# ---------------------------------------------------------------------
# Public dataclasses
# ---------------------------------------------------------------------


@dataclass
class SkippedFile:
    """A file the walker considered but did not include."""

    path: str
    reason: str          # "binary" | "too_large" | "not_allowlisted" | "symlink_outside_repo"
    size_bytes: int | None


@dataclass
class SourceSection:
    """One file's contribution to the model's input."""

    path: str            # repo-relative
    text: str            # POST-redaction text (the model sees this)
    text_raw: str        # PRE-redaction text (for the audit)
    kind: str            # "readme" | "changelog" | "docs"
    truncated: bool


@dataclass
class WalkedSource:
    """Output of ``walk_repo``."""

    source_id: str
    source_sha: str
    sections: list[SourceSection] = field(default_factory=list)
    redactions: list[Redaction] = field(default_factory=list)
    skipped: list[SkippedFile] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)


# ---------------------------------------------------------------------
# File policy (codex v2/v3)
# ---------------------------------------------------------------------


_ALLOWED_EXTENSIONS = {".md", ".rst", ".txt", ".markdown"}
_README_BASENAMES = {
    "readme.md", "readme.rst", "readme.txt", "readme.markdown",
    "readme",
}
_CHANGELOG_BASENAMES = {
    "changelog.md", "changelog.rst", "changelog.txt",
    "changelog.markdown",
    "changelog",
    "release_notes.md", "release_notes.rst",
    "release-notes.md", "release-notes.rst",
    "history.md", "history.rst",
}
_MAX_FILE_BYTES = 200 * 1024     # 200 KiB
_DECODE_SNIFF_BYTES = 8 * 1024    # 8 KiB sniff for binary detection


def _is_binary_bytes(data: bytes) -> bool:
    """Codex v3: binary = NUL byte in first 8 KiB OR UTF-8 decode
    fails on first 8 KiB."""
    sniff = data[:_DECODE_SNIFF_BYTES]
    if b"\x00" in sniff:
        return True
    try:
        sniff.decode("utf-8")
    except UnicodeDecodeError:
        return True
    return False


def _read_text_safely(
    p: Path, repo_root: Path
) -> tuple[str | None, str | None, int]:
    """Return ``(text, skip_reason, size_bytes)`` for path ``p``.

    On success: ``(text, None, size)`` where ``text`` is UTF-8 decoded
    and truncated to ``_MAX_FILE_BYTES`` if needed.
    On skip: ``(None, reason, size)``.
    """
    try:
        resolved = p.resolve()
    except OSError:
        return None, "binary", 0
    # Symlink outside the repo? Codex F-PR5-CR / v3 file policy.
    try:
        resolved.relative_to(repo_root.resolve())
    except ValueError:
        return None, "symlink_outside_repo", 0
    if p.suffix.lower() not in _ALLOWED_EXTENSIONS and p.name.lower() not in (
        _README_BASENAMES | _CHANGELOG_BASENAMES
    ):
        size = p.stat().st_size if p.is_file() else 0
        return None, "not_allowlisted", size
    try:
        data = p.read_bytes()
    except OSError:
        return None, "binary", 0
    size = len(data)
    if _is_binary_bytes(data):
        return None, "binary", size
    truncated_data = data[:_MAX_FILE_BYTES]
    try:
        text = truncated_data.decode("utf-8")
    except UnicodeDecodeError:
        return None, "binary", size
    return text, None, size



# ---------------------------------------------------------------------
# Source id resolution
# ---------------------------------------------------------------------


def resolve_source_id(repo_path: Path) -> tuple[str, str]:
    """Return ``(source_id, source_sha)`` for the repo at
    ``repo_path``. Falls back gracefully when not a git repo.
    """
    git_dir = repo_path / ".git"
    if not git_dir.exists():
        return f"local:{repo_path.name}@no-git", "no-git"
    try:
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=str(repo_path),
            capture_output=True,
            text=True,
            check=True,
            timeout=5,
        ).stdout.strip()
    except (subprocess.SubprocessError, FileNotFoundError):
        return f"local:{repo_path.name}@no-git", "no-git"
    try:
        origin = subprocess.run(
            ["git", "config", "--get", "remote.origin.url"],
            cwd=str(repo_path),
            capture_output=True,
            text=True,
            check=True,
            timeout=5,
        ).stdout.strip()
    except (subprocess.SubprocessError, FileNotFoundError):
        origin = ""
    return _format_source_id(origin, head, repo_path.name), head


_GITHUB_SSH_RE = re.compile(r"^git@github\.com:([^/]+)/(.+?)(?:\.git)?$")
_GITHUB_HTTPS_RE = re.compile(
    r"^https?://github\.com/([^/]+)/([^/]+?)(?:\.git)?/?$"
)


def _format_source_id(
    origin: str, head_sha: str, repo_basename: str
) -> str:
    """Map a git remote URL to a `github:owner/repo@<sha>` id when
    possible; otherwise return `local:<basename>@<sha>`."""
    if origin:
        m = _GITHUB_SSH_RE.match(origin) or _GITHUB_HTTPS_RE.match(origin)
        if m:
            owner, repo = m.group(1), m.group(2)
            return f"github:{owner}/{repo}@{head_sha}"
    return f"local:{repo_basename}@{head_sha}"


# ---------------------------------------------------------------------
# Public walker entry point
# ---------------------------------------------------------------------


def _find_readme(repo: Path) -> Path | None:
    for name in (
        "README.md", "README.rst", "README.txt", "README.markdown",
        "README",
    ):
        p = repo / name
        if p.is_file():
            return p
    return None


def _find_changelog(repo: Path) -> Path | None:
    for name in (
        "CHANGELOG.md", "CHANGELOG.rst", "CHANGELOG",
        "RELEASE_NOTES.md", "RELEASE-NOTES.md",
        "HISTORY.md",
    ):
        p = repo / name
        if p.is_file():
            return p
    return None


def _list_docs(repo: Path) -> list[Path]:
    """One level deep into `docs/`, alphabetical by filename."""
    docs = repo / "docs"
    if not docs.is_dir():
        return []
    return sorted(
        p for p in docs.iterdir()
        if p.is_file() and p.suffix.lower() in _ALLOWED_EXTENSIONS
    )


def walk_repo(
    repo: Path,
    source_id: str | None = None,
) -> WalkedSource:
    """Walk ``repo``, redact citations, return a ``WalkedSource``.

    ``source_id`` overrides the auto-resolved id (handy for tests).
    """
    repo = repo.resolve()
    if source_id is None:
        source_id, source_sha = resolve_source_id(repo)
    else:
        _, source_sha = resolve_source_id(repo)

    walked = WalkedSource(source_id=source_id, source_sha=source_sha)
    candidates: list[tuple[Path, str]] = []
    readme = _find_readme(repo)
    if readme is not None:
        candidates.append((readme, "readme"))
    changelog = _find_changelog(repo)
    if changelog is not None:
        candidates.append((changelog, "changelog"))
    for d in _list_docs(repo):
        candidates.append((d, "docs"))

    for path, kind in candidates:
        rel = str(path.relative_to(repo))
        text, skip_reason, size = _read_text_safely(path, repo)
        if text is None:
            walked.skipped.append(
                SkippedFile(path=rel, reason=skip_reason or "binary", size_bytes=size)
            )
            continue
        truncated = size > _MAX_FILE_BYTES
        if truncated:
            walked.notes.append(
                f"{rel}: truncated at {_MAX_FILE_BYTES} bytes "
                f"(file is {size} bytes)"
            )
        redacted_text, redactions = redact(text, section_path=rel)
        walked.sections.append(
            SourceSection(
                path=rel,
                text=redacted_text,
                text_raw=text,
                kind=kind,
                truncated=truncated,
            )
        )
        walked.redactions.extend(redactions)

    # Audit-only check: empty source.
    if not walked.sections:
        walked.notes.append(
            "no readable text found in this repo's allow-listed files"
        )
    return walked


# ---------------------------------------------------------------------
# Assemble for the model
# ---------------------------------------------------------------------


def assemble_for_model(walked: WalkedSource) -> str:
    """Concatenate the (post-redaction) section texts with explicit
    section headers so the model sees which file each excerpt
    came from.
    """
    parts: list[str] = []
    for section in walked.sections:
        parts.append(f"## source: {section.path}\n\n{section.text.strip()}")
    return "\n\n".join(parts) + ("\n" if parts else "")
