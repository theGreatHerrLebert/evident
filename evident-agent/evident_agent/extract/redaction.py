"""Phase 5 PR6: shared redaction module.

Citation redaction logic shared between the repo walker (PR5) and
the paper walker (PR6). The load-bearing transitive-source rule: a
source's bibliography references to OTHER artifacts get redacted
before the source text reaches the model.

This module was extracted from ``extract/repo.py`` in PR6 so the
paper walker can reuse the same regex set. ``extract/repo.py``
keeps a re-export of the public surface (`redact`, `Redaction`,
the `REDACTION_*` constants) so PR5-era callers stay green.

v3 P1 addition: paper-mode plain-text bibliography detection. PR5's
detector required a markdown heading (`## References`); papers
extracted from PDFs and many `.md` papers ship with a plain
``References`` line and no `#`. ``redact_paper(text, section_path)``
applies the markdown-mode detector first, then the plain-text
detector, so both forms get caught.
"""

from __future__ import annotations

import re
from dataclasses import dataclass


# ---------------------------------------------------------------------
# Public dataclass
# ---------------------------------------------------------------------


@dataclass
class Redaction:
    """One redaction the walker applied to a section."""

    section_path: str
    span_start: int      # offset in the SECTION's pre-redaction text
    span_end: int
    reason: str
    original: str        # the redacted substring (for audit)


# Stable redaction kind strings (used in EXTRACTION.md grouping).
REDACTION_DOI = "external_doi"
REDACTION_ARXIV = "external_arxiv"
REDACTION_PREPRINT = "external_preprint"
REDACTION_BIBLIOGRAPHY = "bibliography"
REDACTION_INLINE = "inline_citation"


# ---------------------------------------------------------------------
# URL/DOI/arXiv/preprint patterns
# ---------------------------------------------------------------------


_DOI_RE = re.compile(
    r"""
    (?:
        \bdoi:\s*10\.\d{4,9}/[-._;()/:%A-Za-z0-9]+
        |
        https?://(?:dx\.)?doi\.org/10\.\d{4,9}/[-._;()/:%A-Za-z0-9]+
    )
    (?:[?#][^\s)\]]*)?
    """,
    re.VERBOSE | re.IGNORECASE,
)

_ARXIV_RE = re.compile(
    r"""
    \b
    (?:
        arXiv:\s*
        (?:
            \d{4}\.\d{4,5}(?:v\d+)?
            |
            [a-z\-]+/\d{7}(?:v\d+)?
        )
        |
        https?://arxiv\.org/(?:abs|pdf)/
        (?:\d{4}\.\d{4,5}|[a-z\-]+/\d{7})(?:v\d+)?
        (?:\.pdf)?
    )
    """,
    re.VERBOSE | re.IGNORECASE,
)

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
    r"onlinelibrary\.wiley\.com",
    r"academic\.oup\.com",
    r"cambridge\.org/core",
    r"thelancet\.com",
    r"nejm\.org",
    r"bmj\.com",
    r"researchgate\.net",
    r"academia\.edu",
)
_PREPRINT_RE = re.compile(
    r"https?://(?:www\.)?(?:" + "|".join(_PREPRINT_HOSTS) + r")[^\s)\]]*",
    re.IGNORECASE,
)


# ---------------------------------------------------------------------
# Bibliography heading detectors
# ---------------------------------------------------------------------


_BIB_HEADING_KEYWORDS = r"(?:references|bibliography|works\s+cited)"

# Markdown-mode (PR5): ATX `## References`, with optional trailing
# punctuation / closed ATX / compound name.
_BIB_HEADING_ATX_RE = re.compile(
    rf"^(?P<hashes>#{{1,6}})\s+(?P<keyword>{_BIB_HEADING_KEYWORDS})"
    r"(?:\s+(?:and|&)\s+\S[^\n]*)?"
    r"(?:[:.\-]+|\s*#{1,6})?\s*$",
    re.IGNORECASE | re.MULTILINE,
)

# Markdown-mode (PR5): Setext form `References\n========`.
_BIB_HEADING_SETEXT_RE = re.compile(
    rf"^(?P<keyword>{_BIB_HEADING_KEYWORDS})\s*\n[=\-]{{2,}}\s*$",
    re.IGNORECASE | re.MULTILINE,
)

