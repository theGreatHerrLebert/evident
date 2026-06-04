# Rustims hand-authored observation demo

End-to-end test of the EVIDENT framework on the user's own
preprint (rustims), with claims hand-authored using the
`kind: third_party_observation` claim type shipped in PR5i.

## Why this exists

The Phase 5 extraction-rate experiment (PR #33) showed that the
LLM extractor accepted **0–1 of 7** model-proposed claims from
the rustims paper. The six rejected candidates were all
benchmarked third-party tools — a shape EVIDENT didn't have a
claim kind for at the time.

PR5i added `third_party_observation` precisely for that shape.
This demo verifies that **the framework can now express most of
the rustims paper's load-bearing claims**, end to end:

```
hand-authored evident.yaml
  + mock docker artifact (observations.json)
  + evident_agent.concordance.evaluate() per claim
  → last_concorded.json
  → typed-trust --last-concorded-sidecar
  → rendered/report.{md,html}
```

## What was hand-authored

The 7 raw model-proposed candidates from PR #33's
`raw_extraction.json` were split into **10 per-cell
observation claims**, all `kind: third_party_observation`:

| Claim | Pattern | Reference value |
|---|---|---|
| MaxQuant peak matching error 7.5min, 150k | `numeric_band` | 30 ± 5 pp |
| PEAKS-XPro real FDR HLA-I 10k | `numeric_band` | 1.8 ± 0.5 pp |
| PEAKS-XPro real FDR HLA-I 100k | `numeric_band` | 1.15 ± 0.5 pp |
| FragPipe real FDR HLA-I 10k | `numeric_band` | 0.91 ± 0.3 pp |
| FragPipe real FDR HLA-I 100k | `numeric_band` | 1.16 ± 0.3 pp |
| FragPipe identification rate HLA-I 10k | `numeric_band` | 36.8 ± 2 pp |
| PEAKS-XPro identification rate HLA-I 10k | `numeric_band` | 32.9 ± 2 pp |
| FragPipe identification rate HLA-I 100k | `numeric_band` | 30.5 ± 2 pp |
| PEAKS-XPro identification rate HLA-I 100k | `numeric_band` | 29.5 ± 2 pp |
| Tool FDR ordering HLA-I 10k (FragPipe < PEAKS-XPro) | `ordinal_match` | lower-is-better, adjacent_swap_ok |

Each carries `third_party_tool`, `metric_definition`, and
`paper_locator` (page+line citation to the original preprint).

## What the mock artifact demonstrates

The `artifacts/observations.json` file is a hand-crafted JSON
emulating what `docker run rustims-experiments evident-replay`
would produce. Values were chosen to exercise all three
comparator verdicts:

- **7 cells** within ε of the paper's reported value → `pass`
- **2 cells** drifted outside ε on purpose → `fail`
- **1 cell** (`maxquant.peak_matching_error.fraction_pct_7p5min_150k`)
  intentionally absent → `not_assessed`

## Results

After running the comparator + typed-trust render:

| Verdict | Count | Claims |
|---|---:|---|
| **Pass ✓** | 7 | FDR ×4, identification rate ×2, ordering ×1 |
| **Fail ✗** | 2 | FragPipe 100k FDR (drift), PEAKS-XPro 100k identification (drift) |
| **Not assessed** | 1 | MaxQuant peak matching error (metric_path absent in artifact) |

See `rendered/report.md` for the full markdown report and
`rendered/report.html` for the HTML version.

## Comparing to PR #33's extraction-rate result

| Metric | PR #33 (LLM extraction) | This demo (hand-authored) |
|---|---:|---:|
| Representable claims | 1 of 7 | **10** |
| Comparator-decided verdicts | 0 | **10** (7 pass + 2 fail + 1 not_assessed) |
| Pattern primitives exercised | n/a | `numeric_band`, `ordinal_match` |
| Author effort | minutes (LLM call) | ~2 hrs hand-authoring + curating ε |

The 10/7 expansion ratio matches the codex-tightened success
criteria in `EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md` v3:
"expected ~10–12 per-cell observations after curator
splitting." Two more candidates (DIA-NN versions, FragPipe
versions on FDR) could have been authored from the same source
spans if exhaustive coverage was the goal.

## What this demo proves

1. **The framework can express the paper's load-bearing
   benchmark claims.** Not just compile; actually represent
   what the paper says.
2. **The comparator + sidecar + render pipeline works end to
   end on real-shape data.** Three verdict outcomes
   (pass/fail/not_assessed) all surface correctly through to
   the rendered output.
3. **Per-cell granularity is the right default.** The
   ordinal_match claim represents the cross-tool comparison the
   paper makes; the per-cell claims represent the absolute
   values. Both kinds coexist cleanly in the same manifest.
4. **`prior_value` never leaks externally.** The rendered
   report calls every reference value "Observed value" (codex
   v3 F-CR1 invariant holds end to end).

## What this demo does NOT prove

- That curator effort is sustainable across many papers. ~2 hrs
  for 10 claims on the author's own paper is not the same as
  authoring claims for a paper you didn't write.
- That the docker contract is ergonomic. The artifact JSON is
  hand-crafted here; a real rustims docker image would need to
  emit the same shape, which is non-trivial scaffolding work
  per repo.
- That the framework helps for non-benchmark papers (methods
  papers, theoretical papers, position papers). The rustims
  benchmark shape is exactly what `third_party_observation` was
  designed for; other paper shapes likely need other claim
  kinds.

## How to re-run

```bash
# 1. Run the comparator
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
    block = {
        "pattern": {**obs["pattern"]},
        "paper_locator": obs["paper_locator"],
        "prior_binding": {"prior_unit": "percentage_points"},
    }
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

# 2. Render
cd ../typed-trust && cargo build
./target/debug/typed-trust --format md \
    --last-concorded-sidecar ../experiments/rustims-concordance-demo/last_concorded.json \
    ../experiments/rustims-concordance-demo/evident.yaml \
    > ../experiments/rustims-concordance-demo/rendered/report.md
```

## Files

```
rustims-concordance-demo/
├── README.md                       # this file
├── evident.yaml                    # 10 hand-authored observation claims
├── artifacts/
│   └── observations.json           # mock docker output
├── last_concorded.json             # comparator verdicts (regenerated)
└── rendered/
    ├── report.md                   # typed-trust markdown render
    └── report.html                 # typed-trust HTML render
```
