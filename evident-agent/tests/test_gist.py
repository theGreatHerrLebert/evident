"""`gist`: a claim told plainly (what / why / wrong_if) for fast screening.
The validator bounds it; typed-trust shows it verbatim, labelled as the
author's summary, in the markdown and site renderings."""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest

import evident_agent.typed_trust as tt
from test_per_criterion_values import VALID_MANIFEST, _validate

GIST = """    gist:
      what: Three checks on one run hold.
      why: Each one guards a different way the run could mislead.
      wrong_if: The error exceeds one percent or any false positive appears.
"""


def _with_gist(gist: str = GIST, manifest: str = VALID_MANIFEST) -> str:
    return manifest.replace("    assumptions: [none]\n", gist + "    assumptions: [none]\n")


def test_validator_accepts_a_gist(tmp_path: Path) -> None:
    r = _validate(tmp_path, _with_gist())
    assert r.returncode == 0, r.stdout + r.stderr


@pytest.mark.parametrize(
    "gist, message",
    [
        (GIST.replace("      wrong_if:", "      falsified_by:"), "unknown keys"),
        ("\n".join(GIST.splitlines()[:3]) + "\n", "gist.wrong_if is required"),
        (GIST.replace("Three checks on one run hold.", '""'), "gist.what must be a non-empty string"),
        (GIST.replace("Three checks on one run hold.", "x" * 301), "keep it under 300"),
        ("    gist: just a sentence\n", "gist must be a mapping"),
    ],
)
def test_validator_rejects_bad_gists(tmp_path: Path, gist: str, message: str) -> None:
    r = _validate(tmp_path, _with_gist(gist))
    assert r.returncode != 0
    assert message in r.stdout + r.stderr, r.stdout + r.stderr


def _typed_trust() -> str:
    binary = tt.find_binary()
    if not (Path(binary).is_file() or shutil.which(binary)):
        pytest.skip("typed-trust binary not built")
    return binary


