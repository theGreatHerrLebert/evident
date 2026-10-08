"""Per-criterion observations: scoring recovers one value per tolerance
output, replay records them as ``values`` in the sidecar, and an empty
extraction leaves the previous entry alone."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path
from textwrap import dedent

import pytest

import evident_agent.typed_trust as tt
from evident_agent.replay import run_replay
from evident_agent.scoring import extract_observations
from evident_agent.sidecar import LastVerifiedEntry, read, write

CLAIM = {
    "id": "multi",
    "tolerances": [
        {"metric": "relative_error", "op": "<", "value": 0.01, "output": "err", "prose": "x"},
        {"metric": "count", "op": "==", "value": 0, "output": "fp", "prose": "x"},
        {"metric": "recall", "op": ">=", "value": 0.75, "output": "recall", "prose": "x"},
    ],
    "evidence": {"artifact": "out.json"},
}


def _artifact(tmp_path: Path, payload) -> Path:
    (tmp_path / "out.json").write_text(json.dumps(payload))
    return tmp_path


# ---------------------------------------------------------------------------
# scoring
# ---------------------------------------------------------------------------


def test_fallback_reads_values_by_output(tmp_path: Path) -> None:
    src = _artifact(tmp_path, {"value": 0.004, "values": {"err": 0.004, "fp": 0, "recall": 0.9}})
    obs = extract_observations(CLAIM, src)
    assert obs.value == 0.004
    assert obs.values == {"err": 0.004, "fp": 0.0, "recall": 0.9}
    assert obs.ignored == []


def test_fallback_drops_undeclared_and_non_numeric_values(tmp_path: Path) -> None:
    src = _artifact(
        tmp_path,
        {"values": {"err": 0.004, "typo": 1.0, "fp": True, "recall": "0.9"}},
    )
    obs = extract_observations(CLAIM, src)
    assert obs.value is None
    assert obs.values == {"err": 0.004}
    assert obs.ignored == ["typo"]


def test_empty_extraction_is_falsy(tmp_path: Path) -> None:
    assert not extract_observations(CLAIM, tmp_path)


def test_project_scoring_is_called_positionally(tmp_path: Path) -> None:
    """proteon's score_claim(claim, repo_root): the previous base_dir=
    keyword raised TypeError on every call and was swallowed."""
    tools = tmp_path / "evident" / "tools"
    tools.mkdir(parents=True)
    (tools / "claim_scoring.py").write_text(
        dedent(
            """
            from types import SimpleNamespace

            def score_claim(claim, repo_root):
                vals = [0.002, 1.0, None]
                return SimpleNamespace(tolerances=[
                    SimpleNamespace(observed=v, output=t["output"])
                    for v, t in zip(vals, claim["tolerances"])
                ])
            """
        )
    )
    obs = extract_observations(CLAIM, tmp_path)
    assert obs.value == 0.002
    assert obs.values == {"err": 0.002, "fp": 1.0}


# ---------------------------------------------------------------------------
# sidecar
# ---------------------------------------------------------------------------


def test_sidecar_round_trips_values(tmp_path: Path) -> None:
    path = tmp_path / "lv.json"
    write(path, {"multi": LastVerifiedEntry(date="2026-10-08", values={"err": 0.004})})
    assert json.loads(path.read_text()) == {"multi": {"date": "2026-10-08", "values": {"err": 0.004}}}
    assert read(path)["multi"].values == {"err": 0.004}


# ---------------------------------------------------------------------------
# replay
# ---------------------------------------------------------------------------


def _manifest(tmp_path: Path) -> Path:
    m = tmp_path / "evident.yaml"
    m.write_text(
        dedent(
            """
            version: 0.1
            project: test
            claims:
              - id: multi
                kind: measurement
                tier: research
                subsystem: io
                source: .
                title: three checks
                claim: three checks on one run
                trust_strategy: [validation]
                tolerances:
                  - {metric: relative_error, op: "<", value: 0.01, output: err, prose: x}
                  - {metric: count, op: "==", value: 0, output: fp, prose: x}
                  - {metric: recall, op: ">=", value: 0.75, output: recall, prose: x}
                evidence:
                  oracle: [Test]
                  command: "true"
                  artifact: out.json
                assumptions: [none]
                failure_modes: [none]
            """
        ).strip()
        + "\n"
    )
    return m


def test_replay_records_values(tmp_path: Path) -> None:
    m = _manifest(tmp_path)
    _artifact(tmp_path, {"value": 0.004, "values": {"err": 0.004, "fp": 2, "recall": 0.9}})
    result = run_replay(manifest_path=m, no_execute=True)
    entry = read(tmp_path / "last_verified.json")["multi"]
    assert entry.value == 0.004
    assert entry.values == {"err": 0.004, "fp": 2.0, "recall": 0.9}
    assert result.claims[0].observed_values == entry.values


def test_replay_keeps_previous_entry_when_nothing_is_extracted(tmp_path: Path) -> None:
    m = _manifest(tmp_path)
    sidecar_path = tmp_path / "last_verified.json"
    previous = LastVerifiedEntry(commit="abc", date="2026-09-01", value=0.003)
    write(sidecar_path, {"multi": previous})

    run_replay(manifest_path=m, no_execute=True)  # no out.json

    assert read(sidecar_path)["multi"] == previous


def test_failed_run_still_replaces_a_stale_pass(tmp_path: Path, monkeypatch) -> None:
    import evident_agent.docker as docker

    m = _manifest(tmp_path)
    sidecar_path = tmp_path / "last_verified.json"
    write(sidecar_path, {"multi": LastVerifiedEntry(date="2026-09-01", value=0.003)})
    _artifact(tmp_path, {"value": 0.004})

    class Failed:
        exit_code, duration_s, stderr_tail, timed_out = 1, 0.1, "boom", False

    monkeypatch.setattr(docker, "run", lambda **kw: Failed())
    run_replay(manifest_path=m)

    entry = read(sidecar_path)["multi"]
    assert entry.date != "2026-09-01"
    assert entry.value is None and entry.values is None


def test_replay_render_assesses_every_criterion(tmp_path: Path) -> None:
    binary = tt.find_binary()
    if not (Path(binary).is_file() or shutil.which(binary)):
        pytest.skip("typed-trust binary not built")
    m = _manifest(tmp_path)
    _artifact(tmp_path, {"value": 0.004, "values": {"err": 0.004, "fp": 2, "recall": 0.9}})
    result = run_replay(manifest_path=m, no_execute=True, render="json")
    report = json.loads(result.rendered)["reports"][0]
    outcomes = [c["result"]["value"]["type"] for c in report["criteria"]]
    assert outcomes == ["pass", "fail", "pass"], outcomes


# ---------------------------------------------------------------------------
# workflow/validate_manifest.py
# ---------------------------------------------------------------------------

VALIDATOR = Path(__file__).resolve().parents[2] / "workflow" / "validate_manifest.py"

VALID_MANIFEST = """version: 0.1
project: test
vocabularies:
  oracle: [Test]
  subsystem: [io]
  tolerance_metric: [relative_error, count]
