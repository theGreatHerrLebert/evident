# cu-ims-primitives: Mid-Flight Validation Lab

Source repo: [`cases/cu-ims-primitives`](cu-ims-primitives/)

## Problem

`cu-ims-primitives` is an experimental GPU-native primitive stack for diaPASEF
proteomics data. It is not presented as a finished product. It is a research
system under active construction, where the core question is not "is this
ready to ship?" but "how do we know which parts are working while the system is
still changing?"

The trust problem is active validation under uncertainty: simulated data has
known ground truth, real data has no perfect oracle, and GPU acceleration adds
another risk layer through CPU/CUDA divergence.

## Trust Strategy

The project combines several weaker sources of evidence into a practical
validation stack.

- Simulated ground truth: TimSim-style synthetic data provides exact peptide,
  precursor, and fragment blueprints.
- Real-data proxy oracles: mature tools such as DiaTracer, DiaNN, FragPipe,
  Sage, and timsseek are used as reference points, not absolute truth.
- Cross-path parity: CPU implementations and CUDA implementations are compared
  by primitive, with tiered tolerances.
- Behavioral validation: some primitives are judged by downstream
  identification impact rather than scalar equality.

## Evidence

The strongest evidence is the layered evaluation structure.

- `bench/README.md` separates simulated and real tracks, with different ground
  truth assumptions.
- `docs/primitives/TEST_MATRIX.md` defines a four-tier parity taxonomy:
  bit-identical, ulp-bounded, algorithmic substitution, and behavioral parity.
- `docs/ORACLE_COMPARISON.md` compares the pipeline against timsseek on
  simulated data and analyzes disagreements.
- `docs/analysis/DIA_TRACER.md` documents recovered behavior from a mature
  reference tool and uses it to explain likely pipeline gaps.
- `scripts/` and `py/eval/` contain many sweep and diagnostic tools for
  measuring changes during exploration.

## Assumptions

- Simulated truth is exact for what the simulator models, but may not represent
  real instrument behavior.
- Real-world tools are reference proxies, not ground truth.
- CPU reference paths are trusted more than CUDA paths unless parity is shown.
- Some components cannot be judged locally and must be evaluated through
  downstream identification behavior.

## Failure Modes

- Overfitting to simulator artifacts can improve synthetic metrics while
  hurting real data.
- Real-data comparisons can become ambiguous because DiaNN, DiaTracer,
  FragPipe, and timsseek make different modeling and scoring choices.
- Benchmark volume can create false confidence if each run does not declare
  the trust question it answers.
- Behavioral parity can hide local errors if downstream metrics are too coarse.
- Experimental scripts can become the only source of reproducibility unless
  promoted into documented, versioned workflows.

## What Worked

The repo is unusually explicit about being behind mature tools while still
measuring progress. That matters. It avoids the common failure mode where an
experimental GPU system advertises speed before it has earned correctness.

The test matrix is especially useful for `evident`: not every component needs
the same validation standard. Some outputs must be bit-identical, some need
floating-point tolerances, some are algorithmic substitutions, and some can
only be judged by downstream behavior.

## What Is Still Lacking

The repo is intentionally mid-flight and looks like a lab notebook plus code
base. From an `evident` perspective, the main gaps are packaging and governance
of evidence: no quick pass found a top-level license, no top-level CI workflow,
and the evaluation material is spread across docs, scripts, Python tooling,
and benchmark notes.

That does not make the project weak. It makes it a different kind of case: a
live example of how trust is built before a stable release boundary exists.

## Evident Lessons

- Simulated data and real-data proxy tools answer different trust questions.
- CPU/GPU parity should be tiered by the kind of output being compared.
- Mid-flight research systems need explicit decision rules, otherwise
  validation becomes a pile of interesting measurements.
- Honest negative framing is valuable evidence: knowing where the system is
  behind mature tools is part of the trust story.
