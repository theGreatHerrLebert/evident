# Phase 5 extraction — rustims subject (2026-06-03)

First subject for the Phase 5 extraction-rate experiment: the
**rustims** project by the same author.

| Artifact | Type | Output |
|---|---|---|
| `repo-rustims` | Cargo workspace + Python packages | 0 claims at root; 3 + 3 from two subpackages |
| `paper-rustims-main` | preprint PDF (20 pages, 1.6 MB) | 0 accepted, 13 rejections |
| `paper-rustims-supplement` | supplement PDF (823 KB) | 0 accepted, 6 rejections |

## Headline finding

**The extractor produced ZERO empirical claims from either PDF on
the first run.** The default-deny framing + load-bearing source-span
validator rejected every candidate.

Calling this a "failure" would be wrong — it's exactly the
discipline EVIDENT was designed to enforce. The question is whether
the rejections are *load-bearing* (the validator is correctly
guarding against claim invention) or *over-strict* (the validator
is blocking claims a curator would have accepted).

## Rejection breakdown

### Main paper (13 rejections across 4 categories)

| Category | Count | Representative pattern |
|---|---|---|
| `metric_not_named` | 6 | Model invented composite metric names (e.g. "true FDR (HLA-I 10k thunder-dda-PASEF)") that weren't literally present in the source span. The metric token check fired. |
| `bound_not_stated` | 3 | "below 1% at higher complexities" — conditional/partial bounds without a clean numeric inequality. |
| `comparator_bound_to_wrong_subject` | 2 | "we observed 3-4% for Spectronaut, up to 5% for DIA-NN v1.8" — FDR observations about third-party tools on simulated data, not bounds on the paper's own system (timsim/rustims). |
| `hedged_qualitative_only` | 1 | "achieved a true FDR close to the nominal 1% threshold" — qualitative descriptor, not a comparator + bound. |
| `ranking_language_only` | 1 | "consistently detected precursors at lower simulated intensities than other tools" — ranking statement, no numeric bound. |

### Supplement (6 rejections across 3 categories)

| Category | Count | Representative pattern |
|---|---|---|
| `comparator_bound_to_wrong_subject` | 4 | "FDR ≤ 0.01 for peptides" inside PEAKS/FragPipe analysis configuration. Bounds describe filter thresholds applied within external tools, not the paper's subject artifact. |
| `bound_not_stated` | 1 | "minimum precursor intensity of 1,000 and a dynamic exclusion window of 25 frames" — simulation configuration values. |
| `hedged_qualitative_only` | 1 | "approximately 4 s for 30 min gradients" — qualitative, with explicit "approximately" hedge. |

## What this means for the experiment

The **3 most common rejection patterns are correct on their face**:

1. **Benchmark-subject conflation** (`comparator_bound_to_wrong_subject`,
   6 cases total across both PDFs). The validator caught the most
   common failure mode for benchmark/simulation papers: the paper's
   own system (rustims) is being used to *benchmark other tools*,
   and many of the natural numeric statements are about those tools'
   behavior, not about rustims's own performance bounds. A curator
   who promoted these would be authoring claims that don't belong
   to the paper's subject.

2. **Metric invention** (`metric_not_named`, 6 cases). The model
   invented composite metric handles like
   `peak_matching_error_rate_7p5min_150kpep_dda_pasef` that aren't
   literally in the source text. The validator's "metric token
   present in source_span" rule blocked these. This is the
   silent-threshold-invention failure mode the validator was
   designed to prevent — working as intended.

3. **Hedged + non-comparator language** (`hedged_qualitative_only`,
   `ranking_language_only`, `bound_not_stated`, 7 cases total).
   The paper's own text uses qualitative language ("close to", "up
   to", "approximately") and ranking language ("more than", "first
   to") far more than strict numeric inequalities. EVIDENT requires
   the latter; the paper mostly offers the former.

## Bigger-picture finding

For a **benchmark/simulation paper** like rustims, the load-bearing
claims aren't of the form "system X measures metric Y within bound
Z." They're of the form "system X reproduces benchmark behavior
patterns Y₁, Y₂, … qualitatively consistent with prior literature."

EVIDENT's measurement-claim type is a poor fit for those.

That doesn't mean the framework is broken for this paper — it
means a curator using EVIDENT on benchmark papers would either:

- **A.** Accept that 0 measurement claims survive and use the
  framework only for the few strict-inequality claims that DO
  exist (none in rustims's first preprint, evidently);
- **B.** Promote the kind of claims rustims actually makes into a
  *different* claim kind — something like
  `kind: behavioral_concordance` — that doesn't require an
  inequality + bound. (Out of scope for this experiment.)

## Repo metadata side

The metadata walker hit a Cargo-workspace blind spot at the root.
Sub-package extraction picked up 6 declarations across two
packages. See `extracted/repo-rustims/RUN_NOTES.md`.

## Next steps for the experiment

This is **one subject**. The plan called for 8–12 artifacts to
get stop/go signal. Two viable directions:

1. **Add more subjects** with different paper styles (a methods
   paper, a benchmark-light methods paper, a strict-thresholds
   paper) and see whether the rustims pattern reproduces or
   whether the framework works elsewhere.
2. **Treat rustims as the first negative data point** and design
   the next subjects to probe specific hypotheses (e.g., "do
   methods-claims papers fare better than benchmark papers?").

Both rectangle the experiment, but neither finishes it on its own.

## Discipline note

The user (= paper author) was instructed to use the **loose**
ground-truth-labeling mode: they know which claims their paper
makes, so post-hoc labeling against the extractor output is not
biased. Ground-truth YAML can now be written by listing what the
author considers load-bearing inequalities in the paper.
