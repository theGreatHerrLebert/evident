"""Phase 5 PR6: paper walker tests.

Covers:

- markdown paper reading + source_id detection (arxiv / doi /
  basename+sha fallback)
- plain-text bibliography detection (codex v1 P1 load-bearing)
- false-positive guard: "References" without numbered refs is
  NOT redacted
- inline citation conservative rule still applies
- file truncation
- PDF extraction failure modes (codex v2 P2 contract):
  pdftotext missing, empty extraction, no form-feeds
- PDF page split via form-feed
- assemble_for_model preserves page headers
"""

from __future__ import annotations

import io
import subprocess
from pathlib import Path
from unittest import mock

import pytest

from evident_agent.extract import paper, redaction


FIXTURES = Path(__file__).resolve().parent / "fixtures" / "extract" / "paper"


# ---------------------------------------------------------------------
# Source-id detection
# ---------------------------------------------------------------------


def test_walk_markdown_detects_arxiv_source_id():
    result = paper.walk_paper(FIXTURES / "clear_paper.md")
    assert result.walked.source_id == "arxiv:2501.12345v1"
    assert result.source_format == "markdown"


def test_walk_markdown_detects_doi_source_id():
    result = paper.walk_paper(FIXTURES / "plaintext_bibliography_paper.md")
    assert result.walked.source_id.startswith("doi:10.5555/our.paper.2026")


def test_walk_markdown_falls_back_to_paper_basename(tmp_path: Path):
    p = tmp_path / "no_id_paper.md"
    p.write_text(
        "# Some Paper\n\nWe show our method achieves rmsd < 0.5.\n"
    )
    result = paper.walk_paper(p)
    assert result.walked.source_id.startswith("paper:no_id_paper@")


def test_source_id_explicit_override(tmp_path: Path):
    p = tmp_path / "ambiguous.md"
    p.write_text("# title\n\nbody")
    result = paper.walk_paper(p, source_id="arxiv:9999.99999v2")
    assert result.walked.source_id == "arxiv:9999.99999v2"


def test_source_id_scan_stops_before_bibliography():
    """Codex F-PR6 v2: the scan must NOT pick up an arXiv id that
    only appears in a cited paper's reference. The plaintext
    bibliography fixture has DOI/arXiv ids in its References list;
    they must not become the source id.
    """
    result = paper.walk_paper(
        FIXTURES / "plaintext_bibliography_paper.md"
    )
    # The cited paper has arXiv:2301.99999v1 in its references.
    # That id must NOT be picked.
    assert "2301.99999" not in result.walked.source_id


# ---------------------------------------------------------------------
# Plain-text bibliography detection (codex v1 P1)
# ---------------------------------------------------------------------


def test_walk_paper_redacts_plaintext_bibliography():
    """Load-bearing for the codex-flagged contamination path: a
    paper with a plain `References` line (no markdown heading)
    followed by numbered refs must have its bibliography redacted."""
    result = paper.walk_paper(
        FIXTURES / "plaintext_bibliography_paper.md"
    )
    kinds = {r.reason for r in result.walked.redactions}
    assert redaction.REDACTION_BIBLIOGRAPHY in kinds
    # Body text stays.
    body = result.walked.sections[0].text
    assert "throughput greater than 1000" in body
    # Cited papers' content does NOT.
    assert "first.cited" not in body
    assert "Doe, A. (2023)" not in body


def test_walk_paper_does_not_redact_references_section_without_refs():
    """Codex v2 false-positive guard: a paper with a `References`
    section name but no numbered references below must NOT be
    redacted."""
    result = paper.walk_paper(
        FIXTURES / "false_positive_references_paper.md"
    )
    bib = [
        r for r in result.walked.redactions
        if r.reason == redaction.REDACTION_BIBLIOGRAPHY
    ]
    assert bib == [], (
        f"unexpected bibliography redaction on the false-positive "
        f"fixture: {bib!r}"
    )
    # Conclusion section stays.
    body = result.walked.sections[0].text
    assert "Future work" in body


