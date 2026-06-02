"""Phase 5 PR6: paper walker.

Reads a paper (markdown or PDF), redacts citations to OTHER
papers, and assembles per-page (PDF) or single-section (markdown)
text for the model.

PDF support is **experimental** via ``pdftotext`` subprocess.
The walker pages-split on form-feed (``\\f``) so the validator's
local-binding rule remains meaningful at per-page granularity.
If ``pdftotext`` is missing OR the output is whitespace-only OR
the output has no form-feeds, the walker skips with a structured
reason and the CLI exits non-zero — codex F-PR6 v2 P2 explicit
contract.
"""

from __future__ import annotations

import hashlib
import re
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

from . import redaction
from .repo import (
    SkippedFile,
    SourceSection,
    WalkedSource,
    _MAX_FILE_BYTES,
)


# ---------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------


# Codex F-PR6-v2: source-id scan window extended from v1's 4 KiB to
# 64 KiB so embedded arXiv ids in PDF title pages / abstracts get
# picked up.
_SOURCE_ID_SCAN_BYTES = 64 * 1024


PDF_EXPERIMENTAL_BANNER = (
    "PDF extraction is experimental. pdftotext-mangled column "
    "breaks can defeat the validator's local-binding check. "
    "Inspect the extracted text in dry-run mode before trusting "
    "a non-dry-run extraction."
)


# Reasons unique to paper-mode (joined with PR5's set).
SKIP_PDFTOTEXT_UNAVAILABLE = "pdftotext_unavailable"
SKIP_PDF_EXTRACTION_EMPTY = "pdf_extraction_empty"
SKIP_PDF_NO_PAGE_BOUNDARIES = "pdf_no_page_boundaries"


# ---------------------------------------------------------------------
# Source-id detection
# ---------------------------------------------------------------------


_ARXIV_ID_INLINE_RE = re.compile(
    r"\barXiv:\s*(?P<id>\d{4}\.\d{4,5}(?:v\d+)?|[a-z\-]+/\d{7}(?:v\d+)?)",
    re.IGNORECASE,
)
_DOI_INLINE_RE = re.compile(
    r"""(?:
        \bdoi:\s*(?P<doi1>10\.\d{4,9}/[-._;()/:A-Za-z0-9]+)
        |
        https?://(?:dx\.)?doi\.org/(?P<doi2>10\.\d{4,9}/[-._;()/:A-Za-z0-9]+)
    )""",
    re.VERBOSE | re.IGNORECASE,
)


def detect_paper_source_id(
    text: str,
    *,
    fallback_basename: str,
    sha_seed: bytes | None = None,
) -> str:
    """Return a canonical paper source-id from the text. Scans the
    first ``_SOURCE_ID_SCAN_BYTES`` of the source, stopping at the
    first bibliography heading so an arXiv id in a cited paper
    doesn't become the source's id.

    Falls back to ``paper:<basename>@<sha256-of-bytes>`` when no
    embedded id is found.
    """
    # Stop the scan at the first bibliography heading. Try both
    # markdown and plain-text detectors.
    cap = text[:_SOURCE_ID_SCAN_BYTES]
    bib_md = redaction._find_bibliography_heading(cap)
    bib_pt = redaction._BIB_HEADING_PLAINTEXT_RE.search(cap)
    cutoffs = [m.start() for m in (bib_md, bib_pt) if m is not None]
    if cutoffs:
        cap = cap[: min(cutoffs)]
    arxiv = _ARXIV_ID_INLINE_RE.search(cap)
    if arxiv is not None:
        return f"arxiv:{arxiv.group('id')}"
    doi = _DOI_INLINE_RE.search(cap)
    if doi is not None:
        doi_str = doi.group("doi1") or doi.group("doi2")
        return f"doi:{doi_str}"
    digest = hashlib.sha256(
        sha_seed if sha_seed is not None else text.encode("utf-8")
    ).hexdigest()[:16]
    return f"paper:{fallback_basename}@{digest}"


# ---------------------------------------------------------------------
# PDF extraction
# ---------------------------------------------------------------------


def _pdftotext_available() -> bool:
    return shutil.which("pdftotext") is not None


def _run_pdftotext(path: Path) -> tuple[str | None, str | None]:
    """Return ``(text, error_reason)``. On success, ``text`` is the
    concatenated string output (still containing form-feeds between
    pages). On failure, ``error_reason`` is one of the SKIP_*
    constants.
    """
    if not _pdftotext_available():
        return None, SKIP_PDFTOTEXT_UNAVAILABLE
    try:
        result = subprocess.run(
            ["pdftotext", "-layout", str(path), "-"],
            capture_output=True,
            text=False,
            timeout=60,
            check=False,
        )
    except (subprocess.SubprocessError, OSError):
        return None, SKIP_PDF_EXTRACTION_EMPTY
    if result.returncode != 0:
        return None, SKIP_PDF_EXTRACTION_EMPTY
    raw = result.stdout
    try:
        decoded = raw.decode("utf-8", errors="replace")
    except Exception:
        return None, SKIP_PDF_EXTRACTION_EMPTY
    if not decoded.strip():
        return None, SKIP_PDF_EXTRACTION_EMPTY
    return decoded, None


def _split_pdf_text_into_pages(text: str) -> list[str] | None:
    """Split form-feed-delimited pdftotext output into per-page
    strings. Returns None if no form-feed is present (codex
    F-PR6-v3 P2: refuse to silently treat as one page).
    """
    if "\f" not in text:
        return None
    pages = [p.strip() for p in text.split("\f")]
    return [p for p in pages if p]


# ---------------------------------------------------------------------
# Public dataclass extension
# ---------------------------------------------------------------------


