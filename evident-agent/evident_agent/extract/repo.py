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
"""

from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass, field
from pathlib import Path


# ---------------------------------------------------------------------
# Public dataclasses
# ---------------------------------------------------------------------


@dataclass
class Redaction:
    """One redaction the walker applied to a section."""

    section_path: str
    span_start: int      # offset in the SECTION's pre-redaction text
    span_end: int
    reason: str          # see REDACTION_KINDS
    original: str        # the redacted substring (for audit)


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


# Stable redaction kind strings (used in EXTRACTION.md grouping).
REDACTION_DOI = "external_doi"
REDACTION_ARXIV = "external_arxiv"
REDACTION_PREPRINT = "external_preprint"
REDACTION_BIBLIOGRAPHY = "bibliography"
REDACTION_INLINE = "inline_citation"


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
# Citation redaction (codex P1 v2 + v3 expansion)
# ---------------------------------------------------------------------


# DOI URLs and bare doi: forms. Captures common suffixes like
# `.pdf`, `/full`, `/abstract`, query strings, etc.
_DOI_RE = re.compile(
    r"""
    (?:
        # Bare doi: form
        \bdoi:\s*10\.\d{4,9}/[-._;()/:%A-Za-z0-9]+
        |
        # https://[dx.]doi.org/...
        https?://(?:dx\.)?doi\.org/10\.\d{4,9}/[-._;()/:%A-Za-z0-9]+
    )
    (?:[?#][^\s)\]]*)?   # optional query string / fragment
    """,
    re.VERBOSE | re.IGNORECASE,
)

# arXiv ids: arXiv:NNNN.NNNNN(vN)?, arxiv.org/abs/NNNN.NNNNN, etc.
# Plus old-style cs/0301012.
_ARXIV_RE = re.compile(
    r"""
    \b
    (?:
        arXiv:\s*
        (?:
            \d{4}\.\d{4,5}(?:v\d+)?            # new-style
            |
            [a-z\-]+/\d{7}(?:v\d+)?            # old-style
        )
        |
        https?://arxiv\.org/(?:abs|pdf)/
        (?:\d{4}\.\d{4,5}|[a-z\-]+/\d{7})(?:v\d+)?
        (?:\.pdf)?
    )
    """,
    re.VERBOSE | re.IGNORECASE,
)

# Non-DOI preprint and publisher paper domains (codex v2 add).
# Match any URL whose host is in this set, eating the full path.
_PREPRINT_HOSTS = (
    r"biorxiv\.org",
    r"medrxiv\.org",
    r"chemrxiv\.org",
    r"osf\.io",
    r"ssrn\.com",
    r"papers\.ssrn\.com",
    r"semanticscholar\.org",
    r"s2-research\.org",
    r"openreview\.net",
    r"aclanthology\.org",
    r"papers\.nips\.cc",
    r"proceedings\.neurips\.cc",
    r"proceedings\.mlr\.press",
    r"pubmed\.ncbi\.nlm\.nih\.gov",
    r"ncbi\.nlm\.nih\.gov/pmc",
    r"dl\.acm\.org",
    r"ieeexplore\.ieee\.org",
    r"link\.springer\.com",
    r"sciencedirect\.com",
    r"nature\.com/articles",
    r"science\.org/doi",
)
_PREPRINT_RE = re.compile(
    r"https?://(?:www\.)?(?:" + "|".join(_PREPRINT_HOSTS) + r")[^\s)\]]*",
    re.IGNORECASE,
)


# Bibliography heading detection. Conservative — only matches
# headings whose normalised text is exactly `references`,
# `bibliography`, or `works cited`. Does NOT match `citation` /
# `how to cite` (codex v2 explicit false-positive guard).
_BIB_HEADING_RE = re.compile(
    r"^(#{1,6})\s+(references|bibliography|works\s+cited)\s*$",
    re.IGNORECASE | re.MULTILINE,
)


# Next-heading detector (any markdown heading depth).
_ANY_HEADING_RE = re.compile(r"^#{1,6}\s+.+$", re.MULTILINE)


# Inline citation markers — applied only after a bibliography was
# redacted from the SAME section. Conservative: numeric `[1]`,
# numeric ranges/lists, parenthetical author-year, inline
# author-year.
_INLINE_NUMERIC_RE = re.compile(
    r"\[\s*\d+(?:\s*[,\-]\s*\d+)*\s*\]"
)
# Optional comma after "et al" because some styles use "Smith et al., 2024".
_INLINE_PAREN_AUTHOR_YEAR_RE = re.compile(
    r"""
    \(
    [A-Z][A-Za-z\-]+
    (?:\s+(?:and|&)\s+[A-Z][A-Za-z\-]+)?
    (?:\s+et\s+al\.?,?)?
    ,?\s*
    (?:19|20)\d{2}[a-z]?
    \)
    """,
    re.VERBOSE,
)
_INLINE_AUTHOR_YEAR_RE = re.compile(
    r"""
    \b
    [A-Z][A-Za-z\-]+
    (?:\s+(?:and|&)\s+[A-Z][A-Za-z\-]+)?
    (?:\s+et\s+al\.?,?)?
    \s+
    \(
    (?:19|20)\d{2}[a-z]?
    \)
    """,
    re.VERBOSE,
)


def _redact_pattern(
    text: str,
    pattern: re.Pattern[str],
    kind: str,
    section_path: str,
    redactions: list[Redaction],
) -> str:
    """Replace each match of ``pattern`` in ``text`` with the
    marker ``[external reference omitted: <kind>]`` and record the
    redaction. Operates on a single section."""
    out: list[str] = []
    pos = 0
    for match in pattern.finditer(text):
        out.append(text[pos:match.start()])
        marker = f"[external reference omitted: {kind}]"
        out.append(marker)
        redactions.append(
            Redaction(
                section_path=section_path,
                span_start=match.start(),
                span_end=match.end(),
                reason=kind,
                original=match.group(0),
            )
        )
        pos = match.end()
    out.append(text[pos:])
    return "".join(out)


def _redact_bibliography_section(
    text: str,
    section_path: str,
    redactions: list[Redaction],
) -> tuple[str, bool]:
    """Find the first bibliography heading and drop everything from
    it through the next heading (or EOF). Returns ``(new_text,
    bibliography_was_redacted)``.

    Codex v3: only matches `references | bibliography | works cited`.
    `## Citation` / `## How to Cite` are NOT bibliography (they
    explain how to cite the repo itself).
    """
    bib_match = _BIB_HEADING_RE.search(text)
    if bib_match is None:
        return text, False
    start = bib_match.start()
    # Find the next heading after the bibliography heading.
    next_heading: re.Match[str] | None = None
    for m in _ANY_HEADING_RE.finditer(text, bib_match.end()):
        if m.start() > bib_match.end():
            next_heading = m
            break
    end = next_heading.start() if next_heading else len(text)
    original = text[start:end]
    marker = "[external reference omitted: bibliography]\n"
    redactions.append(
        Redaction(
            section_path=section_path,
            span_start=start,
            span_end=end,
            reason=REDACTION_BIBLIOGRAPHY,
            original=original,
        )
    )
    return text[:start] + marker + text[end:], True


def redact(
    text: str, section_path: str
) -> tuple[str, list[Redaction]]:
    """Apply all redaction rules to ``text``. Returns the post-
    redaction text and the list of redactions performed.

    Order matters: bibliography redaction first (so URL redaction
    inside the bibliography is unnecessary), then DOI / arXiv /
    preprint URL redaction in the remaining body, then conservative
    inline-citation redaction ONLY IF a bibliography was redacted.
    """
    redactions: list[Redaction] = []
    text, bib_redacted = _redact_bibliography_section(
        text, section_path, redactions
    )
    text = _redact_pattern(
        text, _DOI_RE, REDACTION_DOI, section_path, redactions
    )
    text = _redact_pattern(
        text, _ARXIV_RE, REDACTION_ARXIV, section_path, redactions
    )
    text = _redact_pattern(
        text, _PREPRINT_RE, REDACTION_PREPRINT, section_path, redactions
    )
    if bib_redacted:
        # Conservative inline-citation pass — only fires when a
        # bibliography existed, so a plain README mention of `[1]`
        # in installation steps stays unredacted.
        text = _redact_pattern(
            text, _INLINE_NUMERIC_RE, REDACTION_INLINE, section_path, redactions
        )
        text = _redact_pattern(
            text, _INLINE_PAREN_AUTHOR_YEAR_RE, REDACTION_INLINE, section_path, redactions
        )
        text = _redact_pattern(
            text, _INLINE_AUTHOR_YEAR_RE, REDACTION_INLINE, section_path, redactions
        )
    return text, redactions


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