@pytest.mark.parametrize(
    "snippet",
    [
        "References\n\n1. Smith et al.\n",
        "REFERENCES\n\n1. SMITH ET AL.\n",
        "Bibliography\n\n[1] Smith et al.\n",
        "Works Cited\n\n1. Smith et al.\n",
        # Blank-line gap up to 3 lines.
        "References\n\n\n1. Smith et al.\n",
    ],
)
def test_redact_paper_plaintext_bibliography_variants(snippet):
    body = "Our method achieves rmsd < 0.5 on the suite.\n\n" + snippet
    redacted, redactions = redaction.redact_paper(body, "paper.md")
    bib = [
        r for r in redactions
        if r.reason == redaction.REDACTION_BIBLIOGRAPHY
    ]
    assert bib, f"failed to redact variant: {snippet!r}"
    # Body text stays.
    assert "rmsd < 0.5" in redacted


@pytest.mark.parametrize(
    "snippet",
    [
        # Author-year refs (no numbers) — NOT redacted by the
        # safety rule.
        "References\n\nSmith, J., 2024. Nature, 12, 345.\n",
        # Numbered refs only after a long gap — outside the
        # 3-line lookahead.
        "References\n\n\n\n\n1. Smith et al.\n",
    ],
)
def test_redact_paper_does_not_redact_when_lookahead_fails(snippet):
    body = "Our method achieves rmsd < 0.5 on the suite.\n\n" + snippet
    _, redactions = redaction.redact_paper(body, "paper.md")
    bib = [
        r for r in redactions
        if r.reason == redaction.REDACTION_BIBLIOGRAPHY
    ]
    assert bib == [], (
        f"unexpectedly redacted on the false-negative guard "
        f"variant {snippet!r}: {bib!r}"
    )


def test_walk_paper_redacts_inline_citations_after_bibliography():
    """Codex v3 conservative rule: post-bibliography inline `[1]`
    redaction also fires for paper-mode plain-text bibliographies.
    """
    text = (
        "# Paper\n\n"
        "Our method achieves rmsd < 0.5 [1] across the BPTI suite.\n\n"
        "References\n\n"
        "1. Smith et al. (2024).\n"
    )
    redacted, redactions = redaction.redact_paper(text, "paper.md")
    inline = [
        r for r in redactions
        if r.reason == redaction.REDACTION_INLINE
    ]
    assert inline, "expected post-bibliography inline redaction"
    assert "[1]" not in redacted


# ---------------------------------------------------------------------
# DOI / arxiv / preprint redaction shared with repo
# ---------------------------------------------------------------------


def test_walk_paper_still_redacts_arxiv_and_doi_links_outside_bibliography():
    result = paper.walk_paper(FIXTURES / "clear_paper.md")
    body = result.walked.sections[0].text
    assert "arXiv:2501.12345" not in body  # source id, in abstract
    assert "doi.org/10.1234" not in body   # in the markdown bibliography
    # Body claim survives.
    assert "median RMSD less than 0.5" in body


# ---------------------------------------------------------------------
# File policy
# ---------------------------------------------------------------------


def test_walk_paper_truncates_large_markdown(tmp_path: Path):
    big = tmp_path / "big_paper.md"
    big.write_bytes(b"a" * (300 * 1024))
    result = paper.walk_paper(big)
    assert len(result.walked.sections) == 1
    assert result.walked.sections[0].truncated
    assert any("truncated" in n for n in result.walked.notes)


def test_walk_paper_skips_unsupported_extension(tmp_path: Path):
    weird = tmp_path / "paper.bin"
    weird.write_bytes(b"data")
    result = paper.walk_paper(weird)
    assert result.walked.sections == []
    assert any(
        s.reason == "not_allowlisted" for s in result.walked.skipped
    )


# ---------------------------------------------------------------------
# PDF extraction
# ---------------------------------------------------------------------


def _fake_pdftotext_output(text: str) -> mock.Mock:
    proc = mock.Mock()
    proc.returncode = 0
    proc.stdout = text.encode("utf-8")
    return proc


