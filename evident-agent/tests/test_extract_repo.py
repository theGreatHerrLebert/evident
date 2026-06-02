"""Phase 5 PR5: walker tests.

Cover the load-bearing behaviour codex flagged across two reviews
of the PR5 plan:

- redaction order and coverage (DOI / arXiv / preprint hosts /
  bibliography / inline-citation markers)
- conservative inline-citation rule (only fires AFTER bibliography
  removal)
- false-positive guard on `## Citation` / `## How to Cite` headings
- file truncation, binary detection, symlink rejection
- section-header assembly for the model input
- source_id resolution from git remote URLs
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from evident_agent.extract import repo


FIXTURES = Path(__file__).resolve().parent / "fixtures" / "extract" / "repo"


# ---------------------------------------------------------------------
# Fixture-driven walker behaviour
# ---------------------------------------------------------------------


def test_walk_clean_repo_finds_readme_only():
    walked = repo.walk_repo(FIXTURES / "clean_repo")
    paths = [s.path for s in walked.sections]
    assert paths == ["README.md"]
    assert walked.skipped == []
    # No external citations in this repo.
    assert walked.redactions == []


def test_walk_conflict_repo_finds_readme_and_changelog():
    """Codex v2: README + CHANGELOG must both be included in the
    right priority order (README first)."""
    walked = repo.walk_repo(FIXTURES / "conflict_repo")
    paths = [s.path for s in walked.sections]
    assert paths == ["README.md", "CHANGELOG.md"]


def test_walk_marketing_repo_yields_no_redactions():
    walked = repo.walk_repo(FIXTURES / "marketing_repo")
    assert walked.redactions == []
    # Marketing copy still gets included — the validator is the
    # safety net, not the walker.
    assert any(s.kind == "readme" for s in walked.sections)


def test_walk_empty_dir_produces_zero_sections(tmp_path: Path):
    """Codex v3: empty source is a valid output."""
    walked = repo.walk_repo(tmp_path)
    assert walked.sections == []
    assert any("no readable text" in n for n in walked.notes)


# ---------------------------------------------------------------------
# Citation redaction — fixture
# ---------------------------------------------------------------------


def test_cites_paper_repo_redacts_bibliography_section():
    walked = repo.walk_repo(FIXTURES / "cites_paper_repo")
    # The README's `## References` block should be redacted as a
    # bibliography.
    kinds = {r.reason for r in walked.redactions}
    assert repo.REDACTION_BIBLIOGRAPHY in kinds


def test_cites_paper_repo_redacts_inline_citations_after_bibliography():
    """The README cites [1] and (Doe and Lee, 2023) in the body.
    Because the body also has a `## References` bibliography, the
    conservative inline-citation pass fires."""
    walked = repo.walk_repo(FIXTURES / "cites_paper_repo")
    kinds = {r.reason for r in walked.redactions}
    assert repo.REDACTION_INLINE in kinds
    # The post-redaction text must NOT carry the literal `[1]`
    # or `(Doe and Lee, 2023)`.
    section_text = walked.sections[0].text
    assert "[1]" not in section_text
    assert "(Doe and Lee, 2023)" not in section_text


def test_cite_this_repo_does_not_redact_citation_heading():
    """Codex v3 explicit: `## Citation` / `## How to Cite` are NOT
    bibliography. The body claim must pass through unaffected."""
    walked = repo.walk_repo(FIXTURES / "cite_this_repo")
    bib_kinds = [
        r for r in walked.redactions if r.reason == repo.REDACTION_BIBLIOGRAPHY
    ]
    assert bib_kinds == [], (
        f"`## Citation` heading was incorrectly redacted: {bib_kinds}"
    )
    # The clean tolerance line stays.
    text = walked.sections[0].text
    assert "throughput greater than 5000" in text


def test_cite_this_repo_does_not_inline_redact_without_bibliography():
    """Codex v3 false-positive guard: a README that has no
    bibliography but happens to contain `(Smith and Jones (2026))`
    in a how-to-cite block should NOT trip the inline-citation
    redaction."""
    walked = repo.walk_repo(FIXTURES / "cite_this_repo")
    inline = [
        r for r in walked.redactions if r.reason == repo.REDACTION_INLINE
    ]
    assert inline == [], f"unexpected inline redactions: {inline}"


# ---------------------------------------------------------------------
# Citation redaction — targeted regex coverage
# ---------------------------------------------------------------------


@pytest.mark.parametrize(
    "snippet,expected_kind",
    [
        ("see doi:10.1234/abcd for details", repo.REDACTION_DOI),
        (
            "https://doi.org/10.1234/xyz.567 published 2024",
            repo.REDACTION_DOI,
        ),
        (
            "https://doi.org/10.1234/xyz.567.pdf published 2024",
            repo.REDACTION_DOI,
        ),
        (
            "see https://dx.doi.org/10.1234/xyz?utm=foo here",
            repo.REDACTION_DOI,
        ),
        ("arXiv:2501.12345v2 is great", repo.REDACTION_ARXIV),
        ("see arXiv:cs/0301012v1", repo.REDACTION_ARXIV),
        (
            "https://arxiv.org/abs/2501.12345 contains the paper",
            repo.REDACTION_ARXIV,
        ),
        (
            "see https://arxiv.org/pdf/2501.12345v2.pdf",
            repo.REDACTION_ARXIV,
        ),
        (
            "biorxiv link https://www.biorxiv.org/content/10.1101/2024.01.02v1",
            repo.REDACTION_PREPRINT,
        ),
        (
            "openreview submission https://openreview.net/forum?id=XYZ",
            repo.REDACTION_PREPRINT,
        ),
        (
            "see https://aclanthology.org/2024.acl-long.123/",
            repo.REDACTION_PREPRINT,
        ),
        (
            "nature article https://www.nature.com/articles/s41586-024-12345",
            repo.REDACTION_PREPRINT,
        ),
    ],
)
def test_redact_recognises_citation_form(snippet, expected_kind):
    redacted, redactions = repo.redact(snippet, "test.md")
    assert any(
        r.reason == expected_kind for r in redactions
    ), f"no {expected_kind!r} match in {snippet!r}; got {redactions!r}"
    assert "external reference omitted" in redacted


def test_redact_leaves_clean_text_alone():
    """A README without any citation forms must produce zero
    redactions."""
    text = (
        "Our library does X. It runs fast.\n\n"
        "## Performance\n\n"
        "rmsd < 0.5 on the suite.\n"
    )
    _, redactions = repo.redact(text, "README.md")
    assert redactions == []


def test_redact_inline_only_fires_after_bibliography():
    """Codex v3: a README with no `## References` heading but a
    stray `[1]` (e.g. in install steps) must NOT be inline-
    redacted."""
    text = (
        "## Install\n\n"
        "Run `step [1]` then `step [2]`. See the docs.\n"
    )
    redacted, redactions = repo.redact(text, "README.md")
    inline = [r for r in redactions if r.reason == repo.REDACTION_INLINE]
    assert inline == []
    # Tokens are not replaced.
    assert "[1]" in redacted


@pytest.mark.parametrize(
    "heading_form",
    [
        "## References",
        "## References:",
        "## References.",
        "## References and Resources",
        "## References ##",
        "# References & Acknowledgments",
    ],
)
def test_redact_recognises_atx_bibliography_variants(heading_form):
    """Codex F-PR5-CR1 (P1): the v1 regex only matched bare
    `## References`. v2 also matches trailing-punctuation, compound,
    and closed-ATX forms."""
    text = (
        "## Body\n\nHello.\n\n"
        f"{heading_form}\n\n"
        "[1] Smith et al.\n"
    )
    _, redactions = repo.redact(text, "README.md")
    bib = [r for r in redactions if r.reason == repo.REDACTION_BIBLIOGRAPHY]
    assert bib, f"failed to redact bibliography heading {heading_form!r}"


def test_redact_recognises_setext_bibliography_heading():
    """Codex F-PR5-CR1 (P1): Setext-style headings
    `References\\n========` must also trigger bibliography
    redaction."""
    text = (
        "## Body\n\nHello.\n\n"
        "References\n"
        "----------\n\n"
        "[1] Smith et al.\n"
    )
    _, redactions = repo.redact(text, "README.md")
    bib = [r for r in redactions if r.reason == repo.REDACTION_BIBLIOGRAPHY]
    assert bib, "failed to redact Setext-style bibliography heading"


@pytest.mark.parametrize(
    "snippet",
    [
        "see https://doi.org/10.1145/3637528.3671624.",
        "(see https://doi.org/10.1145/3637528.3671624) for details",
        "see arXiv:2501.12345.",
        "(arXiv:2501.12345)",
    ],
)
def test_redact_trims_trailing_url_noise(snippet):
    """Codex F-PR5-CR2 (P2): the matched span must not eat the
    sentence-ending period or the closing paren that wraps the
    citation."""
    redacted, redactions = repo.redact(snippet, "README.md")
    assert redactions, "expected at least one redaction"
    # The trailing punctuation must survive in the post-redaction
    # text.
    last_orig = redactions[-1].original
    assert not last_orig.endswith(".")
    assert not last_orig.endswith(")")
    # Check the text after redaction still has the surrounding
    # punctuation in the right place.
    if snippet.endswith("."):
        assert redacted.rstrip().endswith(".")
    if "(" in snippet and snippet.split(")", 1)[1]:
        # Closing paren was part of the wrapper, must remain.
        assert ")" in redacted


def test_redact_preserves_balanced_parens_inside_doi_body():
    """DOI bodies can legitimately contain balanced parentheses
    like `10.1002/(SICI)1097-...`. Those must NOT be trimmed."""
    text = "see doi:10.1002/(SICI)1097-4571(199505)46:4 here"
    redacted, redactions = repo.redact(text, "README.md")
    assert any(r.reason == repo.REDACTION_DOI for r in redactions)
    # The redacted original keeps the balanced parens.
    assert "(SICI)" in redactions[0].original
    assert "(199505)" in redactions[0].original


@pytest.mark.parametrize(
    "snippet",
    [
        "see https://onlinelibrary.wiley.com/doi/10.1002/abc",
        "see https://academic.oup.com/journal/article-pdf/123",
        "see https://www.cambridge.org/core/journals/abc",
        "see https://www.thelancet.com/journals/lanonc/article",
        "see https://www.nejm.org/doi/10.1056/foo",
        "see https://www.bmj.com/content/123/bmj.xyz",
        "see https://www.researchgate.net/publication/123_paper",
        "see https://www.academia.edu/45678/paper",
    ],
)
def test_redact_covers_publisher_paper_domains(snippet):
    """Codex F-PR5-CR3 (P2): publisher / paper-surface hosts must
    be redacted, otherwise the model can claim things attributed
    to journal papers."""
    redacted, redactions = repo.redact(snippet, "README.md")
    assert any(
        r.reason == repo.REDACTION_PREPRINT for r in redactions
    ), f"missed publisher host in {snippet!r}: got {redactions!r}"


def test_redact_bibliography_drops_through_next_heading():
    text = (
        "## Body\n\n"
        "Hello world.\n\n"
        "## References\n\n"
        "[1] Smith et al. (2024).\n"
        "[2] Doe et al. (2024).\n\n"
        "## Acknowledgments\n\n"
        "Thanks!\n"
    )
    redacted, redactions = repo.redact(text, "README.md")
    bib = [r for r in redactions if r.reason == repo.REDACTION_BIBLIOGRAPHY]
    assert len(bib) == 1
    # Body stays, Acknowledgments stays.
    assert "Hello world." in redacted
    assert "Acknowledgments" in redacted
    # The bibliography content is gone.
    assert "Smith et al. (2024)" not in redacted
    assert "Doe et al. (2024)" not in redacted


# ---------------------------------------------------------------------
# File policy: truncation, binary detection, symlinks
# ---------------------------------------------------------------------


def test_walk_repo_truncates_large_files(tmp_path: Path):
    """Codex v3 truncate-and-flag, not skip."""
    big_readme = tmp_path / "README.md"
    big_readme.write_bytes(b"a" * (300 * 1024))  # 300 KiB
    walked = repo.walk_repo(tmp_path)
    assert len(walked.sections) == 1
    assert walked.sections[0].truncated is True
    # The file is NOT in the skipped list.
    assert walked.skipped == []
    assert any("truncated" in n for n in walked.notes)


def test_walk_repo_skips_binary_file_via_nul_byte(tmp_path: Path):
    binary_path = tmp_path / "README.md"
    binary_path.write_bytes(b"hello\x00world README content\n")
    walked = repo.walk_repo(tmp_path)
    assert walked.sections == []
    assert any(
        s.path == "README.md" and s.reason == "binary"
        for s in walked.skipped
    )


def test_walk_repo_rejects_symlink_outside_repo(tmp_path: Path):
    if os.name == "nt":  # pragma: no cover — windows symlink quirks
        pytest.skip("symlink semantics differ on windows")
    outside = tmp_path.parent / "outside_target.md"
    outside.write_text("# Outside\n", encoding="utf-8")
    inner = tmp_path / "repo"
    inner.mkdir()
    link = inner / "README.md"
    try:
        link.symlink_to(outside)
    except OSError:
        pytest.skip("symlinks not supported")
    walked = repo.walk_repo(inner)
    assert walked.sections == []
    assert any(
        s.reason == "symlink_outside_repo"
        for s in walked.skipped
    )


# ---------------------------------------------------------------------
# assemble_for_model
# ---------------------------------------------------------------------


def test_assemble_for_model_includes_section_headers():
    walked = repo.walk_repo(FIXTURES / "conflict_repo")
    assembled = repo.assemble_for_model(walked)
    assert "## source: README.md" in assembled
    assert "## source: CHANGELOG.md" in assembled


def test_assemble_for_model_returns_post_redaction_text():
    """The model must NEVER see redacted spans. Verified against
    the cites_paper_repo fixture."""
    walked = repo.walk_repo(FIXTURES / "cites_paper_repo")
    assembled = repo.assemble_for_model(walked)
    # Neither the DOI nor the inline citation should appear.
    assert "10.1234/jis.2024.012" not in assembled
    assert "(Doe and Lee, 2023)" not in assembled


# ---------------------------------------------------------------------
# Source-id resolution
# ---------------------------------------------------------------------


def test_format_source_id_ssh_origin_yields_github_id():
    sid = repo._format_source_id(
        "git@github.com:owner/proj.git", "deadbeef", "proj"
    )
    assert sid == "github:owner/proj@deadbeef"


def test_format_source_id_https_origin_yields_github_id():
    sid = repo._format_source_id(
        "https://github.com/owner/proj.git", "abc", "proj"
    )
    assert sid == "github:owner/proj@abc"


def test_format_source_id_no_origin_falls_back_to_local():
    sid = repo._format_source_id("", "abc", "myrepo")
    assert sid == "local:myrepo@abc"


def test_resolve_source_id_for_non_git_directory(tmp_path: Path):
    sid, sha = repo.resolve_source_id(tmp_path)
    assert sid.startswith("local:")
    assert sid.endswith("@no-git")
    assert sha == "no-git"
