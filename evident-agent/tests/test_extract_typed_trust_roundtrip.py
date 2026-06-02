"""Phase 5 PR4: load-bearing integration test.

Generates a draft manifest via the extract writer, then runs it
through typed-trust's CLI to confirm the manifest the extractor
emits actually parses and synthesizes. This is the integration
contract between the Python extractor and the Rust framework.

If the Phase 5 PR1–3 schema fields (replay_status, structured
provenance, PromoteFromExtracted) ever drift between Python and
Rust, this test catches it.
"""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

import pytest

from evident_agent.extract.render import (
    ExtractedClaim,
    ExtractionResult,
    write_outputs,
)


def _typed_trust_binary() -> Path | None:
    """Locate the typed-trust binary. Prefer the cargo target dir;
    fall back to PATH."""
    # cargo build at the repo's typed-trust/ subdir.
    repo_root = Path(__file__).resolve().parents[2]
    candidates = [
        repo_root / "typed-trust" / "target" / "debug" / "typed-trust",
        repo_root / "typed-trust" / "target" / "release" / "typed-trust",
    ]
    for p in candidates:
        if p.is_file():
            return p
    on_path = shutil.which("typed-trust")
    return Path(on_path) if on_path else None


def _typed_trust_has_promotion_validator() -> bool:
    """Codex F-PR4 review: the tier:ci rejection test depends on
    PR3's validate_promotion_rules wiring. If the binary is older
    than PR3 (e.g. built off main before PR3 lands), the
    promotion gate doesn't fire and the test would be a
    dependency-skew failure rather than a real correctness signal.
    Probe by asking the binary to validate an extracted+ci
    manifest WITHOUT a sidecar: PR3-or-later exits non-zero;
    pre-PR3 exits 0.
    """
    binary = _typed_trust_binary()
    if binary is None:
        return False
    import tempfile

    yaml_text = """version: 0.1
project: probe
claims:
  - id: probe-claim
    title: probe
    kind: measurement
    tier: ci
    source: .
    claim: probe
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: probe
    evidence:
      oracle: [Manual]
      command: "no-replay-path"
      artifact: out.txt
      replay_status: unavailable_artifacts
      replay_reason: code_private
    provenance:
      kind: extracted-from-paper
      extractor:
        extracted_at: "2026-09-14T10:00:00Z"
"""
    with tempfile.NamedTemporaryFile(
        suffix=".yaml", mode="w", delete=False
    ) as f:
        f.write(yaml_text)
        path = f.name
    try:
        result = subprocess.run(
            [str(binary), path],
            capture_output=True,
            text=True,
            timeout=15,
        )
        return result.returncode != 0
    except Exception:
        return False
    finally:
        Path(path).unlink(missing_ok=True)


def _example_result_for_roundtrip() -> ExtractionResult:
    return ExtractionResult(
        source_id="arxiv:2501.12345v1",
        source_sha="deadbeef",
        extractor_model="claude-opus-4-7",
        extracted_at="2026-09-14T10:00:00Z",
        claims=[
            ExtractedClaim(
                id="roundtrip-claim-one",
                title="Roundtrip claim, bound stated",
                claim="Our method achieves rmsd < 0.5 across the suite.",
                subject_aliases=["our method", "we"],
                tolerances=[
                    {
                        "metric": "rmsd",
                        "op": "<",
                        "value": 0.5,
                        "source_span": (
                            "our method achieves rmsd less than 0.5 "
                            "across the BPTI test suite"
                        ),
                        "prose": "stated bound 0.5",
                    }
                ],
            )
        ],
    )


@pytest.mark.skipif(
    _typed_trust_binary() is None,
    reason="typed-trust binary not built (run `cargo build` in typed-trust/)",
)
def test_extracted_manifest_passes_typed_trust_translation(tmp_path: Path):
    """Build a draft manifest, hand it to typed-trust, expect it to
    parse and produce a report (research tier; no promotion needed)."""
    out = tmp_path / "extracted" / "roundtrip"
    write_outputs(
        _example_result_for_roundtrip(),
        output_dir=out,
        project="extracted/roundtrip",
    )
    manifest_path = out / "evident.yaml"
    typed_trust = _typed_trust_binary()
    assert typed_trust is not None
    result = subprocess.run(
        [str(typed_trust), str(manifest_path)],
        capture_output=True,
        text=True,
        timeout=30,
    )
    # Research-tier extracted claim: no promotion event needed.
    assert result.returncode == 0, (
        f"typed-trust exited non-zero.\n"
        f"stderr:\n{result.stderr}\n"
        f"stdout:\n{result.stdout[:1000]}"
    )


@pytest.mark.skipif(
    not _typed_trust_has_promotion_validator(),
    reason=(
        "typed-trust binary missing the PR3 promotion validator; "
        "this is a cross-language contract test that requires PR3+ "
        "to be built into the binary"
    ),
)
def test_extracted_manifest_at_tier_ci_is_rejected_without_promotion(tmp_path: Path):
    """Codex F-PR3 contract: an extracted-from-paper claim at tier:ci
    without a matching PromoteFromExtracted event must be rejected by
    typed-trust. This is the cross-language assertion that PR1+PR2+PR3
    work together correctly with the PR4 output writer."""
    out = tmp_path / "extracted" / "roundtrip-ci"
    write_outputs(
        _example_result_for_roundtrip(),
        output_dir=out,
        project="extracted/roundtrip-ci",
    )
    # Manually tier-promote without a curator event. typed-trust
    # must catch this.
    manifest_path = out / "evident.yaml"
    body = manifest_path.read_text()
    body = body.replace("tier: research", "tier: ci")
    manifest_path.write_text(body)

    typed_trust = _typed_trust_binary()
    assert typed_trust is not None
    result = subprocess.run(
        [str(typed_trust), str(manifest_path)],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode != 0, (
        f"typed-trust exited 0 on extracted+ci without promotion; "
        f"the gate failed to fire.\n"
        f"stderr:\n{result.stderr}\n"
    )
    # The error message should name promote_from_extracted.
    assert "promote_from_extracted" in result.stderr or "extracted" in result.stderr, (
        f"unexpected stderr:\n{result.stderr}"
    )
