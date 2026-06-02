# Phase 5 extraction-rate experiment

Codex-recommended measurement experiment to decide whether Phase 5 is
useful in the wild.

## Question

After Phase 5's input-side extractor ships (PRs #18–24), does it
actually produce a useful number of curator-approved structured
claims from real artifacts? Or does the validator's local-binding
discipline reject so much that the framework is too strict for the
average paper/repo?

The answer determines whether Phase 5-iii (cross-paper / cross-repo
claim graphs) is worth building.

## Stop/go discriminator (from EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md v3)

Per the v3 plan codex review:

> Stop/go discriminator is no longer raw extraction rate. Use
> curator-approved structured claims per artifact plus curator
> minutes per accepted claim. v3 splits the cost threshold by
> source type because paper curation is inherently slower:
>
> - Repo artifacts: ≤ 5 minutes per accepted claim.
> - Paper artifacts: ≤ 10–15 minutes per accepted claim.
> - Mixed median: ≤ 10 minutes per accepted claim.

**Go on Phase 5-iii** if curator-approved ≥ 2 load-bearing claims
per artifact median AND curator cost meets the per-source-type
thresholds above.

**Stop and publish the negative finding** if either falls below
threshold. That's a real result.

## Discipline

The codex v2 review of the parent plan called out that the cheap
version ("hand-author one paper's ideal manifest") tests
representability, not extraction rate. The right experiment must:

1. **Pre-commit to the artifact list before any prompt tuning.**
2. **Hand-label ground truth BEFORE running the extractor.** This
   is what makes the recall measurement honest.
3. **Curator decisions are recorded with timestamps**, not
   reconstructed afterward.
4. **No mid-experiment prompt edits.** If the prompt looks wrong
   on artifact 3, finish all 8–12, then iterate.

## Pre-committed artifact list

Pre-committed before any extractor run. Mix of repos and papers,
mix of paper-claim styles (benchmark-heavy ML vs. methods vs.
systems vs. negative/ablative), at least two PDFs where tables
matter.

### Repos (3–4)

| ID | Artifact | Why this one |
|----|----------|--------------|
| `repo-pdbtbx` | `/scratch/TMAlign/pdbtbx/` | Rust library for PDB/mmCIF I/O. README has performance + feature claims. |
| `repo-proteon` | `/scratch/TMAlign/proteon/` | The user's own scientific software project. Tests + benchmarks + README. Highest-stakes for the user. |
| `repo-freesasa` | `/scratch/TMAlign/freesasa/` | SASA computation library. C + Python. README has numeric performance claims. |
| `repo-foldseek-src` | `/scratch/TMAlign/foldseek-src/` | Fast structure search. Benchmark-heavy README. |

### Papers (4–6)

| ID | Artifact | Why this one |
|----|----------|--------------|
| `paper-chemrxiv-15002830` | `/scratch/TMAlign/chemrxiv.15002830_v1.pdf` | PDF. Real preprint. Tests pdftotext + page-split. |

**Remaining 3–5 papers to add by the curator (you):** pick from
PDFs you already know well so the hand-labeling is grounded.
Pre-commit ids here BEFORE running:

- `paper-???` — TBD
- `paper-???` — TBD
- `paper-???` — TBD
- `paper-???` — TBD

Curator constraint: at least 2 of the picked papers must have
**load-bearing claims inside a table** so the validator's
"value_only_in_image_table" rejection actually fires.

## Per-artifact directory layout

```
experiments/phase5-extraction-rate/
├── README.md                      (this file — committed before runs)
├── artifacts/
│   └── <id>/                      (source-of-truth pointer + sha)
│       └── source.md or source.pdf or repo-snapshot/
├── ground-truth/
│   └── <id>.yaml                  (hand-labelled BEFORE extraction)
├── extracted/
│   └── <id>/                      (extractor output)
│       ├── evident.yaml
│       ├── source/cited.md
│       └── EXTRACTION.md
├── curation/
│   └── <id>.yaml                  (curator decisions + timestamps)
└── results.md                     (final aggregate, written last)
```

## ground-truth/<id>.yaml schema

```yaml
artifact_id: paper-chemrxiv-15002830
artifact_source: /scratch/TMAlign/chemrxiv.15002830_v1.pdf
artifact_sha: <sha256 of bytes>
curator: <name>
labelled_at: <iso timestamp>
labelled_before_extraction: true   # discipline marker
load_bearing_claims:
  - id: lbc-1
    prose_summary: "<one-sentence statement of what the artifact claims>"
    locator: "Table 3 row 'ours'"
    extraction_difficulty: easy | medium | hard
    notes: "why hard / hidden assumptions"
  - id: lbc-2
    ...
```

## curation/<id>.yaml schema

```yaml
artifact_id: paper-chemrxiv-15002830
curator: <name>
extraction_run_at: <iso timestamp>
curated_at: <iso timestamp>
curator_minutes_total: <int>
extracted_claims:
  - extracted_id: <id from manifest>
    matched_ground_truth_id: lbc-1 | null
    decision: accept | drop | rephrase
    minutes_spent: <int>
    rationale: "..."
rejections_reviewed:
  - reason: bound_not_stated | comparator_bound_to_wrong_subject | ...
    count: <int>
    notes: "..."
```

## Metrics tracked (results.md)

Per source-type (repo / paper) and overall:

- **Recall** vs. hand-labelled load-bearing claims: extracted ∩
  ground_truth / |ground_truth|.
- **Precision** of structured claims: curator_approved /
  extracted_total.
- **Curator-approved-per-artifact**: how many claims survive
  curation per artifact (median + range).
- **Curator cost per accepted claim**: median minutes.
- **Replay-path distribution**: fraction `available`,
  `unavailable_artifacts:<reason>`, `not_attempted`.

## Stop/go decision

After all 8–12 artifacts processed:

- Compare per-source-type curator-approved-per-artifact median
  against the ≥ 2 threshold.
- Compare per-source-type curator cost against the ≤ 5 (repo) /
  ≤ 10–15 (paper) / ≤ 10 (mixed) thresholds.
- Both must pass to proceed to Phase 5-iii.
- Otherwise stop and publish the negative finding — that's
  itself a real EVIDENT-style claim about the framework.

## Operational notes

- API costs estimate: ~$5–30 in Anthropic tokens for ~10 artifacts.
- Curator time estimate: ~1 person-week of focused reading
  (codex v3 plan estimate).
- This experiment generates a corpus of extracted + curated
  manifests that is itself a Phase 5 deliverable.

## What this experiment commits us to

NOT to a specific architecture. NOT to ship Phase 5-iii.

The output is a yes/no/maybe on the extractor's real-world utility,
backed by structured measurement. Either outcome (passes thresholds
/ doesn't) is informative.