# Paper-mode plain-text (v3 P1 — new): a bare line whose text is
# `References` / `Bibliography` / `Works Cited`, immediately
# followed (within 3 non-empty lines) by a numbered or bracketed
# citation. The lookahead is the safety: a paper that happens to
# have a "References" prose section without numbered refs below
# is NOT redacted.
_BIB_HEADING_PLAINTEXT_RE = re.compile(
    # The horizontal-whitespace-only `[^\S\n]*` after the keyword is
    # deliberate: `\s*$` would greedily consume the blank lines the
    # `gap` counter is supposed to limit, bypassing the lookahead.
    rf"""
    ^(?P<keyword>{_BIB_HEADING_KEYWORDS})[^\S\n]*$
    (?P<gap>(?:\n[^\S\n]*){{0,3}})
    (?P<first_ref>\n[^\S\n]*(?:\d+\.|\[\d+\])\s+\S)
    """,
    re.IGNORECASE | re.MULTILINE | re.VERBOSE,
)


_ANY_HEADING_RE = re.compile(
    r"^(?:#{1,6}\s+.+|.+\n[=\-]{2,})$",
    re.MULTILINE,
)


def _find_bibliography_heading(text: str) -> re.Match[str] | None:
    """Find the first markdown-style bibliography heading. Returns
    None if no markdown heading exists; the plain-text detector
    handles paper-mode separately.
    """
    atx = _BIB_HEADING_ATX_RE.search(text)
    setext = _BIB_HEADING_SETEXT_RE.search(text)
    if atx is None:
        return setext
    if setext is None:
        return atx
    return atx if atx.start() <= setext.start() else setext


# ---------------------------------------------------------------------
# Inline citation markers
# ---------------------------------------------------------------------


_INLINE_NUMERIC_RE = re.compile(
    r"\[\s*\d+(?:\s*[,\-]\s*\d+)*\s*\]"
)
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


# ---------------------------------------------------------------------
# Trailing-URL-noise trim (codex F-PR5-CR2)
# ---------------------------------------------------------------------


_TRIM_TRAIL_PUNCT = ".,;:!?"


def _trim_trailing_url_noise(s: str) -> str:
    """Strip trailing sentence punctuation and unmatched closing
    brackets from a matched URL/DOI substring.
    """
    while s and (s[-1] in _TRIM_TRAIL_PUNCT or s[-1] in ")]>"):
        if s[-1] == ")":
            if s.count("(") >= s.count(")"):
                break
            s = s[:-1]
            continue
        if s[-1] == "]":
            if s.count("[") >= s.count("]"):
                break
            s = s[:-1]
            continue
        if s[-1] == ">":
            if s.count("<") >= s.count(">"):
                break
            s = s[:-1]
            continue
        s = s[:-1]
    return s


# ---------------------------------------------------------------------
# Pattern-redaction core
# ---------------------------------------------------------------------


def _redact_pattern(
    text: str,
    pattern: re.Pattern[str],
    kind: str,
    section_path: str,
    redactions: list[Redaction],
    trim_url_noise: bool = False,
) -> str:
    out: list[str] = []
    pos = 0
    for match in pattern.finditer(text):
        original = match.group(0)
        end = match.end()
        if trim_url_noise:
            trimmed = _trim_trailing_url_noise(original)
            if len(trimmed) < len(original):
                end = match.start() + len(trimmed)
                original = trimmed
        out.append(text[pos:match.start()])
        out.append(f"[external reference omitted: {kind}]")
        redactions.append(
            Redaction(
                section_path=section_path,
                span_start=match.start(),
                span_end=end,
                reason=kind,
                original=original,
            )
        )
        pos = end
    out.append(text[pos:])
    return "".join(out)