claims:
  - id: multi
    title: three checks
    kind: measurement
    tier: research
    subsystem: io
    case: cases/m.md
    source: src
    trust_strategy: [validation]
    claim: three checks on one run
    inputs:
      fixtures: [none]
    pinned_versions:
      test: "1.0"
      Test: "1.0"
    tolerances:
      - {metric: relative_error, op: "<", value: 0.01, output: err, prose: x}
      - {metric: count, op: "==", value: 0, output: fp, prose: x}
    evidence:
      oracle: [Test]
      command: "true"
      artifact: out.json
    assumptions: [none]
    failure_modes: [none]
    last_verified:
      date: "2026-10-08"
      values: {err: 0.004, fp: 0}
"""


def _validate(tmp_path: Path, manifest: str) -> subprocess.CompletedProcess:
    (tmp_path / "src").mkdir(exist_ok=True)
    (tmp_path / "cases").mkdir(exist_ok=True)
    (tmp_path / "cases" / "m.md").touch()
    path = tmp_path / "evident.yaml"
    path.write_text(manifest)
    return subprocess.run(
        [sys.executable, str(VALIDATOR), str(path)], capture_output=True, text=True
    )


def test_validator_accepts_values_keyed_by_output(tmp_path: Path) -> None:
    r = _validate(tmp_path, VALID_MANIFEST)
    assert r.returncode == 0, r.stdout + r.stderr


@pytest.mark.parametrize(
    "values, message",
    [
        ("{err: 0.004, typo: 1}", "no tolerance declares"),
        ("{err: \"0.004\"}", "finite number"),
        ("{err: true}", "finite number"),
        ("{err: .nan}", "finite number"),
        ("[0.004, 0]", "must be a mapping"),
    ],
)
def test_validator_rejects_bad_values(tmp_path: Path, values: str, message: str) -> None:
    manifest = VALID_MANIFEST.replace("values: {err: 0.004, fp: 0}", f"values: {values}")
    r = _validate(tmp_path, manifest)
    assert r.returncode != 0
    assert message in r.stdout + r.stderr, r.stdout + r.stderr


def test_validator_rejects_values_for_a_shared_output(tmp_path: Path) -> None:
    manifest = VALID_MANIFEST.replace("output: fp", "output: err").replace(
        "values: {err: 0.004, fp: 0}", "values: {err: 0.004}"
    )
    r = _validate(tmp_path, manifest)
    assert r.returncode != 0
    assert "several tolerances" in r.stdout + r.stderr, r.stdout + r.stderr


# ---------------------------------------------------------------------------
# Producer/consumer boundary: replay never writes what typed-trust rejects
# ---------------------------------------------------------------------------


def test_conflicting_value_and_values_is_not_written(tmp_path: Path) -> None:
    m = _manifest(tmp_path)
    sidecar_path = tmp_path / "last_verified.json"
    previous = LastVerifiedEntry(date="2026-09-01", value=0.003)
    write(sidecar_path, {"multi": previous})
    _artifact(tmp_path, {"value": 0.004, "values": {"err": 0.005}})

    obs = extract_observations(CLAIM, tmp_path)
    assert obs.conflict and "first criterion" in obs.conflict
    run_replay(manifest_path=m, no_execute=True)
    assert read(sidecar_path)["multi"] == previous


@pytest.mark.parametrize(
    "tolerances, kept, ignored",
    [
        # undeclared output
        (CLAIM["tolerances"], {"err": 0.004}, ["typo"]),
        # output shared by two tolerances: ambiguous
        (
            [dict(CLAIM["tolerances"][0]), dict(CLAIM["tolerances"][1], output="err")],
            {},
            ["err", "typo"],
        ),
        # prose-only tolerance: no criterion to bind to
        (
            [{"output": "err", "prose": "x"}, CLAIM["tolerances"][1]],
            {},
            ["err", "typo"],
        ),
    ],
)
def test_unbindable_outputs_are_ignored(tmp_path: Path, tolerances, kept, ignored) -> None:
    claim = dict(CLAIM, tolerances=tolerances)
    _artifact(tmp_path, {"values": {"err": 0.004, "typo": 1.0}})
    obs = extract_observations(claim, tmp_path)
    assert obs.values == kept
    assert sorted(obs.ignored) == ignored


def _project_scorer(tmp_path: Path, observed: str, outputs: str = 'output=t.get("output")') -> None:
    tools = tmp_path / "evident" / "tools"
    tools.mkdir(parents=True, exist_ok=True)
    (tools / "claim_scoring.py").write_text(
        dedent(
            f"""
            from dataclasses import dataclass
            from types import SimpleNamespace

            @dataclass
            class Score:
                tolerances: list

            def score_claim(claim, repo_root):
                vals = {observed}
                return Score([
                    SimpleNamespace(observed=v, {outputs})
                    for v, t in zip(vals, claim["tolerances"])
                ])
            """
        )
    )


def test_project_scorer_outputs_are_filtered_too(tmp_path: Path) -> None:
    _project_scorer(tmp_path, "[0.002, 1.0]", outputs='output="renamed"')
    obs = extract_observations(CLAIM, tmp_path)
    assert obs.value == 0.002
    assert obs.values == {}
    assert obs.ignored == ["renamed"]


def test_empty_project_result_falls_back_to_json(tmp_path: Path) -> None:
    _project_scorer(tmp_path, "[None, None, None]")
    _artifact(tmp_path, {"values": {"fp": 0}})
    assert extract_observations(CLAIM, tmp_path).values == {"fp": 0.0}


def test_project_scorer_does_not_touch_a_cached_claim_scoring(tmp_path: Path, monkeypatch) -> None:
    import types

    host = types.ModuleType("claim_scoring")
    host.score_claim = lambda claim, root: pytest.fail("the host's module was used")
    monkeypatch.setitem(sys.modules, "claim_scoring", host)
    _project_scorer(tmp_path, "[0.002, 1.0]")

    obs = extract_observations(CLAIM, tmp_path)
    assert obs.values == {"err": 0.002, "fp": 1.0}
    assert sys.modules["claim_scoring"] is host
    assert not [n for n in sys.modules if n.startswith("_evident_project_claim_scoring_")]


@pytest.mark.parametrize(
    "payload",
    [
        {"value": 0.004, "values": {"err": 0.005}},
        {"values": {"err": 0.004, "typo": 1.0}},
        {"value": 0.004, "values": {"err": 0.004, "fp": 2, "recall": 0.9, "x": 3}},
    ],
)
def test_whatever_replay_writes_typed_trust_accepts(tmp_path: Path, payload) -> None:
    binary = tt.find_binary()
    if not (Path(binary).is_file() or shutil.which(binary)):
        pytest.skip("typed-trust binary not built")
    m = _manifest(tmp_path)
    _artifact(tmp_path, payload)
    result = run_replay(manifest_path=m, no_execute=True, render="json")
    rendered = json.loads(result.rendered)
    assert [s for s in rendered["skipped"] if s.get("fatal")] == []
    assert len(rendered["reports"]) == 1


@pytest.mark.parametrize(
    "edit, message",
    [
        (lambda m: m.replace("date: \"2026-10-08\"", "date: \"2026-10-08\"\n      value: 0.005"), "disagree"),
        (lambda m: m.replace("date: \"2026-10-08\"", "date: \"2026-10-08\"\n      value: .nan"), "finite"),
        (
            lambda m: m.replace(
                "- {metric: relative_error, op: \"<\", value: 0.01, output: err, prose: x}",
                "- {output: err, prose: x}",
            ),
            "no tolerance declares",
        ),
    ],
)
def test_validator_matches_engine_rules(tmp_path: Path, edit, message) -> None:
    r = _validate(tmp_path, edit(VALID_MANIFEST))
    assert r.returncode != 0
    assert message in r.stdout + r.stderr, r.stdout + r.stderr


def test_validator_accepts_agreeing_value_and_values(tmp_path: Path) -> None:
    manifest = VALID_MANIFEST.replace("date: \"2026-10-08\"", "date: \"2026-10-08\"\n      value: 0.004")
    r = _validate(tmp_path, manifest)
    assert r.returncode == 0, r.stdout + r.stderr


# ---------------------------------------------------------------------------
# Second review: null triples, huge integers, helper-module cleanup
# ---------------------------------------------------------------------------


def test_null_triple_is_prose_only(tmp_path: Path) -> None:
    """typed-trust reads `metric: null` as absent, so this tolerance has no
    criterion and its output cannot carry a value."""
    claim = dict(
        CLAIM,
        tolerances=[{"metric": None, "op": None, "value": None, "output": "err", "prose": "x"}],
    )
    _artifact(tmp_path, {"values": {"err": 0.004}})
    obs = extract_observations(claim, tmp_path)
    assert obs.values == {} and obs.ignored == ["err"]


def test_replay_with_a_null_triple_is_accepted_by_typed_trust(tmp_path: Path) -> None:
    binary = tt.find_binary()
    if not (Path(binary).is_file() or shutil.which(binary)):
        pytest.skip("typed-trust binary not built")
    m = _manifest(tmp_path)
    m.write_text(
        m.read_text().replace(
            '- {metric: recall, op: ">=", value: 0.75, output: recall, prose: x}',
            "- {metric: null, op: null, value: null, output: recall, prose: x}",
        )
    )
    _artifact(tmp_path, {"values": {"err": 0.004, "recall": 0.9}})
    result = run_replay(manifest_path=m, no_execute=True, render="json")
    rendered = json.loads(result.rendered)
    assert [s for s in rendered["skipped"] if s.get("fatal")] == []
    assert read(tmp_path / "last_verified.json")["multi"].values == {"err": 0.004}


def test_huge_integer_is_discarded_not_fatal(tmp_path: Path) -> None:
    (tmp_path / "out.json").write_text('{"values": {"err": 1' + "0" * 400 + ', "fp": 0}}')
    obs = extract_observations(CLAIM, tmp_path)
    assert obs.values == {"fp": 0.0}


def _scorer_with_helper(root: Path, helper_value: float, dirname: str = "tools") -> Path:
    tools = root / "evident" / dirname
    tools.mkdir(parents=True, exist_ok=True)
    (tools / "_evident_test_helper.py").write_text(f"VALUE = {helper_value}\n")
    (tools / "claim_scoring.py").write_text(
        dedent(
            """
            from types import SimpleNamespace
            import _evident_test_helper

            def score_claim(claim, repo_root):
                return SimpleNamespace(tolerances=[
                    SimpleNamespace(observed=_evident_test_helper.VALUE, output="err")
                ])
            """
        )
    )
    return tools


def test_each_project_gets_its_own_helpers(tmp_path: Path) -> None:
    a, b = tmp_path / "a", tmp_path / "b"
    _scorer_with_helper(a, 0.001)
    _scorer_with_helper(b, 0.002)
    assert extract_observations(CLAIM, a).values == {"err": 0.001}
    assert extract_observations(CLAIM, b).values == {"err": 0.002}
    assert "_evident_test_helper" not in sys.modules


def test_helpers_behind_a_symlinked_tools_dir_are_forgotten(tmp_path: Path) -> None:
    real = tmp_path / "real"
    _scorer_with_helper(real, 0.003)
    linked = tmp_path / "linked"
    (linked / "evident").mkdir(parents=True)
    (linked / "evident" / "tools").symlink_to(real / "evident" / "tools")
    assert extract_observations(CLAIM, linked).values == {"err": 0.003}
    assert "_evident_test_helper" not in sys.modules


def test_cleanup_keeps_modules_from_a_sibling_directory(tmp_path: Path, monkeypatch) -> None:
    """`tools_extra/` shares a prefix with `tools/` but is not inside it."""
    _scorer_with_helper(tmp_path, 0.001)
    extra = tmp_path / "evident" / "tools_extra"
    extra.mkdir()
    (extra / "_evident_sibling.py").write_text("X = 1\n")
    scorer = tmp_path / "evident" / "tools" / "claim_scoring.py"
    scorer.write_text(
        "import sys\nsys.path.append(%r)\nimport _evident_sibling\n" % str(extra)
        + scorer.read_text()
    )
    monkeypatch.setattr(sys, "path", list(sys.path))
    try:
        extract_observations(CLAIM, tmp_path)
        assert "_evident_sibling" in sys.modules
    finally:
        sys.modules.pop("_evident_sibling", None)


def test_validator_never_accepts_a_value_for_a_null_triple(tmp_path: Path) -> None:
    """The validator rejects a null tolerance field outright (it is not in
    the metric vocabulary), so a value for its output never validates."""
    manifest = VALID_MANIFEST.replace(
        '- {metric: count, op: "==", value: 0, output: fp, prose: x}',
        "- {metric: null, op: null, value: null, output: fp, prose: x}",
    )
    r = _validate(tmp_path, manifest)
    assert r.returncode != 0, r.stdout + r.stderr
