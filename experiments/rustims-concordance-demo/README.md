# Rustims observation pipeline demo

End-to-end exercise of the EVIDENT pipeline using the
`kind: third_party_observation` claim type (shipped in PR5i +
PR5j). **This is NOT a curated demo.** No author or curator
authored any of these claims. See "What this demo actually is"
below.

## Honest description of what was done

1. **LLM extraction (already on record).** The Phase 5
   extractor was run on the rustims preprint in PR #33; the
   model proposed 7 claims, recorded verbatim in
   `experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json`.
   The validator accepted 1 of 7.

2. **Automated reformatting (this demo).** The 7 raw candidates
   were split into 9 per-cell entries plus 1 ordinal_match
   entry. Each was reformatted into the
   `kind: third_party_observation` schema. Fields filled in
   from the LLM's source-span quotes:
   - `third_party_tool`: copied from the candidate's subject_aliases
   - `metric_definition`: paraphrased from the candidate's prose
   - `paper_locator`: copied from the candidate's source span
   - `observed_value`: copied from the candidate's reported value
   - `epsilon`: **picked arbitrarily** at 5% / 0.5 / 0.3 / 2.0
     percentage points, with no domain expertise behind the
     choice
   - `_unit`: hand-typed as "percentage_points" without
     consulting the paper's actual unit declarations

3. **Mock docker artifact.** `artifacts/observations.json` was
   hand-crafted to deliberately exercise all three comparator
   verdicts: 7 cells within ε (pass), 2 cells drifted on purpose
   (fail), 1 cell intentionally missing (not_assessed). The
   values are not from any real rustims docker invocation.

4. **Pipeline run.** `evident_agent.concordance.evaluate()` ran
   against the mock artifact for each claim; the verdicts were
   written to `last_concorded.json`; `typed-trust --format md
   --last-concorded-sidecar` rendered the report.

## What this demo actually proves

- **The schema parses.** All 10 reformatted claims translate
  cleanly through typed-trust, with 0 skipped.
- **The comparator dispatches correctly.** All 10 claims yield
  a verdict (no `ConcordanceError` raised).
- **Three verdict paths work end to end.** Pass, fail, and
  not_assessed all surface through to the rendered markdown
  with the right framing.
- **The PR5i `prior_value` non-leakage invariant holds on real
  data.** No "prior_value" string appears in the rendered output.
- **Per-cell numeric_band and ordinal_match coexist** in the
  same manifest without translator conflict.
- **The CLI render path works for observation claims.** The
  PR5j fix is exercised — `## Observation result` sections
  appear in the markdown.

## What this demo does NOT prove

- **NOT** "the framework expresses the paper's load-bearing
  claims." Nobody asked the paper's author which observations
  are load-bearing. The 7 candidates are the model's guesses,
  not the author's assertions.
- **NOT** "per-cell granularity is the right default for the
  paper." The mechanical splitting is arbitrary; the paper's
  author may want different units of claim.
- **NOT** "epsilon choices are domain-appropriate." Each
  epsilon was picked by me (an LLM) without domain expertise
  in proteomics or knowledge of the actual measurement
  uncertainty.
- **NOT** "curation is sustainable." No curation happened. The
  ~2-hour figure originally in this README was fiction.
- **NOT** "the docker contract is ergonomic." The artifact is
  hand-crafted; building a real rustims docker is non-trivial
  scaffolding work this demo skipped entirely.

## What a real curated demo would look like

The paper's author would:

1. Read their own paper
2. Decide which observations are actually load-bearing for the
   paper's argument
3. Pick the right pattern primitive per claim (and may decide
   that some don't fit any of the 5 primitives)
4. Pick epsilon values reflecting the actual measurement
   uncertainty in the paper's setup
5. Author the metric_definition prose from their own knowledge
   of the toolchain conventions, not paraphrasing the abstract
6. Decide on the unit declarations
7. Build the docker image that actually re-runs the
   experiments and emits the artifact
8. Reach a verdict per claim that has meaningful adjudicative
   weight

None of those steps happened here. The result of this demo is
a pipeline smoke test, not a curated EVIDENT manifest for the
rustims paper.

## Results from the pipeline smoke test

| Verdict | Count | Why |
|---|---:|---|
| **Pass ✓** | 7 | Mock artifact deliberately within ε for these cells |
| **Fail ✗** | 2 | Mock artifact deliberately drifted (FragPipe 100k FDR, PEAKS-XPro 100k identification) |
| **Not assessed** | 1 | Mock artifact deliberately missing `maxquant.peak_matching_error.fraction_pct_7p5min_150k` |

Files:

- `evident.yaml` — 10 reformatted observation claims
- `artifacts/observations.json` — hand-crafted mock artifact
- `last_concorded.json` — comparator verdicts
- `rendered/report.{md,html}` — rendered output

## Comparison with PR #33's extraction-rate result

| Metric | PR #33 (LLM extraction) | This demo (LLM extraction + automated reformat into observation schema) |
|---|---:|---:|
| Raw candidates from rustims main paper | 7 proposed by model | same 7 |
| Validator-accepted under PR #33's schema | 1 (measurement) | n/a |
| Syntactically valid under `third_party_observation` after mechanical splitting | n/a | 10 |

The right reading: **`third_party_observation` makes the
benchmark-paper claim shape schema-expressible.** It does not
mean the rustims paper now has 10 curated EVIDENT claims.
That would require the author's work, which has not happened.

## How to re-run

```bash
# Generate sidecar from the manifest + mock artifact
cd evident-agent
python3 - <<'PY'
import json, yaml
from pathlib import Path
from evident_agent import concordance, last_concorded
demo = Path("../experiments/rustims-concordance-demo")
manifest = yaml.safe_load((demo / "evident.yaml").read_text())
artifact = json.loads((demo / "artifacts/observations.json").read_text())
entries = {}
for claim in manifest["claims"]:
    obs = claim["observation"]
    block = {"pattern": {**obs["pattern"]}, "paper_locator": obs["paper_locator"],
             "prior_binding": {"prior_unit": "percentage_points"}}
    if "observed_value" in block["pattern"]:
        block["pattern"]["prior_value"] = block["pattern"].pop("observed_value")
    result = concordance.evaluate(artifact, block)
    entries[claim["id"]] = last_concorded.LastConcordedEntry(
        comparison_status=result.comparison_status,
        observed_value=result.observed_value,
        observed_unit=result.observed_unit,
        observed_ordering=result.observed_ordering,
        prior_ordering=result.prior_ordering,
        image_digest=result.image_digest,
        produced_at=result.produced_at,
        diagnostics=result.diagnostics,
    )
last_concorded.write(demo / "last_concorded.json", entries)
PY

# Render
cd ../typed-trust && cargo build
./target/debug/typed-trust --format md \
    --last-concorded-sidecar ../experiments/rustims-concordance-demo/last_concorded.json \
    ../experiments/rustims-concordance-demo/evident.yaml \
    > ../experiments/rustims-concordance-demo/rendered/report.md
```