@dataclass
class PaperWalkResult:
    """Convenience wrapper that pairs ``WalkedSource`` with the
    source format the CLI can branch on (markdown vs PDF) for the
    experimental banner."""

    walked: WalkedSource
    source_format: str   # "markdown" | "pdf" | "pdf-skipped"


# ---------------------------------------------------------------------
# Top-level walker
# ---------------------------------------------------------------------


_MARKDOWN_EXTENSIONS = (".md", ".rst", ".txt", ".markdown")
_PDF_EXTENSIONS = (".pdf",)


def walk_paper(
    path: Path,
    *,
    source_id: str | None = None,
) -> PaperWalkResult:
    """Walk a single paper file. Returns a ``PaperWalkResult``
    carrying both the ``WalkedSource`` (consumed by the CLI exactly
    like PR5's repo walker output) and a ``source_format``
    discriminator for the experimental-PDF banner.
    """
    suffix = path.suffix.lower()
    if suffix in _PDF_EXTENSIONS:
        return _walk_pdf(path, source_id=source_id)
    if suffix in _MARKDOWN_EXTENSIONS:
        return _walk_markdown(path, source_id=source_id)
    walked = WalkedSource(
        source_id=source_id or f"paper:{path.name}@unsupported",
        source_sha="unsupported",
    )
    walked.skipped.append(
        SkippedFile(
            path=path.name,
            reason="not_allowlisted",
            size_bytes=path.stat().st_size if path.is_file() else None,
        )
    )
    walked.notes.append(
        f"unsupported paper extension {suffix!r}; expected one of "
        f"{_MARKDOWN_EXTENSIONS + _PDF_EXTENSIONS}"
    )
    return PaperWalkResult(walked=walked, source_format="unsupported")


def _walk_markdown(
    path: Path, *, source_id: str | None
) -> PaperWalkResult:
    raw = path.read_bytes()
    truncated = len(raw) > _MAX_FILE_BYTES
    text = raw[:_MAX_FILE_BYTES].decode("utf-8", errors="replace")
    resolved_id = source_id or detect_paper_source_id(
        text, fallback_basename=path.stem, sha_seed=raw
    )
    sha = hashlib.sha256(raw).hexdigest()
    walked = WalkedSource(source_id=resolved_id, source_sha=sha)
    redacted, redactions = redaction.redact_paper(text, section_path=path.name)
    walked.sections.append(
        SourceSection(
            path=path.name,
            text=redacted,
            text_raw=text,
            kind="paper",
            truncated=truncated,
        )
    )
    walked.redactions.extend(redactions)
    if truncated:
        walked.notes.append(
            f"{path.name}: truncated at {_MAX_FILE_BYTES} bytes "
            f"(file is {len(raw)} bytes)"
        )
    return PaperWalkResult(walked=walked, source_format="markdown")


def _walk_pdf(path: Path, *, source_id: str | None) -> PaperWalkResult:
    raw = path.read_bytes()
    sha = hashlib.sha256(raw).hexdigest()
    text, error = _run_pdftotext(path)
    if error is not None:
        walked = WalkedSource(
            source_id=source_id or f"paper:{path.stem}@{sha[:16]}",
            source_sha=sha,
        )
        walked.skipped.append(
            SkippedFile(
                path=path.name, reason=error, size_bytes=len(raw),
            )
        )
        if error == SKIP_PDFTOTEXT_UNAVAILABLE:
            walked.notes.append(
                "pdftotext is not installed. On Debian/Ubuntu: "
                "`apt install poppler-utils`. On macOS: "
                "`brew install poppler`."
            )
        elif error == SKIP_PDF_EXTRACTION_EMPTY:
            walked.notes.append(
                "pdftotext produced no readable text (PDF may be "
                "image-only / scanned). OCR support is a future "
                "PR6b slice."
            )
        return PaperWalkResult(walked=walked, source_format="pdf-skipped")

    assert text is not None
    pages = _split_pdf_text_into_pages(text)
    if pages is None:
        # Codex F-PR6-v3 P2: no form-feeds means we can't section the
        # text reliably. Refuse rather than silently treating as one
        # page and breaking the validator's local-binding contract.
        walked = WalkedSource(
            source_id=source_id or f"paper:{path.stem}@{sha[:16]}",
            source_sha=sha,
        )
        walked.skipped.append(
            SkippedFile(
                path=path.name,
                reason=SKIP_PDF_NO_PAGE_BOUNDARIES,
                size_bytes=len(raw),
            )
        )
        walked.notes.append(
            "pdftotext output has no form-feed page boundaries. "
            "Either re-extract with a tool that emits page breaks, "
            "or convert to markdown manually."
        )
        return PaperWalkResult(walked=walked, source_format="pdf-skipped")

    resolved_id = source_id or detect_paper_source_id(
        text, fallback_basename=path.stem, sha_seed=raw,
    )
    walked = WalkedSource(source_id=resolved_id, source_sha=sha)
    walked.notes.append(PDF_EXPERIMENTAL_BANNER)
    for i, page in enumerate(pages, start=1):
        section_path = f"page-{i}"
        redacted, redactions = redaction.redact_paper(
            page, section_path=section_path,
        )
        walked.sections.append(
            SourceSection(
                path=section_path,
                text=redacted,
                text_raw=page,
                kind="paper-page",
                truncated=False,
            )
        )
        walked.redactions.extend(redactions)
    return PaperWalkResult(walked=walked, source_format="pdf")


def assemble_for_model(walked: WalkedSource) -> str:
    """Concatenate page sections with explicit headers. Matches the
    repo walker's contract — the model sees ``## source: page-N``
    or ``## source: <filename>`` headers."""
    parts: list[str] = []
    for section in walked.sections:
        parts.append(f"## source: {section.path}\n\n{section.text.strip()}")
    return "\n\n".join(parts) + ("\n" if parts else "")
