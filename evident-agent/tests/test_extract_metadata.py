"""Phase 5 PR5b: tests for the deterministic metadata walker.

Covers:
- pyproject.toml extraction (requires-python, name, version)
- Cargo.toml extraction (rust-version / MSRV, edition, name, version)
- package.json extraction (name, version, engines.node)
- multi-file repo emits claims from every recognized file
- empty repo (no config files) returns no claims and a note
- claim ids are deterministic and slug-safe
- manifest render produces tier:research + extracted-from-repo
  provenance + metadata block (no tolerances/evidence.command)
- cross-language: the generated manifest passes typed-trust's
  translator
"""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

import pytest
import yaml

from evident_agent.extract import metadata as mdwalker


FIXTURES = (
    Path(__file__).resolve().parent / "fixtures" / "extract" / "metadata"
)


# ---------------------------------------------------------------------
# Per-file extractors
# ---------------------------------------------------------------------


def test_pyproject_emits_requires_python_name_version():
    result = mdwalker.walk_repo_metadata(FIXTURES / "pyproject_repo")
    fields = {c.metadata_field for c in result.claims}
    assert "python_version_requirement" in fields
    assert "project_name" in fields
    assert "project_version" in fields
    by_field = {c.metadata_field: c for c in result.claims}
    assert by_field["python_version_requirement"].declared_value == ">=3.10"
    assert by_field["project_version"].declared_value == "0.4.2"


def test_cargo_emits_rust_msrv_edition_name_version():
    result = mdwalker.walk_repo_metadata(FIXTURES / "cargo_repo")
    by_field = {c.metadata_field: c for c in result.claims}
    assert by_field["rust_msrv"].declared_value == "1.67"
    assert by_field["rust_edition"].declared_value == "2021"
    assert by_field["cargo_package_name"].declared_value == "fictional-cargo"
    assert by_field["cargo_package_version"].declared_value == "0.2.1"


def test_package_json_emits_name_version_engines_node():
    result = mdwalker.walk_repo_metadata(FIXTURES / "package_json_repo")
    by_field = {c.metadata_field: c for c in result.claims}
    assert by_field["npm_package_name"].declared_value == "fictional-npm-pkg"
    assert by_field["npm_package_version"].declared_value == "1.2.3"
    assert by_field["node_version_requirement"].declared_value == ">=18.0.0"


def test_multi_file_repo_emits_claims_from_each_file():
    result = mdwalker.walk_repo_metadata(FIXTURES / "multi_file_repo")
    fields = {c.metadata_field for c in result.claims}
    # pyproject contributions
    assert "python_version_requirement" in fields
    # cargo contributions
    assert "rust_msrv" in fields
    assert "rust_edition" in fields


def test_empty_repo_emits_no_claims_and_one_note(tmp_path: Path):
    # Use an actual empty dir from tmp_path (the fixture's empty_repo
    # might be empty, but to be safe use tmp).
    result = mdwalker.walk_repo_metadata(tmp_path)
    assert result.claims == []
    assert any("no pyproject" in n.lower() for n in result.notes)


def test_pyproject_with_no_project_section_skips_with_recognised_reason(
    tmp_path: Path,
):
    """A pyproject.toml that only has [build-system] is recognised
    as parseable but has no recognized compatibility fields."""
    (tmp_path / "pyproject.toml").write_text(
        '[build-system]\nrequires = ["hatchling>=1.18"]\n'
    )
    result = mdwalker.walk_repo_metadata(tmp_path)
    assert result.claims == []
    assert len(result.skipped_files) == 1
    assert result.skipped_files[0].path == "pyproject.toml"
    assert result.skipped_files[0].reason == "no_recognised_fields"


def test_malformed_toml_skip_reason_is_parse_error(tmp_path: Path):
    """Codex F-PR5b-CR2 (P2): a corrupt config file must be
    distinguished from one with no recognized fields. EXTRACTION.md
    can then tell the curator whether to investigate."""
    (tmp_path / "pyproject.toml").write_text(
        "this = is not = valid toml\n"
    )
    result = mdwalker.walk_repo_metadata(tmp_path)
    assert result.claims == []
    assert len(result.skipped_files) == 1
    assert result.skipped_files[0].reason == "parse_error"
    assert "parse error" in (result.skipped_files[0].detail or "").lower()


def test_malformed_json_skip_reason_is_parse_error(tmp_path: Path):
    (tmp_path / "package.json").write_text("{ this: is not json }\n")
    result = mdwalker.walk_repo_metadata(tmp_path)
    assert result.claims == []
    assert len(result.skipped_files) == 1
    assert result.skipped_files[0].reason == "parse_error"


# ---------------------------------------------------------------------
# Claim id discipline
# ---------------------------------------------------------------------


def test_claim_id_prefix_uses_source_id_when_available(tmp_path: Path):
    """Codex F-PR5b-CR3 (P2/P3): two repos with the same basename
    must NOT generate colliding claim ids. The walker now uses
    source_id-derived prefix when a stable id is available."""
    # Same basename, different source_id (passed explicitly).
    (tmp_path / "a").mkdir()
    (tmp_path / "a" / "pyproject.toml").write_text(
        '[project]\nname = "p"\nversion = "0.1.0"\nrequires-python = ">=3.10"\n'
    )
    r_a = mdwalker.walk_repo_metadata(
        tmp_path / "a", source_id="github:org-a/project@deadbeef",
    )
    r_b = mdwalker.walk_repo_metadata(
        tmp_path / "a", source_id="github:org-b/project@cafebabe",
    )
    ids_a = {c.id for c in r_a.claims}
    ids_b = {c.id for c in r_b.claims}
    assert ids_a != ids_b
    # Each set carries the owner in the prefix.
    assert any("org-a" in i for i in ids_a)
    assert any("org-b" in i for i in ids_b)