def _redact_bibliography_section_markdown(
    text: str,
    section_path: str,
    redactions: list[Redaction],
) -> tuple[str, bool]:
    """Markdown-mode: find an ATX / Setext bibliography heading, drop
    everything from it through the next heading (or EOF)."""
    bib_match = _find_bibliography_heading(text)
    if bib_match is None:
        return text, False
    start = bib_match.start()
    next_heading: re.Match[str] | None = None
    for m in _ANY_HEADING_RE.finditer(text, bib_match.end()):
        if m.start() > bib_match.end():
            next_heading = m
            break
    end = next_heading.start() if next_heading else len(text)
    original = text[start:end]
    redactions.append(
        Redaction(
            section_path=section_path,
            span_start=start,
            span_end=end,
            reason=REDACTION_BIBLIOGRAPHY,
            original=original,
        )
    )
    marker = "[external reference omitted: bibliography]\n"
    return text[:start] + marker + text[end:], True


def _redact_bibliography_section_plaintext(
    text: str,
    section_path: str,
    redactions: list[Redaction],
) -> tuple[str, bool]:
    """Paper-mode (v3 P1): a bare ``References`` / ``Bibliography`` /
    ``Works Cited`` line followed by numbered or bracketed
    references within 3 lines. Drops from the heading line to EOF.

    The lookahead-for-numbered-refs is the safety: a paper that has
    a prose "References" section without numbered refs below it is
    NOT redacted.
    """
    m = _BIB_HEADING_PLAINTEXT_RE.search(text)
    if m is None:
        return text, False
    start = m.start("keyword")
    # Adjust start to the BEGINNING of the heading line so the
    # heading line itself is part of the redaction.
    line_start = text.rfind("\n", 0, start) + 1
    end = len(text)  # plain-text bibliography runs to EOF
    original = text[line_start:end]
    redactions.append(
        Redaction(
            section_path=section_path,
            span_start=line_start,
            span_end=end,
            reason=REDACTION_BIBLIOGRAPHY,
            original=original,
        )
    )
    marker = "[external reference omitted: bibliography]\n"
    return text[:line_start] + marker, True


# ---------------------------------------------------------------------
# Public entry points
# ---------------------------------------------------------------------


def redact(text: str, section_path: str) -> tuple[str, list[Redaction]]:
    """Repo-mode redaction (PR5 contract). Markdown bibliography
    only — does NOT fire the plain-text bibliography detector.

    Use ``redact_paper`` for paper sources that may carry plain-text
    bibliography headers without markdown formatting.
    """
    return _apply_redactions(
        text,
        section_path,
        bibliography_detectors=(_redact_bibliography_section_markdown,),
    )


def redact_paper(text: str, section_path: str) -> tuple[str, list[Redaction]]:
    """Paper-mode redaction. Tries the markdown bibliography
    detector first (for markdown papers), then the plain-text
    detector (for PDFs and pre-formatted text). Codex v3 P1.
    """
    return _apply_redactions(
        text,
        section_path,
        bibliography_detectors=(
            _redact_bibliography_section_markdown,
            _redact_bibliography_section_plaintext,
        ),
    )


def _apply_redactions(
    text: str,
    section_path: str,
    *,
    bibliography_detectors,
) -> tuple[str, list[Redaction]]:
    redactions: list[Redaction] = []
    bib_redacted = False
    for detector in bibliography_detectors:
        text, did_redact = detector(text, section_path, redactions)
        bib_redacted = bib_redacted or did_redact
    text = _redact_pattern(
        text, _DOI_RE, REDACTION_DOI, section_path, redactions,
        trim_url_noise=True,
    )
    text = _redact_pattern(
        text, _ARXIV_RE, REDACTION_ARXIV, section_path, redactions,
        trim_url_noise=True,
    )
    text = _redact_pattern(
        text, _PREPRINT_RE, REDACTION_PREPRINT, section_path, redactions,
        trim_url_noise=True,
    )
    if bib_redacted:
        text = _redact_pattern(
            text, _INLINE_NUMERIC_RE, REDACTION_INLINE,
            section_path, redactions,
        )
        text = _redact_pattern(
            text, _INLINE_PAREN_AUTHOR_YEAR_RE, REDACTION_INLINE,
            section_path, redactions,
        )
        text = _redact_pattern(
            text, _INLINE_AUTHOR_YEAR_RE, REDACTION_INLINE,
            section_path, redactions,
        )
    return text, redactions