def _render(tmp_path: Path, fmt: str) -> str:
    binary = _typed_trust()
    path = tmp_path / "evident.yaml"
    path.write_text(_with_gist())
    r = subprocess.run([binary, "--format", fmt, str(path)], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return r.stdout


def test_markdown_quotes_the_gist(tmp_path: Path) -> None:
    md = _render(tmp_path, "md")
    assert "> **In short:** Three checks on one run hold." in md
    assert "> **Why it matters:** Each one guards" in md
    assert "> **Wrong if:** The error exceeds one percent" in md


def test_json_report_carries_the_gist(tmp_path: Path) -> None:
    report = json.loads(_render(tmp_path, "json"))["reports"][0]
    assert report["gist"]["what"] == "Three checks on one run hold."


def test_site_embeds_the_gist(tmp_path: Path) -> None:
    html = _render(tmp_path, "site")
    assert "Three checks on one run hold." in html
    assert "the author’s summary, not a result" in html


def test_a_claim_without_a_gist_renders_as_before(tmp_path: Path) -> None:
    binary = _typed_trust()
    path = tmp_path / "evident.yaml"
    path.write_text(VALID_MANIFEST)
    r = subprocess.run([binary, "--format", "md", str(path)], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    assert "In short" not in r.stdout
    report = json.loads(
        subprocess.run([binary, "--format", "json", str(path)], capture_output=True, text=True).stdout
    )["reports"][0]
    assert "gist" not in report


# ---------------------------------------------------------------------------
# Review fixes: parity with the engine, failure scope, escaping
# ---------------------------------------------------------------------------


def _engine(tmp_path: Path, manifest: str, fmt: str = "json") -> subprocess.CompletedProcess:
    binary = _typed_trust()
    path = tmp_path / "evident.yaml"
    path.write_text(manifest)
    return subprocess.run([binary, "--format", fmt, str(path)], capture_output=True, text=True)


def test_validator_rejects_duplicate_keys(tmp_path: Path) -> None:
    """PyYAML keeps the last of two keys; the engine refuses the manifest."""
    dup = GIST.replace("      why:", "      what: A second what.\n      why:")
    r = _validate(tmp_path, _with_gist(dup))
    assert r.returncode != 0
    assert "duplicate key 'what'" in r.stdout + r.stderr, r.stdout + r.stderr


@pytest.mark.parametrize(
    "gist",
    [
        "    gist: null\n",
        GIST.replace("Three checks on one run hold.", "42"),
        GIST.replace("Three checks on one run hold.", "x" * 301),
        "\n".join(GIST.splitlines()[:3]) + "\n",  # measurement without wrong_if
    ],
)
def test_validator_and_engine_both_reject(tmp_path: Path, gist: str) -> None:
    manifest = _with_gist(gist)
    assert _validate(tmp_path, manifest).returncode != 0
    r = _engine(tmp_path, manifest)
    skipped = json.loads(r.stdout)["skipped"]
    assert [s["fatal"] for s in skipped] == [True], skipped
    assert "gist" in skipped[0]["reason"]


def test_a_bad_gist_fails_only_its_own_claim(tmp_path: Path) -> None:
    good = _with_gist()
    second = good[good.index("  - id: multi"):].replace("id: multi", "id: other").replace(
        "Three checks on one run hold.", "42"
    )
    r = _engine(tmp_path, good + second)
    out = json.loads(r.stdout)
    assert [rep["claim"] for rep in out["reports"]] == ["multi"]
    assert [s["id"] for s in out["skipped"]] == ["other"]


HOSTILE = "**bold** [link](https://example.com) ![img](x) <img src=x onerror=alert(1)> <!--<script> `code`"


def _hostile_manifest() -> str:
    return _with_gist(GIST.replace("Three checks on one run hold.", json.dumps(HOSTILE)))


def test_markdown_escapes_and_labels_the_gist(tmp_path: Path) -> None:
    md = _engine(tmp_path, _hostile_manifest(), "md").stdout
    assert "the author's summary, not a result" in md
    line = next(l for l in md.splitlines() if l.startswith("> **In short:**"))
    assert "<img" not in line and "&lt;img" in line
    assert "\\*\\*bold\\*\\*" in line and "\\[link\\]" in line and "\\`code\\`" in line


def test_html_report_shows_the_escaped_gist(tmp_path: Path) -> None:
    html = _engine(tmp_path, _hostile_manifest(), "html").stdout
    assert "the author's summary, not a result" in html
    assert "&lt;img src=x onerror=alert(1)&gt;" in html
    assert "<img src=x" not in html


def test_site_data_block_cannot_be_closed_by_manifest_text(tmp_path: Path) -> None:
    html = _engine(tmp_path, _hostile_manifest(), "site").stdout
    start = html.index('<script id="evident-data" type="application/json">') + len(
        '<script id="evident-data" type="application/json">'
    )
    end = html.index("</script>", start)
    island = html[start:end]
    assert "<" not in island and ">" not in island and "&" not in island
    data = json.loads(island)
    assert data["claims"][0]["gist"]["what"] == HOSTILE
    # The page after the data block is still there.
    assert '<div id="site">' in html[end:]


def test_site_shows_the_gist_once(tmp_path: Path) -> None:
    html = _engine(tmp_path, _with_gist(), "site").stdout
    start = html.index('<script id="evident-data" type="application/json">')
    data = json.loads(html[start:].split(">", 1)[1].split("</script>", 1)[0])
    assert data["claims"][0]["gist"]["what"] == "Three checks on one run hold."
    assert "In plain words" not in data["fragments"]["multi"]


@pytest.mark.parametrize(
    "gist",
    [
        "    gist: just a sentence\n",
        "    gist:\n      - what\n      - why\n",
        GIST + "      falsified_by: d\n",
    ],
)
def test_gist_shape_errors_fail_only_their_claim(tmp_path: Path, gist: str) -> None:
    good = _with_gist()
    second = _with_gist(gist)
    second = second[second.index("  - id: multi"):].replace("id: multi", "id: other")
    manifest = good + second
    assert _validate(tmp_path, manifest).returncode != 0
    out = json.loads(_engine(tmp_path, manifest).stdout)
    assert [rep["claim"] for rep in out["reports"]] == ["multi"]
    assert [(s["id"], s["fatal"]) for s in out["skipped"]] == [("other", True)]


def test_validator_accepts_merge_keys(tmp_path: Path) -> None:
    """A `<<` merge may be overridden by an explicit key; that is not a
    duplicate."""
    manifest = VALID_MANIFEST.replace(
        "    pinned_versions:\n",
        "    pinned_versions:\n      <<: {test: \"0.9\"}\n",
    )
    r = _validate(tmp_path, manifest)
    assert r.returncode == 0, r.stdout + r.stderr