def test_claim_ids_are_stable_and_slug_safe(tmp_path: Path):
    """Claim ids are derived from repo basename + source path. They
    must be ASCII alphanumeric + hyphen/underscore for downstream
    file-system safety."""
    weird = tmp_path / "My Repo (v2)"
    weird.mkdir()
    (weird / "pyproject.toml").write_text(
        '[project]\nname = "p"\nversion = "0.1.0"\nrequires-python = ">=3.10"\n'
    )
    result = mdwalker.walk_repo_metadata(weird)
    for c in result.claims:
        assert c.id == c.id.lower()
        for ch in c.id:
            assert ch.isalnum() or ch in "-_", (
                f"unexpected char in claim id {c.id!r}"
            )


# ---------------------------------------------------------------------
# Manifest render
# ---------------------------------------------------------------------


def test_render_manifest_produces_tier_research_metadata_claims():
    result = mdwalker.walk_repo_metadata(FIXTURES / "pyproject_repo")
    manifest = mdwalker.render_metadata_manifest(
        result, project="extracted/pyproject-repo",
    )
    assert manifest["version"] == "0.1"
    for c in manifest["claims"]:
        assert c["kind"] == "metadata_compatibility"
        assert c["tier"] == "research"
        assert "metadata" in c
        # Critical: no tolerances, no evidence.command — those
        # would trip typed-trust's metadata-vs-measurement disjoint
        # check.
        assert "tolerances" not in c
        assert "evidence" not in c
        assert c["provenance"]["kind"] == "extracted-from-repo"


def test_render_manifest_metadata_block_carries_all_four_fields():
    result = mdwalker.walk_repo_metadata(FIXTURES / "cargo_repo")
    manifest = mdwalker.render_metadata_manifest(
        result, project="extracted/cargo-repo",
    )
    by_id = {c["id"]: c for c in manifest["claims"]}
    msrv_id = next(
        cid for cid in by_id
        if "cargo-rust-msrv" in cid
    )
    block = by_id[msrv_id]["metadata"]
    assert block["field"] == "rust_msrv"
    assert block["declared_value"] == "1.67"
    assert block["source_file"] == "Cargo.toml"
    assert block["source_path"] == "package.rust-version"


# ---------------------------------------------------------------------
# Cross-language: generated manifest parses through typed-trust
# ---------------------------------------------------------------------


def _typed_trust_binary() -> Path | None:
    repo_root = Path(__file__).resolve().parents[2]
    for p in (
        repo_root / "typed-trust" / "target" / "debug" / "typed-trust",
        repo_root / "typed-trust" / "target" / "release" / "typed-trust",
    ):
        if p.is_file():
            return p
    on_path = shutil.which("typed-trust")
    return Path(on_path) if on_path else None


@pytest.mark.skipif(
    _typed_trust_binary() is None,
    reason="typed-trust binary not built (run `cargo build` in typed-trust/)",
)
def test_generated_metadata_manifest_parses_through_typed_trust(
    tmp_path: Path,
):
    """End-to-end: walk a fixture repo, render the manifest, run
    typed-trust against it, assert exit 0. Proves the Python walker
    + Rust translator are byte-compatible on the metadata schema."""
    result = mdwalker.walk_repo_metadata(FIXTURES / "multi_file_repo")
    manifest = mdwalker.render_metadata_manifest(
        result, project="extracted/multi-file-repo",
        extracted_at="2026-05-01T10:00:00Z",
    )
    manifest_path = tmp_path / "evident.yaml"
    manifest_path.write_text(yaml.safe_dump(manifest, sort_keys=False))
    binary = _typed_trust_binary()
    assert binary is not None
    result_proc = subprocess.run(
        [str(binary), str(manifest_path)],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result_proc.returncode == 0, (
        f"typed-trust rejected the metadata manifest.\n"
        f"stderr:\n{result_proc.stderr}\n"
        f"stdout:\n{result_proc.stdout[:500]}"
    )


# ---------------------------------------------------------------------
# CLI integration
# ---------------------------------------------------------------------


def test_cli_extract_metadata_writes_yaml_and_md(tmp_path: Path):
    from click.testing import CliRunner
    from evident_agent.cli import main

    out = tmp_path / "out"
    runner = CliRunner()
    result = runner.invoke(
        main,
        [
            "extract-metadata",
            "--repo", str(FIXTURES / "multi_file_repo"),
            "--output-dir", str(out),
        ],
    )
    assert result.exit_code == 0, result.output
    assert (out / "evident.yaml").is_file()
    assert (out / "EXTRACTION.md").is_file()
    md = (out / "EXTRACTION.md").read_text()
    assert "Emitted claims" in md
    manifest = yaml.safe_load((out / "evident.yaml").read_text())
    assert all(
        c["kind"] == "metadata_compatibility" for c in manifest["claims"]
    )