def test_walk_paper_pdf_missing_pdftotext(tmp_path: Path, monkeypatch):
    """Codex F-PR6 v2 P2 contract: missing pdftotext → clean skip
    with install hint, no manifest."""
    pdf = tmp_path / "paper.pdf"
    pdf.write_bytes(b"%PDF-1.4\n")
    monkeypatch.setattr(paper.shutil, "which", lambda _: None)
    result = paper.walk_paper(pdf)
    assert result.walked.sections == []
    assert any(
        s.reason == paper.SKIP_PDFTOTEXT_UNAVAILABLE
        for s in result.walked.skipped
    )
    assert any(
        "poppler-utils" in n for n in result.walked.notes
    )


def test_walk_paper_pdf_empty_extraction(tmp_path: Path, monkeypatch):
    """Codex F-PR6 v2 P2 contract: empty/whitespace output → clean
    skip with structured reason."""
    pdf = tmp_path / "scanned.pdf"
    pdf.write_bytes(b"%PDF-1.4\n")
    monkeypatch.setattr(paper.shutil, "which", lambda _: "/usr/bin/pdftotext")
    monkeypatch.setattr(
        paper.subprocess, "run",
        lambda *a, **kw: _fake_pdftotext_output("   \n\n  "),
    )
    result = paper.walk_paper(pdf)
    assert result.walked.sections == []
    assert any(
        s.reason == paper.SKIP_PDF_EXTRACTION_EMPTY
        for s in result.walked.skipped
    )


def test_walk_paper_pdf_no_form_feeds_skipped(tmp_path: Path, monkeypatch):
    """Codex F-PR6 v3 P2 contract: pdftotext output without
    form-feeds → skip with pdf_no_page_boundaries reason. Without
    this, a 50-page PDF would collapse to one section and break
    the validator's local-binding rule.
    """
    pdf = tmp_path / "paper.pdf"
    pdf.write_bytes(b"%PDF-1.4\n")
    monkeypatch.setattr(paper.shutil, "which", lambda _: "/usr/bin/pdftotext")
    monkeypatch.setattr(
        paper.subprocess, "run",
        lambda *a, **kw: _fake_pdftotext_output(
            "Page 1 text without form feeds.\n"
            "Page 2 text continues without page break."
        ),
    )
    result = paper.walk_paper(pdf)
    assert result.walked.sections == []
    assert any(
        s.reason == paper.SKIP_PDF_NO_PAGE_BOUNDARIES
        for s in result.walked.skipped
    )


def test_walk_paper_pdf_splits_on_form_feed(tmp_path: Path, monkeypatch):
    """Codex F-PR6 v2 P2 contract: well-formed pdftotext output gets
    split into per-page sections."""
    pdf = tmp_path / "paper.pdf"
    pdf.write_bytes(b"%PDF-1.4\n")
    monkeypatch.setattr(paper.shutil, "which", lambda _: "/usr/bin/pdftotext")
    monkeypatch.setattr(
        paper.subprocess, "run",
        lambda *a, **kw: _fake_pdftotext_output(
            "Page 1: We achieve median rmsd less than 0.5 on the suite.\f"
            "Page 2: Future work extends the benchmark.\f"
            "Page 3: References\n1. Smith et al.\n"
        ),
    )
    result = paper.walk_paper(pdf)
    assert result.source_format == "pdf"
    paths = [s.path for s in result.walked.sections]
    assert paths == ["page-1", "page-2", "page-3"]
    # Experimental banner is present in notes.
    assert any(
        "experimental" in n.lower() for n in result.walked.notes
    )


# ---------------------------------------------------------------------
# assemble_for_model
# ---------------------------------------------------------------------


def test_assemble_for_model_includes_per_page_headers(
    tmp_path: Path, monkeypatch,
):
    pdf = tmp_path / "p.pdf"
    pdf.write_bytes(b"%PDF-1.4\n")
    monkeypatch.setattr(paper.shutil, "which", lambda _: "/usr/bin/pdftotext")
    monkeypatch.setattr(
        paper.subprocess, "run",
        lambda *a, **kw: _fake_pdftotext_output("page one body\fpage two body\f"),
    )
    result = paper.walk_paper(pdf)
    assembled = paper.assemble_for_model(result.walked)
    assert "## source: page-1" in assembled
    assert "## source: page-2" in assembled
