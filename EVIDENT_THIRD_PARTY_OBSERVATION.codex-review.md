Reading additional input from stdin...
OpenAI Codex v0.136.0
--------
workdir: /scratch/TMAlign/evident
model: gpt-5.5
provider: openai
approval: never
sandbox: workspace-write [workdir, /tmp, $TMPDIR]
reasoning effort: medium
reasoning summaries: none
session id: 019e919b-cd80-7b43-ad3f-5d2282b28b2a
--------
user
Review this DESIGN DRAFT for a sixth EVIDENT claim kind: third_party_observation.

Context: EVIDENT now has five claim kinds (measurement, comparison, metadata_compatibility, behavioral_concordance, plus a few other measurement-flavored variants). The rustims experiment showed that benchmark/simulation papers' load-bearing claims often take the shape 'we ran tool X on our data and observed Y' — no cited prior (so concordance doesn't fit), subject is a third-party tool (so measurement doesn't fit). This PR proposes a sixth kind for that shape.

The proposed design reuses the ConcordancePattern enum (5 primitives), the Python comparator, the last_concorded.json sidecar, and the Rust ConcordanceResult struct — verbatim. Only the Claim-side schema (ObservationDeclaration block) and translator + render + MCP query tool are new.

The draft answers 5 of its open questions internally. Please tell me:

1. Is the reuse decision right? Specifically — keeping prior_value as the field name even though it's no longer a 'prior' in literature-citation sense.
2. Should ObservationDeclaration and ConcordanceDeclaration share a common base (e.g., factor out the pattern + paper_locator into a shared type) given they overlap so heavily?
3. Is the 'distinct ClaimKind, not concordance-with-a-flag' argument strong enough?
4. Are the disjointness invariants complete? What invariant am I missing?
5. Anything in the existing typed-trust schema (measurement claims with subjects that happen to be third-party tools but the paper authors framed as 'their' system; metadata_compatibility 'package_x_version' claims about external packages) that this inadvertently overlaps with or contradicts.
6. The success-criteria claim that 'roughly 7/7 of rustims candidates would translate cleanly after this lands' — sanity-check that.

Concrete and specific. Flag potential bugs prominently. Cap at 800 words.

<stdin>
# Design — `kind: third_party_observation` (v1 DRAFT)

> Status: **idea-level**, design for the sixth claim kind. The
> rustims experiment (PR #33) surfaced this as the dominant shape
> for benchmark/simulation papers: "we ran tool X on our data and
> observed Y." Distinct from `measurement` (subject is third-party,
> not the paper's own system) and from `behavioral_concordance`
> (no cited prior — the paper IS the source of the number).
>
> Drives a codex review before any implementation plan is written.

## Why

The rustims experiment ran the Phase 5 extractor against the
user's own preprint. Of the 7 model-proposed measurement
candidates, EVIDENT accepted 0–1. The 6 rejected candidates were
all about third-party tools being benchmarked on rustims-
simulated data:

> "MaxQuant's peak matching error rate rose sharply, reaching up
> to 30% in the 7.5 min, 150,000-peptide setup."

The validator correctly rejected those as `comparator_bound_to_
wrong_subject`. EVIDENT's `measurement` requires the bound to
bind to the paper's *own* system. But these claims ARE
load-bearing for benchmark papers — they're the entire point of
the paper.

`behavioral_concordance` (the fifth kind, just shipped) doesn't
fit either: it requires a `prior_binding` — the curator
transcribes a value from a *prior* paper that this observation is
being compared against. For "we observed 30% with MaxQuant on
our simulated data" there is no prior paper to cite. The paper
itself IS the source of the number.

This document proposes a sixth claim kind for that shape.

## Proposed claim kind

```yaml
- id: rustims-maxquant-peak-matching-error-7p5min
  kind: third_party_observation
  tier: research
  title: |
    MaxQuant peak matching error reaches up to 30% on
    7.5min 150k-peptide rustims-simulated dda-PASEF data
  claim: |
    On rustims-simulated 7.5min, 150,000-peptide dda-PASEF data,
    MaxQuant's peak matching error rate reached up to 30%.
  observation:
    third_party_tool: MaxQuant
    metric_definition: |
      Peak matching error rate per Cox 2008 §Methods —
      fraction of MS1 alignment peaks MaxQuant failed to bind
      to a precursor, computed against the gold-standard
      annotation set rustims provides.
    pattern:
      pattern_kind: numeric_band
      metric_path: maxquant.peak_matching_error.fraction_pct
      epsilon: 5.0
      prior_value: 30.0
    paper_locator: source/cited.md#rustims-maxquant-peak-matching
  evidence:
    docker_image: ghcr.io/theGreatHerrLebert/rustims-experiments@sha256:abc...
    command: evident-replay maxquant-peak-matching-7p5min
    artifact: outputs/maxquant_peak_matching.json
    artifact_schema_version: "1"
    replay_status: available
  provenance:
    kind: extracted-from-paper
    source_id: preprint:rustims-v1-covered-6dadf069
```

Three things distinguish this from the other kinds:

1. **`observation.third_party_tool`** — REQUIRED. Names the
   third-party tool being benchmarked. The presence of this
   field is the schema-level signal that the subject is NOT the
   paper's own system. Translator rejects empty / missing value.
2. **`observation.metric_definition`** — REQUIRED prose
   defining the metric. Plays the same load-bearing role as
   `prior_binding.prior_metric_definition` did for concordance:
   "FDR" / "peak matching error" / "identification rate" mean
   different things in different toolchains; the curator pins
   down which.
3. **`observation.pattern`** — REUSES the `ConcordancePattern`
   enum from PR5f. Same five primitives (`numeric_band`,
   `relative_band`, `same_order_of_magnitude`, `ordinal_match`,
   `monotone_with`). The `prior_value` field carries the
   *paper's own observed value* (no external prior to bind to).

## Why a distinct kind, not concordance with a flag

Considered: collapse this into `behavioral_concordance` with a
`prior_binding.kind: paper_self` discriminator instead of a
separate ClaimKind.

Rejected because:

1. **Curators discriminate the two cases.** "Verified against
   external prior literature" and "we made an observation and
   you can replay it" are different review semantics. A
   reviewer who endorses a concordance claim has implicitly
   audited the prior paper; a reviewer who endorses a third-
   party observation has implicitly verified the replay.
2. **Promotion gate semantics differ.** For
   `behavioral_concordance`, promotion to `tier:ci` means
   "the docker reproduces the prior value within tolerance."
   For `third_party_observation`, promotion means "an
   *independent party* (not the paper authors) has replayed
   the docker and observed the same value." The latter is a
   stronger claim about reproducibility.
3. **Corpus-level queries.** "Show me all claims about third-
   party tools where I haven't yet audited the prior literature"
   only makes sense if the discriminator is at the ClaimKind
   level. A flag inside `prior_binding` is structurally hidden
   behind one layer of object inspection.

The reuse remains substantial — see "What's reused" below.

## What's reused from PR5f / PR5g / PR5h

The bulk of the comparator and sidecar machinery applies
unchanged:

| Layer | Reused? | Notes |
|---|---|---|
| `ConcordancePattern` enum (5 primitives) | **YES** | Used verbatim. `prior_value` is the paper's observed value. |
| Python `evident_agent.concordance.evaluate()` | **YES** | Same function, same dispatch. Dispatches off `pattern.pattern_kind`. |
| `last_concorded.json` sidecar shape | **YES** | Same `LastConcordedEntry` shape. The `comparison_status` enum (`pass / fail / not_assessed`) reads the same way. |
| Rust `ConcordanceResult` struct | **YES** | Unchanged. |
| typed-trust `--last-concorded-sidecar` flag | **YES** | Unchanged. |
| Kind-keyed sidecar dispatch | **EXTENDED** | Both `behavioral_concordance` AND `third_party_observation` claims read from `last_concorded.json`. Duplicate-claim-id-across-sidecars discipline still applies. |
| MCP `query_concordance` tool | **PARALLEL** | New `query_observation` mirroring it for the new kind. |

## What's new

| Layer | What |
|---|---|
| `Claim` schema | New `observation: Option<ObservationDeclaration>` field (mirrors `concordance`). New `ClaimKind::ThirdPartyObservation` variant. |
| `ObservationDeclaration` struct | `third_party_tool: String`, `metric_definition: String`, `pattern: ConcordancePattern` (reused), `paper_locator: String`. |
| Translator path | New invariants: kind=third_party_observation requires `observation` block; rejects `tolerances`, `evidence.oracle`, top-level `source`, `metadata` block, `concordance` block. Other kinds reject `observation` block. |
| Render | New `## Observation` section in markdown, `<dl class="observation-declaration">` in HTML. |
| MCP `query_observation` | Mirrors `query_concordance`. Filter by `third_party_tool` (exact) and `pattern_kind` (exact). |
| Agent CLI | No new path — comparator already dispatches off `pattern.pattern_kind`. The `observation` block is read alongside `concordance` and the same `evaluate()` runs against the docker artifact. |

## Translator invariants (codex-pattern from PR5f)

For `kind: third_party_observation`:

1. `observation` block REQUIRED — without it, `ConcordanceClaimMissingBlock`-style error.
2. `observation.third_party_tool` REQUIRED non-empty.
3. `observation.metric_definition` REQUIRED non-empty.
4. Reject `tolerances` (the pattern primitive IS the bound).
5. Reject `evidence.oracle` list (the pattern primitive IS the oracle).
6. Reject top-level `source` field (use `observation.paper_locator`).
7. Reject `metadata` block (path disjoint).
8. Reject `concordance` block (path disjoint).
9. Apply the same `ConcordancePattern`-level validation: `OrdinalMatch` keyset alignment, `SameOrderOfMagnitude` positive `prior_value`, `RelativeBand` ratio > 1.0.

For other kinds (`measurement`, `metadata_compatibility`,
`behavioral_concordance`): reject `observation` block. Keeps the
kinds disjoint at the schema level.

## Sidecar boundary update

Currently (after PR5h): measurement → `last_verified.json`,
concordance → `last_concorded.json`. With this PR:

- `measurement` claims → `last_verified.json`
- `behavioral_concordance` claims → `last_concorded.json`
- `third_party_observation` claims → `last_concorded.json`

Both `behavioral_concordance` and `third_party_observation` use
`last_concorded.json` because the sidecar entry shape is
identical — the comparator's verdict is the same regardless of
whether the prior was external or paper-self. The kind-keyed
dispatch logic in `main.rs` becomes:

- measurement → read from `last_verified` overlay
- concordance OR observation → read from `last_concorded` overlay

Duplicate-claim-id-across-sidecars detection still applies.

## What this commits us to

- A new typed `ClaimKind::ThirdPartyObservation` variant.
- A typed `ObservationDeclaration` struct on `Claim`.
- A new `observation:` block in the manifest YAML.
- A new translator path with disjointness invariants.
- A render section + MCP query tool.
- Sidecar dispatch extended to cover the new kind.

## What this does NOT commit us to

- Reworking `ConcordancePattern` or the comparator primitives —
  reused verbatim.
- Reworking the `last_concorded.json` sidecar shape or
  Rust/Python read/write layers — reused verbatim.
- Adding new comparator primitives.
- A new `--last-observed-sidecar` flag (we use
  `--last-concorded-sidecar` for both).
- A "verified by independent party" promotion event (the
  stronger ci-tier claim about independent reproduction is a
  follow-up, not v1).

## Open questions for codex

1. **Reuse vs. fork on the pattern.** v1 reuses
   `ConcordancePattern`. Should the `prior_value` field be
   renamed to something kind-neutral (`reference_value`?) given
   it's no longer always a "prior" in the literature-citation
   sense? Or is keeping the same name in a reused enum more
   important than the semantic mismatch?
2. **`metric_definition` overlap.** Both
   `behavioral_concordance.prior_binding.prior_metric_definition`
   and `third_party_observation.observation.metric_definition`
   carry the same kind of prose (what the metric IS). Worth
   factoring out into a shared block? Or accept the duplication
   for v1 to keep the schemas independent?
3. **Curator second-signature for `tier:ci` promotion.** The
   v4 concordance design called this out as a future
   enforcement. Observation has a stronger version of the same
   problem: promotion implies independent reproduction. Should
   v1 require ANY structured curator block on the promotion
   event, or document it as a future gate?
4. **Promotion event semantics.** When a third_party_observation
   claim is promoted from research → ci, what does the
   `PromoteFromExtracted` review event need to carry that's
   different from the concordance case? Possibly the
   `reproduced_by` curator identity?
5. **Negative observations** ("we did NOT observe Y with this
   tool"). Out of scope for v1; flag as follow-up.

## Estimated size

- ~200–250 LOC Rust for `ClaimKind` + `ObservationDeclaration`
  + translator + render + paper_locator threading
- ~80 LOC Rust for `query_observation` MCP tool
- ~0 LOC Python (comparator unchanged — dispatches off
  `pattern.pattern_kind` and reads any `observation` or
  `concordance` block the agent passes it)
- 20–30 tests

Smaller than PR5f because the comparator + sidecar are reused.
Roughly half the LOC.

## What success looks like for v1

The remaining 6 model-proposed candidates from rustims PR #33 —
the ones that didn't fit measurement or concordance — translate
cleanly as `third_party_observation` claims. The curator can
hand-author a docker image + a manifest, run the comparator,
and the rendered report shows `## Observation result: Pass ✓`
(or Fail) for each.

After this lands, the rustims paper's full extractable claim
count goes from 1/7 (measurement) + 0/7 (concordance, no priors
cited) to roughly 7/7 (one or two concordance, the rest
third-party observations). That's the demo-rate the experiment
was originally targeting.
</stdin>
codex
I’ll check the current schema and translator shape before judging overlap/invariants, so the review is grounded in the code rather than only the draft.
exec
/bin/bash -lc 'rg --files .' in /scratch/TMAlign/evident
 succeeded in 0ms:
./EVIDENT_AGENT_PHASE2C.code-review.md
./EVIDENT_AGENT_PHASE2D.codex-review-2.md
./EVIDENT_AGENT_PHASE2.code-review.md
./README.md
./EVIDENT_PHASE5_PR2.codex-review.md
./typed-trust-codex-review-2.md
./typed-trust-codex-review-10.md
./EVIDENT_CURATOR.code-review.md
./typed-trust-codex-review-4.md
./EVIDENT_PHASE5_PR5.code-review.md
./EVIDENT_DESIGN.md
./EVIDENT_PHASE5_PR6.codex-review.md
./EVIDENT_AGENT_PHASE2B.codex-review.md
./EVIDENT_AGENT_PHASE2C_DRAFT.md
./EVIDENT_RECORD_FIXTURES.code-review.md
./EVIDENT_PHASE5_PR5_DRAFT.md
./typed-trust-codex-review-6.md
./typed-trust-codex-review-7.md
./EVIDENT_PHASE5.codex-review.md
./EVIDENT_DESIGN_v0.5_DRAFT.md
./EVIDENT_PHASE5_PR6_DRAFT.md
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md
./EVIDENT_AGENT_PHASE3.code-review.md
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md
./EVIDENT_PR5B.code-review.md
./experiments/phase5-extraction-rate/curation/_template.yaml
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md
./experiments/phase5-extraction-rate/artifacts/repo-rustims/source.yaml
./experiments/phase5-extraction-rate/artifacts/paper-rustims-main/source.yaml
./patterns/README.md
./experiments/phase5-extraction-rate/artifacts/paper-rustims-supplement/source.yaml
./EVIDENT_AGENT_PHASE2D.codex-review.md
./EVIDENT_AGENT_PHASE2C.codex-review.md
./EVIDENT_METADATA_RENDER_MCP_PLAN.md
./EVIDENT_AGENT_PHASE3.codex-review.md
./experiments/phase5-extraction-rate/README.md
./cases/proteon.md
./checklist/README.md
./EVIDENT_DESIGN_v0.3.codex-review.md
./experiments/phase5-extraction-rate/extracted/repo-rustims-rustms/EXTRACTION.md
./experiments/phase5-extraction-rate/extracted/repo-rustims-rustms/evident.yaml
./experiments/phase5-extraction-rate/extracted/repo-rustims/EXTRACTION.md
./experiments/phase5-extraction-rate/extracted/repo-rustims/RUN_NOTES.md
./experiments/phase5-extraction-rate/extracted/repo-rustims/evident.yaml
./cases/README.md
./cases/cu-ims-primitives.md
./EVIDENT_DESIGN_v0.6_DRAFT.md
./EVIDENT_AGENT_PHASE2_DRAFT.md
./EVIDENT_AGENT_PHASE2B_DRAFT.md
./typed-trust-codex-review-8.md
./experiments/phase5-extraction-rate/extracted/repo-rustims-imspy-core/EXTRACTION.md
./experiments/phase5-extraction-rate/extracted/repo-rustims-imspy-core/evident.yaml
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/EXTRACTION.md
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/evident.yaml
./rules/README.md
./EVIDENT_AGENT_PHASE2D.code-review.md
./typed-trust-codex-review-11.md
./EVIDENT_PHASE5_PR4.codex-review.md
./EVIDENT_METADATA_RENDER_MCP.codex-review.md
./evident.yaml
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/source/cited.md
./EVIDENT_PHASE5_PR5.codex-review-v2.md
./EVIDENT_PHASE5_PR5.codex-review.md
./typed-trust-codex-review-9.md
./typed-trust-codex-review-3.md
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/EXTRACTION.md
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/raw_extraction.json
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/evident.yaml
./EVIDENT_PHASE5_PR6.codex-review-v2.md
./EVIDENT_DESIGN_v0.5.codex-review.md
./EVIDENT_PHASE5_PR3.codex-review.md
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md
./EVIDENT_DESIGN_v0.4_DRAFT.md
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/source/cited.md
./experiments/phase5-extraction-rate/aggregate.py
./experiments/phase5-extraction-rate/ground-truth/_template.yaml
./EVIDENT_AGENT_PHASE3.code-review-2.md
./EVIDENT_REPHRASE.code-review.md
./anti-patterns/README.md
./EVIDENT_AGENT_PHASE2D_DRAFT.md
./EVIDENT_PHASE5_PR1.codex-review.md
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md
./EVIDENT_DESIGN_v0.6.codex-review.md
./EVIDENT_AGENT_PHASE2B.code-review.md
./EVIDENT_AGENT_PHASE3_DRAFT.md
./EVIDENT_AGENT_PHASE2.codex-review.md
./EVIDENT_DESIGN_v0.3_DRAFT.md
./workflow/evident.py
./workflow/SCHEMA.md
./workflow/validate_manifest.py
./workflow/GRAMMAR.md
./workflow/README.md
./workflow/Dockerfile
./EVIDENT_PHASE5_PR6.code-review.md
./EVIDENT_PHASE5.codex-review-v2.md
./typed-trust-codex-review.md
./evident-agent/pyproject.toml
./concepts/typed-trust-json-shape.md
./concepts/ai-assisted-coding.md
./concepts/README.md
./concepts/not-just-a-unit-test.md
./concepts/typed-trust-proteon-fit.md
./concepts/typed-trust-overview.md
./concepts/typed-trust.md
./evident-agent/tests/test_violation.py
./evident-agent/tests/test_rephrase.py
./evident-agent/tests/test_curator.py
./evident-agent/tests/test_scoring.py
./evident-agent/tests/test_sidecar.py
./evident-agent/tests/test_extract_validator.py
./latex/evident-paper/references.bib
./evident-agent/tests/test_cli_dry_run.py
./evident-agent/tests/test_extract_metadata.py
./evident-agent/tests/test_extract_paper_cli.py
./evident-agent/tests/test_extract_typed_trust_roundtrip.py
./viewer/tsconfig.json
./viewer/astro.config.mjs
./viewer/package.json
./evident-agent/tests/test_extract_render.py
./evident-agent/tests/test_review_sidecar.py
./evident-agent/tests/test_typed_trust.py
./evident-agent/tests/test_concordance.py
./latex/evident-paper/Makefile
./evident-agent/tests/test_extract_framing.py
./latex/evident-paper/README.md
./evident-agent/tests/test_evidence.py
./evident-agent/tests/test_docker.py
./evident-agent/tests/test_review.py
./evident-agent/evident_agent/review_walkthrough.py
./evident-agent/evident_agent/violation.py
./evident-agent/evident_agent/sidecar.py
./evident-agent/evident_agent/cli.py
./evident-agent/tests/test_prompt.py
./evident-agent/tests/test_extract_cli.py
./evident-agent/tests/test_review_walkthrough.py
./evident-agent/tests/test_cli_review.py
./evident-agent/tests/test_extract_repo.py
./evident-agent/tests/test_manifest.py
./evident-agent/tests/test_required_fixtures.py
./evident-agent/tests/test_extract_paper.py
./evident-agent/evident_agent/concordance.py
./evident-agent/examples/proteon_sasa_release.sh
./EVIDENT_WALKTHROUGH_LADDER.code-review.md
./EVIDENT_DESIGN_v0.4_FIT_TEST.md
./EVIDENT_MULTISTEP.code-review.md
./evident-agent/evident_agent/docker.py
./evident-agent/evident_agent/prompt.py
./evident-agent/evident_agent/__init__.py
./latex/evident-paper/scripts/make_claim_layers.py
./latex/evident-paper/main.tex
./viewer/src/lib/manifest.ts
./viewer/src/lib/match.ts
./viewer/src/env.d.ts
./LICENSE
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md
./typed-trust-codex-review-5.md
./evident-agent/tests/fixtures/adversarial/sasa_endorse/evident.yaml
./evident-agent/tests/fixtures/adversarial/sasa_endorse/results.json
./evident-agent/evident_agent/review_sidecar.py
./evident-agent/evident_agent/typed_trust.py
./evident-agent/evident_agent/curator.py
./evident-agent/evident_agent/last_concorded.py
./evident-agent/evident_agent/review.py
./evident-agent/evident_agent/manifest.py
./evident-agent/evident_agent/evidence.py
./EVIDENT_AGENT_PHASE3.codex-review-2.md
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md
./viewer/package-lock.json
./evident-agent/tests/fixtures/adversarial/ball_challenge/results.json
./evident-agent/tests/fixtures/adversarial/ball_challenge/evident.yaml
./viewer/src/styles/global.css
./evident-agent/evident_agent/extract/paper.py
./evident-agent/evident_agent/extract/framing.py
./evident-agent/evident_agent/scoring.py
./evident-agent/evident_agent/extract/redaction.py
./evident-agent/evident_agent/extract/metadata.py
./evident-agent/evident_agent/extract/cli.py
./evident-agent/evident_agent/extract/audit.py
./typed-trust/Cargo.toml
./evident-agent/evident_agent/extract/__init__.py
./evident-agent/evident_agent/extract/validator.py
./evident-agent/evident_agent/extract/repo.py
./evident-agent/evident_agent/extract/render.py
./typed-trust/tests/human_render.rs
./typed-trust/src/claim.rs
./viewer/src/pages/composer.astro
./typed-trust/src/main.rs
./typed-trust/src/human_render.rs
./viewer/src/pages/coverage.astro
./typed-trust/src/synthesize.rs
./typed-trust/src/report.rs
./typed-trust/src/loader.rs
./typed-trust/src/lib.rs
./typed-trust/src/evidence.rs
./typed-trust/src/render.rs
./typed-trust/src/translate.rs
./typed-trust/src/html_render.rs
./typed-trust/src/identity.rs
./typed-trust/src/review.rs
./typed-trust/src/ids.rs
./typed-trust/src/graph.rs
./typed-trust/src/derivation.rs
./viewer/src/pages/index.astro
./typed-trust/tests/review_events_sidecar.rs
./typed-trust/tests/translator.rs
./typed-trust/tests/panel_summary.rs
./typed-trust/tests/mcp_tools.rs
./typed-trust/tests/json_output.rs
./typed-trust/tests/mcp_loadbearing.rs
./typed-trust/tests/supersede_loadbearing.rs
./typed-trust/tests/synthesize_and_render.rs
./typed-trust/tests/proteon_walkthrough.rs
./typed-trust/tests/html_render.rs
./typed-trust/tests/promote_from_extracted.rs
./typed-trust/tests/fixtures/augmented_contested.trustreport.json
./typed-trust/tests/fixtures/proteon_charmm19_contested.trustreport.json
./typed-trust/tests/fixtures/augmented_sasa_release.trustreport.json
./typed-trust/tests/fixtures/synthetic_challenge.event.json
./typed-trust/tests/fixtures/contested.mermaid
./typed-trust/tests/fixtures/contested.md
./typed-trust/tests/fixtures/contested.html
./typed-trust/tests/fixtures/proteon_sasa_release.trustreport.json
./evident-agent/tests/fixtures/adversarial/ball_dissent/evident.yaml
./evident-agent/tests/fixtures/adversarial/ball_dissent/results.json
./typed-trust/src/mcp/handlers.rs
./typed-trust/src/provenance.rs
./typed-trust/src/mcp/mod.rs
./typed-trust/src/mcp/tools.rs
./viewer/src/layouts/Layout.astro
./evident-agent/tests/fixtures/extract/metadata/cargo_repo/Cargo.toml
./evident-agent/tests/fixtures/extract/metadata/uv_workspace_repo/pyproject.toml
./evident-agent/tests/fixtures/review/sasa_ci_endorse.json
./evident-agent/tests/fixtures/extract/metadata/pyproject_repo/pyproject.toml
./evident-agent/tests/fixtures/extract/metadata/multi_file_repo/pyproject.toml
./typed-trust/src/bin/mcp.rs
./evident-agent/tests/fixtures/extract/metadata/multi_file_repo/Cargo.toml
./evident-agent/tests/fixtures/extract/metadata/uv_workspace_repo/packages/core/pyproject.toml
./viewer/src/pages/claims/[id].astro
./evident-agent/tests/fixtures/extract/metadata/package_json_repo/package.json
./evident-agent/tests/fixtures/extract/metadata/cargo_workspace_repo/Cargo.toml
./evident-agent/tests/fixtures/extract/repo/clean_repo/README.md
./evident-agent/tests/fixtures/extract/repo/conflict_repo/README.md
./evident-agent/tests/fixtures/extract/repo/conflict_repo/CHANGELOG.md
./evident-agent/tests/fixtures/extract/metadata/uv_workspace_repo/packages/vis/pyproject.toml
./evident-agent/tests/fixtures/extract/repo/future_tense_repo/README.md
./evident-agent/tests/fixtures/review/ball-electrostatic-synthetic-challenge/claude-haiku-4-5-20251001.json
./evident-agent/tests/fixtures/review/ball-electrostatic-synthetic-challenge/claude-sonnet-4-6.json
./evident-agent/tests/fixtures/review/ball-electrostatic-synthetic-challenge/claude-opus-4-7.json
./evident-agent/tests/fixtures/extract/metadata/cargo_workspace_repo/mscore/Cargo.toml
./evident-agent/tests/fixtures/review/proteon-sasa-vs-biopython-release-1k-pdbs.json
./evident-agent/tests/fixtures/review/ball-electrostatic-synthetic-challenge.json
./evident-agent/tests/fixtures/review/ball-electrostatic-synthetic-dissent.json
./evident-agent/tests/fixtures/extract/metadata/cargo_workspace_repo/rustms/Cargo.toml
./evident-agent/tests/fixtures/extract/paper/plaintext_bibliography_paper.md
./evident-agent/tests/fixtures/extract/paper/clear_paper.md
./evident-agent/tests/fixtures/extract/paper/wrong_subject_binding.md
./evident-agent/tests/fixtures/review/ball_electrostatic_challenge.json
./evident-agent/tests/fixtures/extract/paper/false_positive_references_paper.md
./evident-agent/tests/fixtures/review/ball_electrostatic_dissent.json
./evident-agent/tests/fixtures/extract/repo/no_claim_repo/README.md
./evident-agent/tests/fixtures/extract/repo/cites_paper_repo/README.md
./evident-agent/tests/fixtures/extract/paper/hedged_paper.md
./evident-agent/tests/fixtures/extract/repo/marketing_repo/README.md
./evident-agent/tests/fixtures/extract/repo/cite_this_repo/README.md

exec
/bin/bash -lc 'rg -n "enum ClaimKind|struct .*Declaration|metadata_compatibility|behavioral_concordance|ConcordancePattern|last_concorded|last_verified|oracle|tolerances|package_x_version|subject" -S .' in /scratch/TMAlign/evident
 succeeded in 0ms:
./patterns/README.md:40:independent oracle, a documented tolerance, a reproducible command, and a
./patterns/README.md:49:- oracle identity and version
./patterns/README.md:66:- the final task has no perfect oracle on real data
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:16:Context: EVIDENT now has five claim kinds (measurement, comparison, metadata_compatibility, behavioral_concordance, plus a few other measurement-flavored variants). The rustims experiment showed that benchmark/simulation papers' load-bearing claims often take the shape 'we ran tool X on our data and observed Y' — no cited prior (so concordance doesn't fit), subject is a third-party tool (so measurement doesn't fit). This PR proposes a sixth kind for that shape.
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:18:The proposed design reuses the ConcordancePattern enum (5 primitives), the Python comparator, the last_concorded.json sidecar, and the Rust ConcordanceResult struct — verbatim. Only the Claim-side schema (ObservationDeclaration block) and translator + render + MCP query tool are new.
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:26:5. Anything in the existing typed-trust schema (measurement claims with subjects that happen to be third-party tools but the paper authors framed as 'their' system; metadata_compatibility 'package_x_version' claims about external packages) that this inadvertently overlaps with or contradicts.
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:37:> observed Y." Distinct from `measurement` (subject is third-party,
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:38:> not the paper's own system) and from `behavioral_concordance`
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:55:wrong_subject`. EVIDENT's `measurement` requires the bound to
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:60:`behavioral_concordance` (the fifth kind, just shipped) doesn't
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:109:   field is the schema-level signal that the subject is NOT the
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:117:3. **`observation.pattern`** — REUSES the `ConcordancePattern`
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:125:Considered: collapse this into `behavioral_concordance` with a
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:138:   `behavioral_concordance`, promotion to `tier:ci` means
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:159:| `ConcordancePattern` enum (5 primitives) | **YES** | Used verbatim. `prior_value` is the paper's observed value. |
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:161:| `last_concorded.json` sidecar shape | **YES** | Same `LastConcordedEntry` shape. The `comparison_status` enum (`pass / fail / not_assessed`) reads the same way. |
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:164:| Kind-keyed sidecar dispatch | **EXTENDED** | Both `behavioral_concordance` AND `third_party_observation` claims read from `last_concorded.json`. Duplicate-claim-id-across-sidecars discipline still applies. |
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:172:| `ObservationDeclaration` struct | `third_party_tool: String`, `metric_definition: String`, `pattern: ConcordancePattern` (reused), `paper_locator: String`. |
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:173:| Translator path | New invariants: kind=third_party_observation requires `observation` block; rejects `tolerances`, `evidence.oracle`, top-level `source`, `metadata` block, `concordance` block. Other kinds reject `observation` block. |
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:185:4. Reject `tolerances` (the pattern primitive IS the bound).
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:186:5. Reject `evidence.oracle` list (the pattern primitive IS the oracle).
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:190:9. Apply the same `ConcordancePattern`-level validation: `OrdinalMatch` keyset alignment, `SameOrderOfMagnitude` positive `prior_value`, `RelativeBand` ratio > 1.0.
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:192:For other kinds (`measurement`, `metadata_compatibility`,
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:193:`behavioral_concordance`): reject `observation` block. Keeps the
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:198:Currently (after PR5h): measurement → `last_verified.json`,
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:199:concordance → `last_concorded.json`. With this PR:
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:201:- `measurement` claims → `last_verified.json`
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:202:- `behavioral_concordance` claims → `last_concorded.json`
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:203:- `third_party_observation` claims → `last_concorded.json`
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:205:Both `behavioral_concordance` and `third_party_observation` use
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:206:`last_concorded.json` because the sidecar entry shape is
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:211:- measurement → read from `last_verified` overlay
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:212:- concordance OR observation → read from `last_concorded` overlay
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:227:- Reworking `ConcordancePattern` or the comparator primitives —
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:229:- Reworking the `last_concorded.json` sidecar shape or
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:241:   `ConcordancePattern`. Should the `prior_value` field be
./EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md:247:   `behavioral_concordance.prior_binding.prior_metric_definition`
./typed-trust/tests/human_render.rs:9:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust/tests/human_render.rs:27:    tolerances:
./typed-trust/tests/human_render.rs:35:      oracle:
./typed-trust/tests/human_render.rs:40:    last_verified:
./typed-trust/tests/human_render.rs:61:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/fixtures/proteon_charmm19_contested.trustreport.json:45:        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust/tests/synthesize_and_render.rs:13:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust/tests/synthesize_and_render.rs:19:/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust/tests/synthesize_and_render.rs:20:/// translation working) with last_verified populated.
./typed-trust/tests/synthesize_and_render.rs:34:    tolerances:
./typed-trust/tests/synthesize_and_render.rs:42:      oracle:
./typed-trust/tests/synthesize_and_render.rs:47:    last_verified:
./typed-trust/tests/synthesize_and_render.rs:56:/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust/tests/synthesize_and_render.rs:70:    tolerances:
./typed-trust/tests/synthesize_and_render.rs:77:      oracle:
./typed-trust/tests/synthesize_and_render.rs:82:    last_verified:
./typed-trust/tests/synthesize_and_render.rs:102:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/synthesize_and_render.rs:150:    // Strip last_verified so the rerun has no observations.
./typed-trust/tests/synthesize_and_render.rs:152:        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust/tests/synthesize_and_render.rs:153:        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust/tests/synthesize_and_render.rs:1661:// PR5f: behavioral_concordance render augmentation
./typed-trust/tests/synthesize_and_render.rs:1669:            ConcordanceDeclaration, ConcordancePattern, PriorBindingContext,
./typed-trust/tests/synthesize_and_render.rs:1684:        pattern: ConcordancePattern::NumericBand {
./typed-trust/tests/synthesize_and_render.rs:1729:            ConcordanceDeclaration, ConcordancePattern, PriorBindingContext,
./typed-trust/tests/synthesize_and_render.rs:1745:        pattern: ConcordancePattern::NumericBand {
./typed-trust/tests/synthesize_and_render.rs:1786:            ComparisonStatus, ConcordanceDeclaration, ConcordancePattern,
./typed-trust/tests/synthesize_and_render.rs:1802:        pattern: ConcordancePattern::NumericBand {
./typed-trust/tests/synthesize_and_render.rs:1855:            ComparisonStatus, ConcordanceDeclaration, ConcordancePattern,
./typed-trust/tests/synthesize_and_render.rs:1872:        pattern: ConcordancePattern::NumericBand {
./typed-trust/tests/synthesize_and_render.rs:1919:            ComparisonStatus, ConcordanceDeclaration, ConcordancePattern,
./typed-trust/tests/synthesize_and_render.rs:1942:        pattern: ConcordancePattern::OrdinalMatch {
./typed-trust/tests/proteon_walkthrough.rs:38:               within tier-specific tolerances."
./typed-trust/tests/proteon_walkthrough.rs:52:    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust/tests/proteon_walkthrough.rs:99:                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust/tests/proteon_walkthrough.rs:116:                against: None, // pass rate isn't oracle-specific
./typed-trust/tests/proteon_walkthrough.rs:127:    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust/tests/proteon_walkthrough.rs:213:    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust/tests/proteon_walkthrough.rs:219:    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust/tests/html_render.rs:7:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust/tests/html_render.rs:25:    tolerances:
./typed-trust/tests/html_render.rs:33:      oracle:
./typed-trust/tests/html_render.rs:38:    last_verified:
./typed-trust/tests/html_render.rs:59:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/promote_from_extracted.rs:41:    tolerances:
./typed-trust/tests/promote_from_extracted.rs:48:      oracle: [Paper-Authority]
./typed-trust/tests/promote_from_extracted.rs:72:    tolerances:
./typed-trust/tests/promote_from_extracted.rs:79:      oracle: [Paper-Authority]
./typed-trust/tests/promote_from_extracted.rs:207:    tolerances:
./typed-trust/tests/promote_from_extracted.rs:213:      oracle: [Biopython]
./typed-trust/tests/promote_from_extracted.rs:588:    tolerances:
./typed-trust/tests/promote_from_extracted.rs:595:      oracle: [Paper-Authority]
./typed-trust/tests/promote_from_extracted.rs:787:    tolerances:
./typed-trust/tests/promote_from_extracted.rs:793:      oracle: [Paper-Authority]
./typed-trust/tests/review_events_sidecar.rs:94:        tolerances: Some(vec![ManifestTolerance {
./typed-trust/tests/review_events_sidecar.rs:102:            oracle: vec!["BALL".into()],
./typed-trust/tests/review_events_sidecar.rs:109:        last_verified: None,
./typed-trust/tests/translator.rs:10:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust/tests/translator.rs:15:/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust/tests/translator.rs:16:/// `last_verified` block has all null values (the CI replay loop is
./typed-trust/tests/translator.rs:34:    tolerances:
./typed-trust/tests/translator.rs:42:      oracle:
./typed-trust/tests/translator.rs:47:    last_verified:
./typed-trust/tests/translator.rs:55:      - Single-oracle agreement can mask a shared convention choice.
./typed-trust/tests/translator.rs:59:/// claim with a populated last_verified block. Verbatim values for the
./typed-trust/tests/translator.rs:60:/// fields that matter; oracle list trimmed to one for the
./typed-trust/tests/translator.rs:61:/// single-oracle path (the real claim has two — Biopython AND
./typed-trust/tests/translator.rs:77:    tolerances:
./typed-trust/tests/translator.rs:85:      oracle:
./typed-trust/tests/translator.rs:90:    last_verified:
./typed-trust/tests/translator.rs:104:  - id: proteon-oracle-backed-release-gate
./typed-trust/tests/translator.rs:105:    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust/tests/translator.rs:115:      accepted for release only when they point to an independent oracle.
./typed-trust/tests/translator.rs:117:      oracle:
./typed-trust/tests/translator.rs:119:      command: pytest tests/oracle -v
./typed-trust/tests/translator.rs:143:    tolerances:
./typed-trust/tests/translator.rs:163:      oracle:
./typed-trust/tests/translator.rs:165:      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust/tests/translator.rs:190:    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust/tests/translator.rs:211:fn translates_single_oracle_tolerance_populates_against() {
./typed-trust/tests/translator.rs:213:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust/tests/translator.rs:221:    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust/tests/translator.rs:222:    // entry in `evidence.oracle`.
./typed-trust/tests/translator.rs:227:    // last_verified Reruns bind to this stable id.
./typed-trust/tests/translator.rs:242:            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust/tests/translator.rs:250:fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust/tests/translator.rs:254:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust/tests/translator.rs:276:    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust/tests/translator.rs:306:    tolerances:
./typed-trust/tests/translator.rs:312:      oracle: [SomeOracle]
./typed-trust/tests/translator.rs:317:    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust/tests/translator.rs:343:    tolerances:
./typed-trust/tests/translator.rs:349:      oracle: [internal]
./typed-trust/tests/translator.rs:350:      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust/tests/translator.rs:354:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust/tests/translator.rs:396:    tolerances:
./typed-trust/tests/translator.rs:400:      oracle: [Foo]
./typed-trust/tests/translator.rs:401:      command: pytest tests/oracle
./typed-trust/tests/translator.rs:405:    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust/tests/translator.rs:416:fn rejects_measurement_claim_without_tolerances() {
./typed-trust/tests/translator.rs:417:    // Codex round 5: kind: measurement requires non-empty tolerances per
./typed-trust/tests/translator.rs:422:  - id: measurement-no-tolerances
./typed-trust/tests/translator.rs:423:    title: missing tolerances
./typed-trust/tests/translator.rs:430:      oracle: [Foo]
./typed-trust/tests/translator.rs:435:    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust/tests/translator.rs:438:            assert_eq!(id, "measurement-no-tolerances");
./typed-trust/tests/translator.rs:459:    tolerances:
./typed-trust/tests/translator.rs:466:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust/tests/translator.rs:478:fn rejects_measurement_claim_with_empty_tolerances_list() {
./typed-trust/tests/translator.rs:481:  - id: measurement-empty-tolerances
./typed-trust/tests/translator.rs:482:    title: empty tolerances
./typed-trust/tests/translator.rs:488:    tolerances: []
./typed-trust/tests/translator.rs:490:      oracle: [Foo]
./typed-trust/tests/translator.rs:495:    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust/tests/translator.rs:513:    tolerances:
./typed-trust/tests/translator.rs:517:      oracle: [Foo]
./typed-trust/tests/translator.rs:518:      command: pytest tests/oracle
./typed-trust/tests/translator.rs:522:    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust/tests/translator.rs:543:    tolerances:
./typed-trust/tests/translator.rs:548:      oracle: [Foo]
./typed-trust/tests/translator.rs:553:    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust/tests/translator.rs:569:    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust/tests/translator.rs:572:// --- Evidence + last_verified translation ---
./typed-trust/tests/translator.rs:575:fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust/tests/translator.rs:578:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:585:    // CI tier without populated last_verified → empty reruns.
./typed-trust/tests/translator.rs:614:fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust/tests/translator.rs:617:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:634:    // last_verified is fully populated → one Rerun.
./typed-trust/tests/translator.rs:648:    // last_verified.value is the primary scalar metric).
./typed-trust/tests/translator.rs:662:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:704:    tolerances:
./typed-trust/tests/translator.rs:712:      oracle: [Paper-Authority]
./typed-trust/tests/translator.rs:718:    last_verified:
./typed-trust/tests/translator.rs:729:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:750:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:775:    tolerances:
./typed-trust/tests/translator.rs:782:      oracle: [Manual]
./typed-trust/tests/translator.rs:790:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:817:    tolerances:
./typed-trust/tests/translator.rs:824:      oracle: [Manual]
./typed-trust/tests/translator.rs:831:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:856:    tolerances:
./typed-trust/tests/translator.rs:863:      oracle: [Manual]
./typed-trust/tests/translator.rs:870:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:898:    tolerances:
./typed-trust/tests/translator.rs:905:      oracle: [Manual]
./typed-trust/tests/translator.rs:912:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:941:    tolerances:
./typed-trust/tests/translator.rs:948:      oracle: [Manual]
./typed-trust/tests/translator.rs:956:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:996:    tolerances:
./typed-trust/tests/translator.rs:1003:      oracle: [Manual]
./typed-trust/tests/translator.rs:1014:        let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:1079:    tolerances:
./typed-trust/tests/translator.rs:1087:      oracle: [Repo-README]
./typed-trust/tests/translator.rs:1131:    tolerances:
./typed-trust/tests/translator.rs:1137:      oracle: [Manual]
./typed-trust/tests/translator.rs:1168:    tolerances:
./typed-trust/tests/translator.rs:1174:      oracle: [Manual]
./typed-trust/tests/translator.rs:1209:    tolerances:
./typed-trust/tests/translator.rs:1215:      oracle: [Manual]
./typed-trust/tests/translator.rs:1265:    tolerances:
./typed-trust/tests/translator.rs:1271:      oracle: [Manual]
./typed-trust/tests/translator.rs:1306:    tolerances:
./typed-trust/tests/translator.rs:1312:      oracle: [Biopython]
./typed-trust/tests/translator.rs:1338:    tolerances:
./typed-trust/tests/translator.rs:1344:      oracle: [Manual]
./typed-trust/tests/translator.rs:1355:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:1376:// PR5b: metadata_compatibility claim kind
./typed-trust/tests/translator.rs:1385:    kind: metadata_compatibility
./typed-trust/tests/translator.rs:1414:    kind: metadata_compatibility
./typed-trust/tests/translator.rs:1452:    tolerances:
./typed-trust/tests/translator.rs:1458:      oracle: [Manual]
./typed-trust/tests/translator.rs:1475:    kind: metadata_compatibility
./typed-trust/tests/translator.rs:1493:fn metadata_claim_with_tolerances_is_rejected() {
./typed-trust/tests/translator.rs:1497:    title: metadata claim with tolerances
./typed-trust/tests/translator.rs:1498:    kind: metadata_compatibility
./typed-trust/tests/translator.rs:1507:    tolerances:
./typed-trust/tests/translator.rs:1534:    tolerances:
./typed-trust/tests/translator.rs:1540:      oracle: [Manual]
./typed-trust/tests/translator.rs:1565:    kind: metadata_compatibility
./typed-trust/tests/translator.rs:1577:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:1590:    kind: metadata_compatibility
./typed-trust/tests/translator.rs:1603:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust/tests/translator.rs:1617:    kind: metadata_compatibility
./typed-trust/tests/translator.rs:1645:    kind: metadata_compatibility
./typed-trust/tests/translator.rs:1655:      oracle: [Manual]
./typed-trust/tests/translator.rs:1670:// PR5f: behavioral_concordance translator + Manifest deserialization
./typed-trust/tests/translator.rs:1674:fn behavioral_concordance_numeric_band_translates_with_full_block() {
./typed-trust/tests/translator.rs:1679:    kind: behavioral_concordance
./typed-trust/tests/translator.rs:1706:        typed_trust::ConcordancePattern::NumericBand {
./typed-trust/tests/translator.rs:1723:fn behavioral_concordance_ordinal_match_keyset_alignment_enforced() {
./typed-trust/tests/translator.rs:1730:    kind: behavioral_concordance
./typed-trust/tests/translator.rs:1764:fn behavioral_concordance_same_order_rejects_non_positive_prior() {
./typed-trust/tests/translator.rs:1769:    kind: behavioral_concordance
./typed-trust/tests/translator.rs:1797:fn behavioral_concordance_relative_band_rejects_ratio_at_or_below_one() {
./typed-trust/tests/translator.rs:1802:    kind: behavioral_concordance
./typed-trust/tests/translator.rs:1830:fn behavioral_concordance_rejects_top_level_source() {
./typed-trust/tests/translator.rs:1837:    kind: behavioral_concordance
./typed-trust/tests/translator.rs:1866:fn behavioral_concordance_rejects_oracle_in_evidence() {
./typed-trust/tests/translator.rs:1871:    kind: behavioral_concordance
./typed-trust/tests/translator.rs:1888:      oracle: [BALL]
./typed-trust/tests/translator.rs:1903:fn behavioral_concordance_rejects_tolerances() {
./typed-trust/tests/translator.rs:1908:    kind: behavioral_concordance
./typed-trust/tests/translator.rs:1911:    tolerances:
./typed-trust/tests/translator.rs:1950:    tolerances:
./typed-trust/tests/translator.rs:1956:      oracle: [Manual]
./typed-trust/tests/translator.rs:1984:fn behavioral_concordance_missing_block_rejected() {
./typed-trust/tests/translator.rs:1989:    kind: behavioral_concordance
./typed-trust/tests/translator.rs:2004:fn behavioral_concordance_monotone_with_translates_with_null_prior_value() {
./typed-trust/tests/translator.rs:2009:    kind: behavioral_concordance
./typed-trust/tests/translator.rs:2037:        typed_trust::ConcordancePattern::MonotoneWith {
./typed-trust/tests/mcp_tools.rs:112:    tolerances:
./typed-trust/tests/mcp_tools.rs:118:      oracle: [BALL, Tinker]
./typed-trust/tests/mcp_tools.rs:175:    tolerances:
./typed-trust/tests/mcp_tools.rs:181:      oracle: [Test]
./typed-trust/tests/mcp_tools.rs:435:            "  - id: claim-{i}\n    kind: measurement\n    tier: ci\n    source: .\n    title: t\n    claim: c\n    tolerances:\n      - metric: m\n        op: \"<\"\n        value: 0.02\n        prose: x\n    evidence:\n      oracle: [Test]\n      command: \"true\"\n      artifact: out.json\n"
./typed-trust/tests/mcp_tools.rs:477:    tolerances:
./typed-trust/tests/mcp_tools.rs:483:      oracle: [Paper-Authority]
./typed-trust/tests/mcp_tools.rs:494:    tolerances:
./typed-trust/tests/mcp_tools.rs:500:      oracle: [Biopython]
./typed-trust/tests/mcp_tools.rs:555:    tolerances:
./typed-trust/tests/mcp_tools.rs:561:      oracle: [Biopython]
./typed-trust/tests/mcp_tools.rs:571:    tolerances:
./typed-trust/tests/mcp_tools.rs:577:      oracle: [Repo-README]
./typed-trust/tests/mcp_tools.rs:637:    tolerances:
./typed-trust/tests/mcp_tools.rs:643:      oracle: [Paper-Authority]
./typed-trust/tests/mcp_tools.rs:918:    tolerances:
./typed-trust/tests/mcp_tools.rs:924:      oracle: [Test]
./typed-trust/tests/mcp_tools.rs:928:    kind: metadata_compatibility
./typed-trust/tests/mcp_tools.rs:943:    kind: metadata_compatibility
./typed-trust/tests/mcp_tools.rs:1185:/// error (tier-2 data) for `kind: metadata_compatibility` claims that
./typed-trust/tests/mcp_tools.rs:1195:    kind: metadata_compatibility
./typed-trust/tests/mcp_tools.rs:1229:/// metadata_compatibility`, even if the raw YAML carried a
./typed-trust/tests/mcp_tools.rs:1244:    tolerances:
./typed-trust/tests/mcp_tools.rs:1250:      oracle: [Manual]
./typed-trust/tests/mcp_tools.rs:1319:    kind: behavioral_concordance
./typed-trust/tests/mcp_tools.rs:1337:    kind: behavioral_concordance
./typed-trust/tests/json_output.rs:158:                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust/tests/json_output.rs:180:    // populated to the oracle name.
./typed-trust/tests/json_output.rs:226:    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust/tests/mcp_loadbearing.rs:121:    tolerances:
./typed-trust/tests/mcp_loadbearing.rs:127:      oracle: [BALL, Tinker]
./typed-trust/tests/mcp_loadbearing.rs:341:    tolerances:
./typed-trust/tests/mcp_loadbearing.rs:347:      oracle: [Test]
./typed-trust/tests/mcp_loadbearing.rs:374:    tolerances:
./typed-trust/tests/mcp_loadbearing.rs:380:      oracle: [Test]
./typed-trust/src/claim.rs:48:/// `ConcordancePattern` enum + its per-variant parameters and
./typed-trust/src/claim.rs:54:pub struct ConcordanceDeclaration {
./typed-trust/src/claim.rs:59:    pub pattern: ConcordancePattern,
./typed-trust/src/claim.rs:83:pub enum ConcordancePattern {
./typed-trust/src/claim.rs:189:/// PR5h: typed lift of a `last_concorded.json` entry.
./typed-trust/src/claim.rs:193:/// `last_concorded.json`. typed-trust reads it back here. The
./typed-trust/src/claim.rs:249:/// the typed `ConcordancePattern`, pattern-typed for the variant).
./typed-trust/src/claim.rs:281:pub struct MetadataDeclaration {
./typed-trust/src/claim.rs:298:pub enum ClaimKind {
./typed-trust/src/main.rs:29:    translate_review_event, translate_tolerances, ManifestClaim, ManifestLastVerified,
./typed-trust/src/main.rs:81:    // Overlay sidecar entries onto each claim's last_verified field
./typed-trust/src/main.rs:84:    let last_verified_overlay: HashMap<String, ManifestLastVerified> =
./typed-trust/src/main.rs:97:        if let Some(lv) = last_verified_overlay.get(&cw.claim.id) {
./typed-trust/src/main.rs:98:            cw.claim.last_verified = Some(lv.clone());
./typed-trust/src/main.rs:102:    // PR5h: load `last_concorded.json` (concordance comparator
./typed-trust/src/main.rs:105:    // `last_verified`, concordance claims use `last_concorded`.
./typed-trust/src/main.rs:120:    let mut id_overlap: Vec<&String> = last_verified_overlay
./typed-trust/src/main.rs:127:            "error: {} claim id(s) appear in BOTH last_verified.json and last_concorded.json: {}",
./typed-trust/src/main.rs:136:            "hint: measurement claims use last_verified; concordance claims use last_concorded. \
./typed-trust/src/main.rs:230:        let criteria = match translate_tolerances(mc) {
./typed-trust/src/main.rs:233:                // All translate_tolerances errors at this point are
./typed-trust/src/main.rs:363:    /// status comes from `last_concorded.json`; measurement claims'
./typed-trust/src/main.rs:364:    /// from `last_verified.json` (the `sidecar` field above).
./typed-trust/src/main.rs:453:    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./typed-trust/src/main.rs:668:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./typed-trust/src/main.rs:678:/// PR5h: load `last_concorded.json`. The shape matches the Python
./typed-trust/src/main.rs:812:        let bc_criteria = translate_tolerances(bc)
./typed-trust/src/human_render.rs:33:    // PR5c: metadata_compatibility claims have no criteria — their
./typed-trust/src/human_render.rs:40:    // PR5f: behavioral_concordance claims — surface the pattern
./typed-trust/src/human_render.rs:48:    // not_assessed) when last_concorded.json carried an entry.
./typed-trust/src/human_render.rs:190:/// PR5f: render the behavioral_concordance block. Pattern-typed —
./typed-trust/src/synthesize.rs:7://! observed values against tolerances, applies the §8 rule for
./typed-trust/src/synthesize.rs:30:///   `last_verified`).
./typed-trust/src/evidence.rs:37:///   `evidence.command`. Sidecar `last_verified` gets populated by
./typed-trust/src/evidence.rs:83:    /// Replay depends on human raters / subjective evaluation.
./typed-trust/src/render.rs:200:    /// read from `last_concorded.json`. None when the comparator
./typed-trust/src/render.rs:233:        // PR5f: same pattern for behavioral_concordance claims.
./typed-trust/src/render.rs:237:        // last_concorded.json sidecar; the declaration is what
./typed-trust/src/translate.rs:13://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust/src/translate.rs:14://!   values, populating `against` from a single-oracle heuristic
./typed-trust/src/translate.rs:15://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust/src/translate.rs:23://! - Translating `last_verified` into a [`Rerun`].
./typed-trust/src/translate.rs:59:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust/src/translate.rs:71:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust/src/translate.rs:74:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust/src/translate.rs:77:    /// PR5b: required when ``kind == "metadata_compatibility"``.
./typed-trust/src/translate.rs:84:    /// PR5f: required when ``kind == "behavioral_concordance"``.
./typed-trust/src/translate.rs:92:/// PR5b: structured block for ``kind: metadata_compatibility``
./typed-trust/src/translate.rs:115:/// PR5f: structured block for ``kind: behavioral_concordance``
./typed-trust/src/translate.rs:140:    pub pattern: ManifestConcordancePattern,
./typed-trust/src/translate.rs:160:pub enum ManifestConcordancePattern {
./typed-trust/src/translate.rs:378:    pub oracle: Vec<String>,
./typed-trust/src/translate.rs:409:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust/src/translate.rs:418:    /// release claims must carry structured tolerances.
./typed-trust/src/translate.rs:420:    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust/src/translate.rs:422:    /// tolerances on measurement claims; without them the
./typed-trust/src/translate.rs:446:    /// PR5b: `kind: metadata_compatibility` claim missing the
./typed-trust/src/translate.rs:449:    /// PR5b: metadata claims must NOT carry tolerances; they're
./typed-trust/src/translate.rs:457:    /// PR5f: `kind: behavioral_concordance` claim missing the
./typed-trust/src/translate.rs:460:    /// PR5f: concordance claims must NOT carry tolerances; the
./typed-trust/src/translate.rs:468:    /// PR5f: concordance evidence must NOT carry an `oracle`
./typed-trust/src/translate.rs:469:    /// list. The pattern primitive IS the oracle.
./typed-trust/src/translate.rs:484:    /// PR5f: measurement / metadata_compatibility claims must NOT
./typed-trust/src/translate.rs:520:                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust/src/translate.rs:521:                 add tolerances or change to kind: policy / reference"
./typed-trust/src/translate.rs:545:                "claim {id}: kind=metadata_compatibility requires a \
./typed-trust/src/translate.rs:551:                "claim {id}: kind=metadata_compatibility must NOT carry \
./typed-trust/src/translate.rs:552:                 tolerances; metadata is declarative, not empirical"
./typed-trust/src/translate.rs:556:                "claim {id}: kind=metadata_compatibility must NOT carry \
./typed-trust/src/translate.rs:562:                 block; metadata belongs only to metadata_compatibility claims"
./typed-trust/src/translate.rs:566:                "claim {id}: kind=behavioral_concordance requires a \
./typed-trust/src/translate.rs:572:                "claim {id}: kind=behavioral_concordance must NOT carry \
./typed-trust/src/translate.rs:573:                 `tolerances`; the pattern primitive is the bound"
./typed-trust/src/translate.rs:577:                "claim {id}: kind=behavioral_concordance must NOT carry \
./typed-trust/src/translate.rs:584:                "claim {id}: kind=behavioral_concordance evidence must \
./typed-trust/src/translate.rs:585:                 NOT carry an `oracle` list; the pattern primitive \
./typed-trust/src/translate.rs:586:                 (numeric_band, ordinal_match, etc.) IS the oracle"
./typed-trust/src/translate.rs:608:                "claim {id}: only kind=behavioral_concordance may carry \
./typed-trust/src/translate.rs:645:    // §0 scope: measurement claims (empirical), metadata_compatibility
./typed-trust/src/translate.rs:647:    // behavioral_concordance claims (PR5f — paper measured-behavior
./typed-trust/src/translate.rs:650:        && mc.kind != "metadata_compatibility"
./typed-trust/src/translate.rs:651:        && mc.kind != "behavioral_concordance"
./typed-trust/src/translate.rs:659:    // PR5b: metadata_compatibility claims require the `metadata`
./typed-trust/src/translate.rs:661:    // NOT carry tolerances/evidence — those belong to the empirical
./typed-trust/src/translate.rs:663:    if mc.kind == "metadata_compatibility" {
./typed-trust/src/translate.rs:669:        if mc.tolerances.is_some() {
./typed-trust/src/translate.rs:691:    // PR5f: behavioral_concordance claims require the `concordance`
./typed-trust/src/translate.rs:692:    // block and must NOT carry `tolerances` (the comparator
./typed-trust/src/translate.rs:693:    // primitive IS the bound) or the `oracle` list inside evidence
./typed-trust/src/translate.rs:694:    // (the comparator IS the oracle). They DO carry `evidence`
./typed-trust/src/translate.rs:696:    if mc.kind == "behavioral_concordance" {
./typed-trust/src/translate.rs:702:        if mc.tolerances.is_some() {
./typed-trust/src/translate.rs:716:        // The `oracle` list is a measurement-evidence concept;
./typed-trust/src/translate.rs:718:        // oracle. Reject to make the disjointness load-bearing.
./typed-trust/src/translate.rs:720:            if !ev.oracle.is_empty() {
./typed-trust/src/translate.rs:730:            if let ManifestConcordancePattern::OrdinalMatch {
./typed-trust/src/translate.rs:748:            if let ManifestConcordancePattern::SameOrderOfMagnitude { prior_value, .. } =
./typed-trust/src/translate.rs:758:            if let ManifestConcordancePattern::RelativeBand { ratio, .. } = &cb.pattern {
./typed-trust/src/translate.rs:767:        // A measurement or metadata_compatibility claim that
./typed-trust/src/translate.rs:774:    let kind = if mc.kind == "metadata_compatibility" {
./typed-trust/src/translate.rs:776:    } else if mc.kind == "behavioral_concordance" {
./typed-trust/src/translate.rs:855:        ConcordanceDeclaration, ConcordancePattern, MonotoneDirection, PriorBindingContext,
./typed-trust/src/translate.rs:889:        ManifestConcordancePattern::NumericBand {
./typed-trust/src/translate.rs:893:        } => ConcordancePattern::NumericBand {
./typed-trust/src/translate.rs:898:        ManifestConcordancePattern::RelativeBand {
./typed-trust/src/translate.rs:902:        } => ConcordancePattern::RelativeBand {
./typed-trust/src/translate.rs:907:        ManifestConcordancePattern::SameOrderOfMagnitude {
./typed-trust/src/translate.rs:911:        } => ConcordancePattern::SameOrderOfMagnitude {
./typed-trust/src/translate.rs:922:        ManifestConcordancePattern::OrdinalMatch {
./typed-trust/src/translate.rs:927:        } => ConcordancePattern::OrdinalMatch {
./typed-trust/src/translate.rs:945:        ManifestConcordancePattern::MonotoneWith {
./typed-trust/src/translate.rs:949:        } => ConcordancePattern::MonotoneWith {
./typed-trust/src/translate.rs:975:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust/src/translate.rs:979:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust/src/translate.rs:980:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust/src/translate.rs:982:pub fn translate_tolerances(
./typed-trust/src/translate.rs:985:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust/src/translate.rs:986:        if e.oracle.len() == 1 {
./typed-trust/src/translate.rs:987:            Some(e.oracle[0].clone())
./typed-trust/src/translate.rs:993:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust/src/translate.rs:994:        // Measurement claims require non-empty tolerances per
./typed-trust/src/translate.rs:1014:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust/src/translate.rs:1026:    single_oracle: &Option<String>,
./typed-trust/src/translate.rs:1035:        // tolerances; allowing them to translate would let
./typed-trust/src/translate.rs:1052:            against: single_oracle.clone(),
./typed-trust/src/translate.rs:1065:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust/src/translate.rs:1066:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust/src/translate.rs:1068:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust/src/translate.rs:1083:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust/src/translate.rs:1085:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust/src/translate.rs:1107:    let reruns = translate_last_verified(
./typed-trust/src/translate.rs:1108:        mc.last_verified.as_ref(),
./typed-trust/src/translate.rs:1216:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust/src/translate.rs:1218:/// - `last_verified` is absent;
./typed-trust/src/translate.rs:1219:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust/src/translate.rs:1220:/// - `last_verified.value` is null (no primary observation).
./typed-trust/src/translate.rs:1225:fn translate_last_verified(
./typed-trust/src/translate.rs:1251:        // Shipping convention: a populated last_verified records a
./typed-trust/src/translate.rs:1318:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust/src/translate.rs:1319:///   (most measurement claims compare against an oracle).
./typed-trust/src/translate.rs:1326:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust/src/html_render.rs:152:    // PR5c: metadata_compatibility claims surface the typed
./typed-trust/src/html_render.rs:158:    // PR5f: behavioral_concordance — pattern + paper_locator +
./typed-trust/src/mcp/tools.rs:45:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./typed-trust/src/mcp/tools.rs:52:                "last_verified_sidecar": {"type": "string", "description": "Optional last_verified.json sidecar path"}
./typed-trust/src/mcp/tools.rs:156:        "description": "Query declarative `metadata_compatibility` claims in a manifest. Use when the user asks about config-file claims (Python version requirements, Rust MSRV, Node engine ranges, etc.). For empirical / measurement claims use list_claims + read_report instead.\n\nFilters compose conjunctively, all exact-match and case-sensitive: `field` (semantic name like `rust_msrv`), `source_file` (e.g. `Cargo.toml`). Each result item carries the four metadata fields plus the same audit context list_claims returns (title, tier, provenance_kind, source_id, source_context) so the response is self-contained.",
./typed-trust/src/mcp/tools.rs:179:                "last_verified_sidecar": {"type": "string"},
./typed-trust/src/mcp/tools.rs:190:        "description": "Query declarative `behavioral_concordance` claims in a manifest. Use when the user asks about claims that compare measured behavior to a prior paper's reported behavior (numeric_band / relative_band / same_order_of_magnitude / ordinal_match / monotone_with). For empirical / measurement claims use list_claims + read_report; for declarative config-file claims use query_metadata.\n\nFilter compose conjunctively (exact, case-sensitive): `pattern_kind` (one of the five primitives). Each result item carries the pattern_kind, paper_locator, prior_binding source_id, and the same audit context list_claims returns (title, tier, provenance_kind, source_id, source_context).",
./typed-trust/src/mcp/handlers.rs:24:    translate_tolerances, ManifestClaim, ManifestReviewEvent, ReviewEventSidecar,
./typed-trust/src/mcp/handlers.rs:250:            if c.claim.kind == "metadata_compatibility" {
./typed-trust/src/mcp/handlers.rs:286:    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./typed-trust/src/mcp/handlers.rs:288:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./typed-trust/src/mcp/handlers.rs:520:/// PR5c: structured query path for metadata_compatibility claims.
./typed-trust/src/mcp/handlers.rs:522:/// Walks the manifest, filters to `kind == metadata_compatibility`,
./typed-trust/src/mcp/handlers.rs:534:    // Codex F-PR5c-CR1 (P1): a `kind: metadata_compatibility` claim
./typed-trust/src/mcp/handlers.rs:541:        if c.claim.kind == "metadata_compatibility" && c.claim.metadata.is_none() {
./typed-trust/src/mcp/handlers.rs:543:                "claim {}: kind=metadata_compatibility requires a metadata block",
./typed-trust/src/mcp/handlers.rs:550:        .filter(|c| c.claim.kind == "metadata_compatibility")
./typed-trust/src/mcp/handlers.rs:612:    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./typed-trust/src/mcp/handlers.rs:613:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./typed-trust/src/mcp/handlers.rs:636:    _last_verified_path: Option<&str>,
./typed-trust/src/mcp/handlers.rs:654:    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./typed-trust/src/mcp/handlers.rs:722:                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
./typed-trust/src/mcp/handlers.rs:773:/// PR5h: structured query path for behavioral_concordance claims.
./typed-trust/src/mcp/handlers.rs:781:        if c.claim.kind == "behavioral_concordance" && c.claim.concordance.is_none() {
./typed-trust/src/mcp/handlers.rs:783:                "claim {}: kind=behavioral_concordance requires a concordance block",
./typed-trust/src/mcp/handlers.rs:790:        .filter(|c| c.claim.kind == "behavioral_concordance")
./typed-trust/src/mcp/handlers.rs:795:                crate::translate::ManifestConcordancePattern::NumericBand { .. } => "numeric_band",
./typed-trust/src/mcp/handlers.rs:796:                crate::translate::ManifestConcordancePattern::RelativeBand { .. } => "relative_band",
./typed-trust/src/mcp/handlers.rs:797:                crate::translate::ManifestConcordancePattern::SameOrderOfMagnitude { .. } => "same_order_of_magnitude",
./typed-trust/src/mcp/handlers.rs:798:                crate::translate::ManifestConcordancePattern::OrdinalMatch { .. } => "ordinal_match",
./typed-trust/src/mcp/handlers.rs:799:                crate::translate::ManifestConcordancePattern::MonotoneWith { .. } => "monotone_with",
./typed-trust/src/provenance.rs:3://! Cf. shipping schema's `pinned_versions` + `last_verified` +
./EVIDENT_METADATA_RENDER_MCP_PLAN.md:39:pub struct MetadataDeclaration {
./EVIDENT_METADATA_RENDER_MCP_PLAN.md:60:typed `MetadataDeclaration` when `kind == metadata_compatibility`.
./EVIDENT_METADATA_RENDER_MCP_PLAN.md:112:where `kind == metadata_compatibility` and applies the optional
./EVIDENT_AGENT_PHASE3.codex-review-2.md:43:- **Phase 1**: deterministic replay agent populates `last_verified`.
./EVIDENT_AGENT_PHASE3.codex-review-2.md:156:             canonical last_verified sidecar path)
./EVIDENT_AGENT_PHASE3.codex-review-2.md:202:read_report(manifest_path, claim_id, sidecar?, last_verified_sidecar?)
./EVIDENT_AGENT_PHASE3.codex-review-2.md:237:              last_verified_sidecar?, format)
./EVIDENT_AGENT_PHASE3.codex-review-2.md:389:    last_verified_sidecar: Option<String>,
./EVIDENT_AGENT_PHASE3.codex-review-2.md:498:  files (e.g., passing a `last_verified.json` shape via
./EVIDENT_AGENT_PHASE3.codex-review-2.md:500:- `last_verified.json` sidecars are accepted only via the
./EVIDENT_AGENT_PHASE3.codex-review-2.md:501:  `last_verified_sidecar` parameter; same schema-bind discipline.
./EVIDENT_AGENT_PHASE3.codex-review-2.md:611:- **Sidecar type mismatch.** Passing a `last_verified.json`
./EVIDENT_DESIGN_v0.5_DRAFT.md:147:The shipping `last_verified: {commit, date, value, corpus_sha}` maps to
./EVIDENT_DESIGN_v0.5_DRAFT.md:155:This is the change that makes `last_verified` natively expressible.
./EVIDENT_DESIGN_v0.5_DRAFT.md:212:enum ClaimKind {
./EVIDENT_DESIGN_v0.5_DRAFT.md:334:The relationship to the shipping manifest's `tolerances` list:
./EVIDENT_DESIGN_v0.5_DRAFT.md:338:| `tolerances[].metric` | `Criterion.tolerance.metric` |
./EVIDENT_DESIGN_v0.5_DRAFT.md:339:| `tolerances[].op` | `Criterion.tolerance.op` |
./EVIDENT_DESIGN_v0.5_DRAFT.md:340:| `tolerances[].value` | `Criterion.tolerance.value` |
./EVIDENT_DESIGN_v0.5_DRAFT.md:341:| `tolerances[].output` | `Criterion.tolerance.output` |
./EVIDENT_DESIGN_v0.5_DRAFT.md:342:| `tolerances[].prose` | `Criterion.tolerance.prose` |
./EVIDENT_DESIGN_v0.5_DRAFT.md:396:| `tolerances[].*` | `Criterion.tolerance: Tolerance` |
./EVIDENT_DESIGN_v0.5_DRAFT.md:397:| `evidence.{oracle, command, artifact}` | `Evidence { kind, locator, extraction: Verified }` |
./EVIDENT_DESIGN_v0.5_DRAFT.md:399:| `last_verified.*` | latest entry of `Verified.reproductions` |
./EVIDENT_PHASE5_PR6_DRAFT.md:24:>   that's pages apart from its subject.
./EVIDENT_PHASE5_PR6_DRAFT.md:35:papers, ask the model to extract structured tolerances, validate
./EVIDENT_PHASE5_PR6_DRAFT.md:230:  `comparator_bound_to_wrong_subject`.
./EVIDENT_PHASE5_PR6_DRAFT.md:231:- `wrong_subject_binding.md` — codex-flagged: the comparator
./EVIDENT_PHASE5_PR6_DRAFT.md:276:- `--paper wrong_subject_binding.md` produces 0 claims because
./typed-trust-codex-review-3.md:238:+enum ClaimKind {
./typed-trust-codex-review-3.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-3.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-3.md:368:+                                        // tolerances
./typed-trust-codex-review-3.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-3.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-3.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-3.md:614:+pub enum ClaimKind {
./typed-trust-codex-review-3.md:1224:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-3.md:1287:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-3.md:1500:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-3.md:1971:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-3.md:1994:+///   `last_verified`).
./typed-trust-codex-review-3.md:2550:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-3.md:2551:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-3.md:2552:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-3.md:2560:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-3.md:2594:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-3.md:2606:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-3.md:2609:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-3.md:2641:+    pub oracle: Vec<String>,
./typed-trust-codex-review-3.md:2662:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-3.md:2771:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-3.md:2775:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-3.md:2776:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-3.md:2778:+pub fn translate_tolerances(
./typed-trust-codex-review-3.md:2781:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-3.md:2782:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-3.md:2783:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-3.md:2789:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-3.md:2797:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review-3.md:2809:+    single_oracle: &Option<String>,
./typed-trust-codex-review-3.md:2821:+            against: single_oracle.clone(),
./typed-trust-codex-review-3.md:2834:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-3.md:2835:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-3.md:2837:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-3.md:2852:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-3.md:2854:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-3.md:2864:+    let reruns = translate_last_verified(
./typed-trust-codex-review-3.md:2865:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-3.md:2902:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-3.md:2904:+/// - `last_verified` is absent;
./typed-trust-codex-review-3.md:2905:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-3.md:2906:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-3.md:2911:+fn translate_last_verified(
./typed-trust-codex-review-3.md:2937:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-3.md:3004:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-3.md:3005:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-3.md:3012:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-3.md:3279:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-3.md:3441:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-3.md:3459:+    tolerances:
./typed-trust-codex-review-3.md:3467:+      oracle:
./typed-trust-codex-review-3.md:3472:+    last_verified:
./typed-trust-codex-review-3.md:3493:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-3.md:3758:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-3.md:3780:+    // populated to the oracle name.
./typed-trust-codex-review-3.md:3826:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-3.md:3934:+               within tier-specific tolerances."
./typed-trust-codex-review-3.md:3946:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-3.md:3993:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-3.md:4010:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-3.md:4021:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-3.md:4105:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-3.md:4111:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-3.md:4221:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-3.md:4227:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-3.md:4228:+/// translation working) with last_verified populated.
./typed-trust-codex-review-3.md:4242:+    tolerances:
./typed-trust-codex-review-3.md:4250:+      oracle:
./typed-trust-codex-review-3.md:4255:+    last_verified:
./typed-trust-codex-review-3.md:4264:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-3.md:4278:+    tolerances:
./typed-trust-codex-review-3.md:4285:+      oracle:
./typed-trust-codex-review-3.md:4290:+    last_verified:
./typed-trust-codex-review-3.md:4310:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-3.md:4356:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-3.md:4358:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-3.md:4359:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-3.md:5141:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-3.md:5146:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-3.md:5147:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-3.md:5165:+    tolerances:
./typed-trust-codex-review-3.md:5173:+      oracle:
./typed-trust-codex-review-3.md:5178:+    last_verified:
./typed-trust-codex-review-3.md:5186:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-3.md:5190:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-3.md:5191:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-3.md:5192:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-3.md:5208:+    tolerances:
./typed-trust-codex-review-3.md:5216:+      oracle:
./typed-trust-codex-review-3.md:5221:+    last_verified:
./typed-trust-codex-review-3.md:5235:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-3.md:5236:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-3.md:5246:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-3.md:5248:+      oracle:
./typed-trust-codex-review-3.md:5250:+      command: pytest tests/oracle -v
./typed-trust-codex-review-3.md:5274:+    tolerances:
./typed-trust-codex-review-3.md:5294:+      oracle:
./typed-trust-codex-review-3.md:5296:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-3.md:5321:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-3.md:5342:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-3.md:5344:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-3.md:5352:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-3.md:5353:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-3.md:5358:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-3.md:5373:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-3.md:5381:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-3.md:5385:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-3.md:5407:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-3.md:5437:+    tolerances:
./typed-trust-codex-review-3.md:5443:+      oracle: [SomeOracle]
./typed-trust-codex-review-3.md:5448:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-3.md:5474:+    tolerances:
./typed-trust-codex-review-3.md:5480:+      oracle: [internal]
./typed-trust-codex-review-3.md:5481:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-3.md:5485:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-3.md:5523:+    tolerances:
./typed-trust-codex-review-3.md:5528:+      oracle: [Foo]
./typed-trust-codex-review-3.md:5533:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-3.md:5549:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-3.md:5552:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-3.md:5555:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-3.md:5558:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-3.md:5565:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-3.md:5594:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-3.md:5597:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-3.md:5614:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-3.md:5628:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-3.md:5642:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-3.md:5658: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-3.md:5664: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-3.md:5675:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-3.md:5697:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-3.md:5759:test ci_claim_with_null_last_verified_has_empty_reruns ... ok
./typed-trust-codex-review-3.md:5765:test translates_dssp_tolerances_including_eq_operator ... ok
./typed-trust-codex-review-3.md:5766:test translates_single_oracle_tolerance_populates_against ... ok
./typed-trust-codex-review-3.md:5767:test release_claim_with_populated_last_verified_emits_rerun ... ok
./typed-trust-codex-review-3.md:5808:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-3.md:5871:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-3.md:6033://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-3.md:6034://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-3.md:6035://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-3.md:6043://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-3.md:6077:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-3.md:6089:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-3.md:6092:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-3.md:6124:    pub oracle: Vec<String>,
./typed-trust-codex-review-3.md:6145:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-3.md:6254:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-3.md:6258:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-3.md:6259:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-3.md:6261:pub fn translate_tolerances(
./typed-trust-codex-review-3.md:6264:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-3.md:6265:        if e.oracle.len() == 1 {
./typed-trust-codex-review-3.md:6266:            Some(e.oracle[0].clone())
./typed-trust-codex-review-3.md:6272:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-3.md:6280:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review-3.md:6287://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-3.md:6310:///   `last_verified`).
./typed-trust-codex-review-3.md:6546:pub fn translate_tolerances(
./typed-trust-codex-review-3.md:6549:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-3.md:6550:        if e.oracle.len() == 1 {
./typed-trust-codex-review-3.md:6551:            Some(e.oracle[0].clone())
./typed-trust-codex-review-3.md:6557:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-3.md:6565:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review-3.md:6577:    single_oracle: &Option<String>,
./typed-trust-codex-review-3.md:6589:            against: single_oracle.clone(),
./typed-trust-codex-review-3.md:6602:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-3.md:6603:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-3.md:6605:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-3.md:6620:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-3.md:6622:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-3.md:6632:    let reruns = translate_last_verified(
./typed-trust-codex-review-3.md:6633:        mc.last_verified.as_ref(),
./typed-trust-codex-review-3.md:6670:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-3.md:6672:/// - `last_verified` is absent;
./typed-trust-codex-review-3.md:6673:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-3.md:6674:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-3.md:6679:fn translate_last_verified(
./typed-trust-codex-review-3.md:6705:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-3.md:6772:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-3.md:6773:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-3.md:6780:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-3.md:8021:workflow/validate_manifest.py:226:            f"claim {claim_id}: pinned_versions must include every oracle "
./typed-trust-codex-review-3.md:8041:workflow/SCHEMA.md:110:- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review-3.md:8101:  oracle:       [...]
./EVIDENT_PHASE5_PR5.codex-review.md:118:- If a claim ends up with zero valid tolerances: drops the claim
./EVIDENT_PHASE5_PR5.codex-review.md:122:emit anything, but only validator-approved tolerances reach the
./EVIDENT_PHASE5_PR5.codex-review.md:169:- The model-response processor moves invalid tolerances to rejections
./EVIDENT_PHASE5_PR5.codex-review.md:171:- A claim with zero valid tolerances after validation is dropped
./EVIDENT_PHASE5_PR5.codex-review.md:265:`pyproject.toml`/`Cargo.toml`/`package.json` compatibility metadata should not be improvised into ordinary empirical tolerances unless PR4’s validator already has a compatible representation. A source span like `requires-python = ">=3.10"` is locally bound, but semantically it is not a performance or scientific claim.
./EVIDENT_PHASE5_PR5.codex-review.md:275:  "tolerances": [
./EVIDENT_PHASE5_PR5.codex-review.md:283:  "claim_kind": "metadata_compatibility"
./EVIDENT_PHASE5_PR5.codex-review.md:289:My concrete recommendation: keep pyproject parsing in PR5 only if the validator/render contract already supports version-like tolerances. Otherwise defer metadata to PR5b and make PR5 README/CHANGELOG/docs only.
./EVIDENT_PHASE5_PR5.codex-review.md:351:`pyproject.toml`/`Cargo.toml`/`package.json` compatibility metadata should not be improvised into ordinary empirical tolerances unless PR4’s validator already has a compatible representation. A source span like `requires-python = ">=3.10"` is locally bound, but semantically it is not a performance or scientific claim.
./EVIDENT_PHASE5_PR5.codex-review.md:361:  "tolerances": [
./EVIDENT_PHASE5_PR5.codex-review.md:369:  "claim_kind": "metadata_compatibility"
./EVIDENT_PHASE5_PR5.codex-review.md:375:My concrete recommendation: keep pyproject parsing in PR5 only if the validator/render contract already supports version-like tolerances. Otherwise defer metadata to PR5b and make PR5 README/CHANGELOG/docs only.
./typed-trust-codex-review-6.md:238:+enum ClaimKind {
./typed-trust-codex-review-6.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-6.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-6.md:368:+                                        // tolerances
./typed-trust-codex-review-6.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-6.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-6.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-6.md:614:+pub enum ClaimKind {
./typed-trust-codex-review-6.md:1225:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-6.md:1292:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-6.md:1533:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-6.md:2012:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-6.md:2035:+///   `last_verified`).
./typed-trust-codex-review-6.md:2615:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-6.md:2616:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-6.md:2617:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-6.md:2625:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-6.md:2659:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-6.md:2671:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-6.md:2674:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-6.md:2706:+    pub oracle: Vec<String>,
./typed-trust-codex-review-6.md:2727:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-6.md:2736:+    /// release claims must carry structured tolerances.
./typed-trust-codex-review-6.md:2738:+    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-6.md:2740:+    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-6.md:2769:+                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-6.md:2770:+                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-6.md:2857:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-6.md:2861:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-6.md:2862:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-6.md:2864:+pub fn translate_tolerances(
./typed-trust-codex-review-6.md:2867:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-6.md:2868:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-6.md:2869:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-6.md:2875:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-6.md:2876:+        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-6.md:2896:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-6.md:2908:+    single_oracle: &Option<String>,
./typed-trust-codex-review-6.md:2917:+        // tolerances; allowing them to translate would let
./typed-trust-codex-review-6.md:2934:+            against: single_oracle.clone(),
./typed-trust-codex-review-6.md:2947:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-6.md:2948:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-6.md:2950:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-6.md:2965:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-6.md:2967:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-6.md:2977:+    let reruns = translate_last_verified(
./typed-trust-codex-review-6.md:2978:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-6.md:3015:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-6.md:3017:+/// - `last_verified` is absent;
./typed-trust-codex-review-6.md:3018:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-6.md:3019:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-6.md:3024:+fn translate_last_verified(
./typed-trust-codex-review-6.md:3050:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-6.md:3117:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-6.md:3118:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-6.md:3125:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-6.md:3392:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-6.md:3554:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-6.md:3572:+    tolerances:
./typed-trust-codex-review-6.md:3580:+      oracle:
./typed-trust-codex-review-6.md:3585:+    last_verified:
./typed-trust-codex-review-6.md:3606:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-6.md:3872:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-6.md:3894:+    // populated to the oracle name.
./typed-trust-codex-review-6.md:3940:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-6.md:4048:+               within tier-specific tolerances."
./typed-trust-codex-review-6.md:4060:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-6.md:4107:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-6.md:4124:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-6.md:4135:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-6.md:4219:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-6.md:4225:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-6.md:4335:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-6.md:4341:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-6.md:4342:+/// translation working) with last_verified populated.
./typed-trust-codex-review-6.md:4356:+    tolerances:
./typed-trust-codex-review-6.md:4364:+      oracle:
./typed-trust-codex-review-6.md:4369:+    last_verified:
./typed-trust-codex-review-6.md:4378:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-6.md:4392:+    tolerances:
./typed-trust-codex-review-6.md:4399:+      oracle:
./typed-trust-codex-review-6.md:4404:+    last_verified:
./typed-trust-codex-review-6.md:4424:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-6.md:4472:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-6.md:4474:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-6.md:4475:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-6.md:5449:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-6.md:5454:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-6.md:5455:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-6.md:5473:+    tolerances:
./typed-trust-codex-review-6.md:5481:+      oracle:
./typed-trust-codex-review-6.md:5486:+    last_verified:
./typed-trust-codex-review-6.md:5494:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-6.md:5498:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-6.md:5499:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-6.md:5500:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-6.md:5516:+    tolerances:
./typed-trust-codex-review-6.md:5524:+      oracle:
./typed-trust-codex-review-6.md:5529:+    last_verified:
./typed-trust-codex-review-6.md:5543:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-6.md:5544:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-6.md:5554:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-6.md:5556:+      oracle:
./typed-trust-codex-review-6.md:5558:+      command: pytest tests/oracle -v
./typed-trust-codex-review-6.md:5582:+    tolerances:
./typed-trust-codex-review-6.md:5602:+      oracle:
./typed-trust-codex-review-6.md:5604:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-6.md:5629:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-6.md:5650:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-6.md:5652:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-6.md:5660:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-6.md:5661:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-6.md:5666:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-6.md:5681:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-6.md:5689:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-6.md:5693:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-6.md:5715:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-6.md:5745:+    tolerances:
./typed-trust-codex-review-6.md:5751:+      oracle: [SomeOracle]
./typed-trust-codex-review-6.md:5756:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-6.md:5782:+    tolerances:
./typed-trust-codex-review-6.md:5788:+      oracle: [internal]
./typed-trust-codex-review-6.md:5789:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-6.md:5793:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-6.md:5835:+    tolerances:
./typed-trust-codex-review-6.md:5839:+      oracle: [Foo]
./typed-trust-codex-review-6.md:5840:+      command: pytest tests/oracle
./typed-trust-codex-review-6.md:5844:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-6.md:5855:+fn rejects_measurement_claim_without_tolerances() {
./typed-trust-codex-review-6.md:5856:+    // Codex round 5: kind: measurement requires non-empty tolerances per
./typed-trust-codex-review-6.md:5861:+  - id: measurement-no-tolerances
./typed-trust-codex-review-6.md:5862:+    title: missing tolerances
./typed-trust-codex-review-6.md:5869:+      oracle: [Foo]
./typed-trust-codex-review-6.md:5874:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-6.md:5877:+            assert_eq!(id, "measurement-no-tolerances");
./typed-trust-codex-review-6.md:5884:+fn rejects_measurement_claim_with_empty_tolerances_list() {
./typed-trust-codex-review-6.md:5887:+  - id: measurement-empty-tolerances
./typed-trust-codex-review-6.md:5888:+    title: empty tolerances
./typed-trust-codex-review-6.md:5894:+    tolerances: []
./typed-trust-codex-review-6.md:5896:+      oracle: [Foo]
./typed-trust-codex-review-6.md:5901:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-6.md:5919:+    tolerances:
./typed-trust-codex-review-6.md:5923:+      oracle: [Foo]
./typed-trust-codex-review-6.md:5924:+      command: pytest tests/oracle
./typed-trust-codex-review-6.md:5928:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-6.md:5949:+    tolerances:
./typed-trust-codex-review-6.md:5954:+      oracle: [Foo]
./typed-trust-codex-review-6.md:5959:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-6.md:5975:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-6.md:5978:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-6.md:5981:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-6.md:5984:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-6.md:5991:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-6.md:6020:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-6.md:6023:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-6.md:6040:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-6.md:6054:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-6.md:6068:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-6.md:6084: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-6.md:6090: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-6.md:6101:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-6.md:6123:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-6.md:6232://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-6.md:6233://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-6.md:6234://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-6.md:6242://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-6.md:6276:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-6.md:6288:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-6.md:6291:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-6.md:6323:    pub oracle: Vec<String>,
./typed-trust-codex-review-6.md:6344:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-6.md:6353:    /// release claims must carry structured tolerances.
./typed-trust-codex-review-6.md:6355:    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-6.md:6357:    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-6.md:6386:                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-6.md:6387:                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-6.md:6466://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-6.md:6489:///   `last_verified`).
./typed-trust-codex-review-6.md:6739:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-6.md:6743:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-6.md:6744:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-6.md:6746:pub fn translate_tolerances(
./typed-trust-codex-review-6.md:6749:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-6.md:6750:        if e.oracle.len() == 1 {
./typed-trust-codex-review-6.md:6751:            Some(e.oracle[0].clone())
./typed-trust-codex-review-6.md:6757:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-6.md:6758:        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-6.md:6778:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-6.md:6790:    single_oracle: &Option<String>,
./typed-trust-codex-review-6.md:6799:        // tolerances; allowing them to translate would let
./typed-trust-codex-review-6.md:6816:            against: single_oracle.clone(),
./typed-trust-codex-review-6.md:6829:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-6.md:6830:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-6.md:6832:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-6.md:6847:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-6.md:6849:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-6.md:6859:    let reruns = translate_last_verified(
./typed-trust-codex-review-6.md:6860:        mc.last_verified.as_ref(),
./typed-trust-codex-review-6.md:6897:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-6.md:6899:/// - `last_verified` is absent;
./typed-trust-codex-review-6.md:6900:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-6.md:6901:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-6.md:6906:fn translate_last_verified(
./typed-trust-codex-review-6.md:6932:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-6.md:6999:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-6.md:7000:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-6.md:7007:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-6.md:7385:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-6.md:7452:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-6.md:8062:workflow/SCHEMA.md:110:- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review-6.md:8087:   `subsystem`, `oracle`, and `capability` are declared per-manifest;
./typed-trust-codex-review-6.md:8106:  oracle:       [...]
./typed-trust-codex-review-6.md:8128:| `oracle`           | empty — consumer must define                                       |
./typed-trust-codex-review-6.md:8151:| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
./typed-trust-codex-review-6.md:8153:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-6.md:8154:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-6.md:8157:| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-6.md:8163:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-6.md:8174:- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review-6.md:8175:  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review-6.md:8184:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-6.md:8193:tolerances:
./typed-trust-codex-review-6.md:8951:replace the underlying `tolerances` and `evidence` fields.
./typed-trust-codex-review-6.md:8956:last_verified:
./typed-trust-codex-review-6.md:8966:typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-6.md:8977:  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-6.md:9202:typed-trust/src/translate.rs:125:    /// An unknown comparison operator in `tolerances[].op`.
./EVIDENT_AGENT_PHASE3.codex-review.md:19:- Sidecars: last_verified.json (Phase 1 framework convention, populated by agent) and review_events.json (Phase 2a append-only with fcntl.flock, cross-language canonical event_id parity).
./EVIDENT_AGENT_PHASE3.codex-review.md:43:- **Phase 1**: deterministic replay agent populates `last_verified`.
./EVIDENT_AGENT_PHASE3.codex-review.md:174:`render_report` also accept an optional `last_verified_sidecar` for
./EVIDENT_AGENT_PHASE3.codex-review.md:175:the Phase 1 `last_verified` overlay.
./EVIDENT_AGENT_PHASE3.codex-review.md:238:    last_verified_sidecar: Option<String>,
./EVIDENT_AGENT_PHASE3.codex-review.md:469:3. **Sidecar acceptance rules need to distinguish `review_events.json` and `last_verified.json`.**
./EVIDENT_AGENT_PHASE3.codex-review.md:471:The plan says all tools take optional `sidecar`, and some take `last_verified_sidecar`. That is fine, but acceptance rules should constrain filenames and schemas:
./EVIDENT_AGENT_PHASE3.codex-review.md:474:- `last_verified.json` should be accepted only through `last_verified_sidecar`, then parsed as that schema.
./EVIDENT_AGENT_PHASE3.codex-review.md:486:`cache key = canonical paths + mtimes/sizes of manifest, includes, review_events, last_verified`
./EVIDENT_AGENT_PHASE3.codex-review.md:611:3. **Sidecar acceptance rules need to distinguish `review_events.json` and `last_verified.json`.**
./EVIDENT_AGENT_PHASE3.codex-review.md:613:The plan says all tools take optional `sidecar`, and some take `last_verified_sidecar`. That is fine, but acceptance rules should constrain filenames and schemas:
./EVIDENT_AGENT_PHASE3.codex-review.md:616:- `last_verified.json` should be accepted only through `last_verified_sidecar`, then parsed as that schema.
./EVIDENT_AGENT_PHASE3.codex-review.md:628:`cache key = canonical paths + mtimes/sizes of manifest, includes, review_events, last_verified`
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1120://! observed values against tolerances, applies the §8 rule for
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1143:///   `last_verified`).
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1390://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1391://!   values, populating `against` from a single-oracle heuristic
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1392://!   (multi-oracle disambiguation needs schema work or convention).
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1400://! - Translating `last_verified` into a [`Rerun`].
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1434:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1446:    pub tolerances: Option<Vec<ManifestTolerance>>,
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1449:    pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1481:    pub oracle: Vec<String>,
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1502:    /// An unknown comparison operator in `tolerances[].op`.
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1511:    /// release claims must carry structured tolerances.
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1513:    /// A `kind: measurement` claim omitted `tolerances` or provided
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1515:    /// tolerances on measurement claims; without them the
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1549:                "claim {id}: kind=measurement requires non-empty tolerances; \
./EVIDENT_AGENT_PHASE2D.codex-review-2.md:1550:                 add tolerances or change to kind: policy / reference"
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:1:# Design — `kind: behavioral_concordance` (v4 DRAFT)
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:34:>   Measurement claims consult `last_verified.json`; concordance
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:35:>   claims consult `last_concorded.json`. Missing entries are
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:57:> - **Concordance claims do NOT carry `evidence.oracle`.** The
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:58:>   pattern_kind IS the oracle; the `[BALL, FreeSASA, …]` shape
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:76:>   existing `last_verified` shape (v1 falsely claimed they were
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:98:>   `last_verified.json`. `last_verified.value` (scalar) cannot
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:116:paper's **own** subject. The validator correctly rejected six of
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:117:the seven as `comparator_bound_to_wrong_subject`. The one that
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:135:  kind: behavioral_concordance
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:165:    # Concordance claims do NOT carry `oracle` — the comparator
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:166:    # primitive IS the oracle. They DO carry the docker contract.
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:193:5. **No `evidence.oracle`** — the pattern_kind is the oracle.
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:367:  kind: behavioral_concordance
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:386:versioned contract**, distinct from `last_verified.json`.
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:436:`last_verified.json` is **not** extended. A separate
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:437:`last_concorded.json` carries concordance results:
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:463:Codex's reasoning, applied: `last_verified.value` is a scalar
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:468:## Sidecar boundary: `last_verified.json` vs `last_concorded.json`
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:473:- `kind: measurement` → status reads from `last_verified.json`.
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:476:- `kind: behavioral_concordance` → status reads from
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:477:  `last_concorded.json`. If the claim id is absent, the criterion
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:480:A concordance claim whose id appears in `last_verified.json` is
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:482:claim whose id appears in `last_concorded.json`. The
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:488:does NOT propagate into `last_concorded.json`. The two have
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:532:  `nix-replay` oracle is a follow-up.
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:560:  enum ConcordancePattern {
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:576:  dispatch on `pattern_kind`, write `last_concorded.json`.
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:578:- A new sidecar shape (`last_concorded.json`) that typed-trust
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:579:  reads alongside `last_verified.json`.
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:585:  `measurement` / `metadata_compatibility` paths changes.
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:615:   on `kind == behavioral_concordance` or fall back to
./EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md:648:v1 of `behavioral_concordance` is **complete enough for the
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:6:> observed Y." Distinct from `measurement` (subject is third-party,
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:7:> not the paper's own system) and from `behavioral_concordance`
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:24:wrong_subject`. EVIDENT's `measurement` requires the bound to
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:29:`behavioral_concordance` (the fifth kind, just shipped) doesn't
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:78:   field is the schema-level signal that the subject is NOT the
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:86:3. **`observation.pattern`** — REUSES the `ConcordancePattern`
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:94:Considered: collapse this into `behavioral_concordance` with a
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:107:   `behavioral_concordance`, promotion to `tier:ci` means
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:128:| `ConcordancePattern` enum (5 primitives) | **YES** | Used verbatim. `prior_value` is the paper's observed value. |
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:130:| `last_concorded.json` sidecar shape | **YES** | Same `LastConcordedEntry` shape. The `comparison_status` enum (`pass / fail / not_assessed`) reads the same way. |
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:133:| Kind-keyed sidecar dispatch | **EXTENDED** | Both `behavioral_concordance` AND `third_party_observation` claims read from `last_concorded.json`. Duplicate-claim-id-across-sidecars discipline still applies. |
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:141:| `ObservationDeclaration` struct | `third_party_tool: String`, `metric_definition: String`, `pattern: ConcordancePattern` (reused), `paper_locator: String`. |
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:142:| Translator path | New invariants: kind=third_party_observation requires `observation` block; rejects `tolerances`, `evidence.oracle`, top-level `source`, `metadata` block, `concordance` block. Other kinds reject `observation` block. |
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:154:4. Reject `tolerances` (the pattern primitive IS the bound).
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:155:5. Reject `evidence.oracle` list (the pattern primitive IS the oracle).
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:159:9. Apply the same `ConcordancePattern`-level validation: `OrdinalMatch` keyset alignment, `SameOrderOfMagnitude` positive `prior_value`, `RelativeBand` ratio > 1.0.
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:161:For other kinds (`measurement`, `metadata_compatibility`,
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:162:`behavioral_concordance`): reject `observation` block. Keeps the
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:167:Currently (after PR5h): measurement → `last_verified.json`,
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:168:concordance → `last_concorded.json`. With this PR:
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:170:- `measurement` claims → `last_verified.json`
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:171:- `behavioral_concordance` claims → `last_concorded.json`
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:172:- `third_party_observation` claims → `last_concorded.json`
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:174:Both `behavioral_concordance` and `third_party_observation` use
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:175:`last_concorded.json` because the sidecar entry shape is
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:180:- measurement → read from `last_verified` overlay
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:181:- concordance OR observation → read from `last_concorded` overlay
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:196:- Reworking `ConcordancePattern` or the comparator primitives —
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:198:- Reworking the `last_concorded.json` sidecar shape or
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:210:   `ConcordancePattern`. Should the `prior_value` field be
./EVIDENT_THIRD_PARTY_OBSERVATION_DRAFT.md:216:   `behavioral_concordance.prior_binding.prior_metric_definition`
./checklist/README.md:12:- [ ] Is there an independent oracle?
./typed-trust-codex-review-9.md:238:+enum ClaimKind {
./typed-trust-codex-review-9.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-9.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-9.md:368:+                                        // tolerances
./typed-trust-codex-review-9.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-9.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-9.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-9.md:580:+``last_verified.json`` (framework convention) which typed-trust then
./typed-trust-codex-review-9.md:668:+    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./typed-trust-codex-review-9.md:706:+        sidecar_path = manifest_path.parent / "last_verified.json"
./typed-trust-codex-review-9.md:817:+oracle binaries available.
./typed-trust-codex-review-9.md:1072:+        if not score.tolerances:
./typed-trust-codex-review-9.md:1075:+        first = score.tolerances[0]
./typed-trust-codex-review-9.md:1143:+"""Sidecar ``last_verified.json`` read/write.
./typed-trust-codex-review-9.md:1312:+#  - Writes a sidecar entry in workflow/evident.py's last_verified.json
./typed-trust-codex-review-9.md:1321:+SIDECAR="/scratch/TMAlign/proteon/evident/last_verified.json"
./typed-trust-codex-review-9.md:1625:+    path = tmp_path / "last_verified.json"
./typed-trust-codex-review-9.md:1765:+pub enum ClaimKind {
./typed-trust-codex-review-9.md:2929:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-9.md:2972:+    // Overlay sidecar entries onto each claim's last_verified field
./typed-trust-codex-review-9.md:2980:+                        cw.claim.last_verified = Some(lv.clone());
./typed-trust-codex-review-9.md:3020:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-9.md:3143:+    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./typed-trust-codex-review-9.md:3380:+/// The shape matches `workflow/evident.py`'s `last_verified.json`
./typed-trust-codex-review-9.md:3397:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-9.md:3893:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-9.md:3916:+///   `last_verified`).
./typed-trust-codex-review-9.md:4537:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-9.md:4538:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-9.md:4539:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-9.md:4547:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-9.md:4581:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-9.md:4593:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-9.md:4596:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-9.md:4628:+    pub oracle: Vec<String>,
./typed-trust-codex-review-9.md:4649:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-9.md:4658:+    /// release claims must carry structured tolerances.
./typed-trust-codex-review-9.md:4660:+    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-9.md:4662:+    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-9.md:4696:+                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-9.md:4697:+                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-9.md:4789:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-9.md:4793:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-9.md:4794:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-9.md:4796:+pub fn translate_tolerances(
./typed-trust-codex-review-9.md:4799:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-9.md:4800:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-9.md:4801:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-9.md:4807:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-9.md:4808:+        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-9.md:4828:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-9.md:4840:+    single_oracle: &Option<String>,
./typed-trust-codex-review-9.md:4849:+        // tolerances; allowing them to translate would let
./typed-trust-codex-review-9.md:4866:+            against: single_oracle.clone(),
./typed-trust-codex-review-9.md:4879:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-9.md:4880:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-9.md:4882:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-9.md:4897:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-9.md:4899:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-9.md:4920:+    let reruns = translate_last_verified(
./typed-trust-codex-review-9.md:4921:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-9.md:4958:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-9.md:4960:+/// - `last_verified` is absent;
./typed-trust-codex-review-9.md:4961:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-9.md:4962:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-9.md:4967:+fn translate_last_verified(
./typed-trust-codex-review-9.md:4993:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-9.md:5060:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-9.md:5061:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-9.md:5068:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-9.md:5479:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-9.md:5639:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-9.md:5657:+    tolerances:
./typed-trust-codex-review-9.md:5665:+      oracle:
./typed-trust-codex-review-9.md:5670:+    last_verified:
./typed-trust-codex-review-9.md:5691:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-9.md:5846:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-9.md:5864:+    tolerances:
./typed-trust-codex-review-9.md:5872:+      oracle:
./typed-trust-codex-review-9.md:5877:+    last_verified:
./typed-trust-codex-review-9.md:5898:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-9.md:6243:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-9.md:6265:+    // populated to the oracle name.
./typed-trust-codex-review-9.md:6311:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-9.md:6419:+               within tier-specific tolerances."
./typed-trust-codex-review-9.md:6431:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-9.md:6478:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-9.md:6495:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-9.md:6506:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-9.md:6590:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-9.md:6596:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-9.md:6706:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-9.md:6712:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-9.md:6713:+/// translation working) with last_verified populated.
./typed-trust-codex-review-9.md:6727:+    tolerances:
./typed-trust-codex-review-9.md:6735:+      oracle:
./typed-trust-codex-review-9.md:6740:+    last_verified:
./typed-trust-codex-review-9.md:6749:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-9.md:6763:+    tolerances:
./typed-trust-codex-review-9.md:6770:+      oracle:
./typed-trust-codex-review-9.md:6775:+    last_verified:
./typed-trust-codex-review-9.md:6795:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-9.md:6843:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-9.md:6845:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-9.md:6846:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-9.md:8165:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-9.md:8170:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-9.md:8171:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-9.md:8189:+    tolerances:
./typed-trust-codex-review-9.md:8197:+      oracle:
./typed-trust-codex-review-9.md:8202:+    last_verified:
./typed-trust-codex-review-9.md:8210:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-9.md:8214:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-9.md:8215:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-9.md:8216:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-9.md:8232:+    tolerances:
./typed-trust-codex-review-9.md:8240:+      oracle:
./typed-trust-codex-review-9.md:8245:+    last_verified:
./typed-trust-codex-review-9.md:8259:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-9.md:8260:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-9.md:8270:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-9.md:8272:+      oracle:
./typed-trust-codex-review-9.md:8274:+      command: pytest tests/oracle -v
./typed-trust-codex-review-9.md:8298:+    tolerances:
./typed-trust-codex-review-9.md:8318:+      oracle:
./typed-trust-codex-review-9.md:8320:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-9.md:8345:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-9.md:8366:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-9.md:8368:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-9.md:8376:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-9.md:8377:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-9.md:8382:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-9.md:8397:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-9.md:8405:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-9.md:8409:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-9.md:8431:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-9.md:8461:+    tolerances:
./typed-trust-codex-review-9.md:8467:+      oracle: [SomeOracle]
./typed-trust-codex-review-9.md:8472:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-9.md:8498:+    tolerances:
./typed-trust-codex-review-9.md:8504:+      oracle: [internal]
./typed-trust-codex-review-9.md:8505:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-9.md:8509:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-9.md:8551:+    tolerances:
./typed-trust-codex-review-9.md:8555:+      oracle: [Foo]
./typed-trust-codex-review-9.md:8556:+      command: pytest tests/oracle
./typed-trust-codex-review-9.md:8560:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-9.md:8571:+fn rejects_measurement_claim_without_tolerances() {
./typed-trust-codex-review-9.md:8572:+    // Codex round 5: kind: measurement requires non-empty tolerances per
./typed-trust-codex-review-9.md:8577:+  - id: measurement-no-tolerances
./typed-trust-codex-review-9.md:8578:+    title: missing tolerances
./typed-trust-codex-review-9.md:8585:+      oracle: [Foo]
./typed-trust-codex-review-9.md:8590:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-9.md:8593:+            assert_eq!(id, "measurement-no-tolerances");
./typed-trust-codex-review-9.md:8614:+    tolerances:
./typed-trust-codex-review-9.md:8621:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-9.md:8633:+fn rejects_measurement_claim_with_empty_tolerances_list() {
./typed-trust-codex-review-9.md:8636:+  - id: measurement-empty-tolerances
./typed-trust-codex-review-9.md:8637:+    title: empty tolerances
./typed-trust-codex-review-9.md:8643:+    tolerances: []
./typed-trust-codex-review-9.md:8645:+      oracle: [Foo]
./typed-trust-codex-review-9.md:8650:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-9.md:8668:+    tolerances:
./typed-trust-codex-review-9.md:8672:+      oracle: [Foo]
./typed-trust-codex-review-9.md:8673:+      command: pytest tests/oracle
./typed-trust-codex-review-9.md:8677:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-9.md:8698:+    tolerances:
./typed-trust-codex-review-9.md:8703:+      oracle: [Foo]
./typed-trust-codex-review-9.md:8708:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-9.md:8724:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-9.md:8727:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-9.md:8730:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-9.md:8733:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-9.md:8740:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-9.md:8769:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-9.md:8772:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-9.md:8789:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-9.md:8803:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-9.md:8817:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-9.md:8833: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-9.md:8839: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-9.md:8850:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-9.md:8872:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-9.md:9068:    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./typed-trust-codex-review-9.md:9106:        sidecar_path = manifest_path.parent / "last_verified.json"
./typed-trust-codex-review-9.md:9211:oracle binaries available.
./typed-trust-codex-review-9.md:9458:        if not score.tolerances:
./typed-trust-codex-review-9.md:9461:        first = score.tolerances[0]
./typed-trust-codex-review-9.md:9523:"""Sidecar ``last_verified.json`` read/write.
./typed-trust-codex-review-9.md:9694:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-9.md:9737:    // Overlay sidecar entries onto each claim's last_verified field
./typed-trust-codex-review-9.md:9745:                        cw.claim.last_verified = Some(lv.clone());
./typed-trust-codex-review-9.md:9785:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-9.md:9908:    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./typed-trust-codex-review-9.md:9938://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-9.md:9939://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-9.md:9940://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-9.md:9948://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-9.md:9982:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-9.md:9994:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-9.md:9997:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-9.md:10029:    pub oracle: Vec<String>,
./typed-trust-codex-review-9.md:10050:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-9.md:10059:    /// release claims must carry structured tolerances.
./typed-trust-codex-review-9.md:10061:    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-9.md:10063:    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-9.md:10097:                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-9.md:10098:                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-9.md:10195:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-9.md:10199:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-9.md:10200:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-9.md:10202:pub fn translate_tolerances(
./typed-trust-codex-review-9.md:10205:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-9.md:10206:        if e.oracle.len() == 1 {
./typed-trust-codex-review-9.md:10207:            Some(e.oracle[0].clone())
./typed-trust-codex-review-9.md:10213:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-9.md:10214:        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-9.md:10234:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-9.md:10246:    single_oracle: &Option<String>,
./typed-trust-codex-review-9.md:10255:        // tolerances; allowing them to translate would let
./typed-trust-codex-review-9.md:10272:            against: single_oracle.clone(),
./typed-trust-codex-review-9.md:10285:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-9.md:10286:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-9.md:10288:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-9.md:10303:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-9.md:10305:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-9.md:10326:    let reruns = translate_last_verified(
./typed-trust-codex-review-9.md:10327:        mc.last_verified.as_ref(),
./typed-trust-codex-review-9.md:10364:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-9.md:10366:/// - `last_verified` is absent;
./typed-trust-codex-review-9.md:10367:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-9.md:10368:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-9.md:10373:fn translate_last_verified(
./typed-trust-codex-review-9.md:10399:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-9.md:10466:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-9.md:10467:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-9.md:10474:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-9.md:10512:    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./typed-trust-codex-review-9.md:10749:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./typed-trust-codex-review-9.md:10783://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-9.md:10806:///   `last_verified`).
./typed-trust-codex-review-9.md:12015:   480	/// The shape matches `workflow/evident.py`'s `last_verified.json`
./typed-trust-codex-review-9.md:12355:/bin/bash -lc "grep -n \"oracle\\|evidence\" -n workflow/SCHEMA.md | head -80" in /scratch/TMAlign/evident
./typed-trust-codex-review-9.md:12357:23:   `subsystem`, `oracle`, and `capability` are declared per-manifest;
./typed-trust-codex-review-9.md:12358:42:  oracle:       [...]
./typed-trust-codex-review-9.md:12359:64:| `oracle`           | empty — consumer must define                                       |
./typed-trust-codex-review-9.md:12360:87:| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
./typed-trust-codex-review-9.md:12361:90:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-9.md:12362:110:- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review-9.md:12363:111:  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review-9.md:12364:177:  Biopython: "1.83"              # one entry per oracle named in evidence.oracle
./typed-trust-codex-review-9.md:12365:181:The validator requires that every name in `evidence.oracle` appears as a
./typed-trust-codex-review-9.md:12366:202:- `automatic` — produced by a runner (CI, an oracle script, a benchmark).
./typed-trust-codex-review-9.md:12369:219:replace the underlying `tolerances` and `evidence` fields.
./typed-trust-codex-review-9.md:12371:267:- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-9.md:12372:292:AND oracle ∩ {OpenMM, BALL} != ∅
./typed-trust-codex-review-9.md:12374:307:1. Lift `evidence.tolerance` into one or more `tolerances:` entries.
./typed-trust-codex-review-9.md:12392:| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
./typed-trust-codex-review-9.md:12394:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-9.md:12395:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-9.md:12398:| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-9.md:12404:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-9.md:12449:Each named entry can be referenced by `tolerances[].output` so a single
./typed-trust-codex-review-9.md:12468:still need their own oracle or benchmark commands.
./typed-trust-codex-review-9.md:12504:    "oracle": set(),
./typed-trust-codex-review-9.md:12529:    "tolerances",
./typed-trust-codex-review-9.md:12587:    for field in ("oracle", "command", "artifact"):
./typed-trust-codex-review-9.md:12593:            f"move tolerance text into the top-level tolerances: list "
./typed-trust-codex-review-9.md:12596:    oracles = require_string_list(value["oracle"], "evidence.oracle", claim_id)
./typed-trust-codex-review-9.md:12597:    for oracle_name in oracles:
./typed-trust-codex-review-9.md:12599:            oracle_name, "oracle", vocabularies, "evidence.oracle[]", claim_id
./typed-trust-codex-review-9.md:12603:    return oracles
./typed-trust-codex-review-9.md:12606:def validate_tolerances(
./typed-trust-codex-review-9.md:12610:        fail(f"claim {claim_id}: tolerances must be a non-empty list")
./typed-trust-codex-review-9.md:12613:            fail(f"claim {claim_id}: tolerances[{i}] must be a mapping")
./typed-trust-codex-review-9.md:12616:            fail(f"claim {claim_id}: tolerances[{i}].prose is required")
./typed-trust-codex-review-9.md:12624:                f"claim {claim_id}: tolerances[{i}] metric/op/value are "
./typed-trust-codex-review-9.md:12632:                f"tolerances[{i}].metric",
./typed-trust-codex-review-9.md:12639:                f"tolerances[{i}].op",
./typed-trust-codex-review-9.md:12645:                fail(f"claim {claim_id}: tolerances[{i}].value must be numeric")
./typed-trust-codex-review-9.md:12668:    value: Any, oracle_names: list[str], project: str, claim_id: str
./typed-trust-codex-review-9.md:12685:    missing_oracles = [o for o in oracle_names if o not in value]
./typed-trust-codex-review-9.md:12686:    if missing_oracles:
./typed-trust-codex-review-9.md:12688:            f"claim {claim_id}: pinned_versions must include every oracle "
./typed-trust-codex-review-9.md:12689:            f"named in evidence.oracle; missing: {missing_oracles}"
./typed-trust-codex-review-9.md:12750:def validate_last_verified(value: Any, claim_id: str) -> None:
./typed-trust-codex-review-9.md:12752:        fail(f"claim {claim_id}: last_verified must be a mapping")
./typed-trust-codex-review-9.md:12760:                fail(f"claim {claim_id}: last_verified.value must be numeric or null")
./typed-trust-codex-review-9.md:12762:            fail(f"claim {claim_id}: last_verified.{key} must be a string or null")
./typed-trust-codex-review-9.md:12886:        oracles = validate_evidence(claim["evidence"], claim_id, vocabularies)
./typed-trust-codex-review-9.md:12901:        if "last_verified" in claim:
./typed-trust-codex-review-9.md:12902:            validate_last_verified(claim["last_verified"], claim_id)
./typed-trust-codex-review-9.md:12919:            validate_tolerances(claim["tolerances"], claim_id, vocabularies)
./typed-trust-codex-review-9.md:12921:                claim["pinned_versions"], oracles, project, claim_id
./EVIDENT_DESIGN_v0.3.codex-review.md:52:fact. That discipline is the subject of this document.
./EVIDENT_DESIGN_v0.3.codex-review.md:211:enum ClaimKind {
./EVIDENT_DESIGN_v0.3.codex-review.md:672:enum ClaimKind {
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:21:`comparator_bound_to_wrong_subject`):
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:65:  kind: behavioral_concordance
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:107:    oracle: [docker-replay]
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:135:4. **`oracle: [docker-replay]` is silly.** Measurement claims list real oracles like `[BALL, FreeSASA, Biopython]`. Concordance claims have one mechanism (the comparator) and one data source (the docker artifact). Listing `docker-replay` as an oracle pretends concordance has the same shape as measurement; it doesn't. **Suggested fix:** drop the `oracle` field for concordance claims. The pattern_kind IS the oracle.
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:146:  kind: behavioral_concordance
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:245:| `oracle: [...]` field carried over from measurement | Doesn't fit concordance. Drop. |
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:257:3. **Drop `oracle` from concordance claims.**
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:269:because the subject is wrong (third-party tool, not paper);
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:270:`behavioral_concordance` doesn't fit because there's often no
./EVIDENT_BEHAVIORAL_CONCORDANCE_RUSTIMS_SKETCH.md:276:subject is not the paper's own system; differs from concordance
./viewer/src/lib/manifest.ts:44:  oracle: string[];
./viewer/src/lib/manifest.ts:64:  tolerances?: Tolerance[];
./viewer/src/lib/manifest.ts:68:  last_verified?: LastVerified;
./viewer/src/lib/manifest.ts:78:  oracle: string[];
./viewer/src/lib/manifest.ts:107:  oracle: [],
./viewer/src/lib/manifest.ts:149:    oracle: [...new Set([...BASE_VOCAB.oracle, ...(declared.oracle ?? [])])],
./viewer/src/lib/match.ts:12:  oracles?: string[];
./viewer/src/lib/match.ts:53:  if (!claim.tolerances) return false;
./viewer/src/lib/match.ts:56:  for (const t of claim.tolerances) {
./viewer/src/lib/match.ts:87:  if (profile.oracles?.length) {
./viewer/src/lib/match.ts:88:    const claimOracles = new Set(claim.evidence?.oracle ?? []);
./viewer/src/lib/match.ts:89:    const hit = profile.oracles.some((o) => claimOracles.has(o));
./viewer/src/lib/match.ts:91:      missing.push(`oracle (claim has ${[...claimOracles].join(', ') || 'none'})`);
./viewer/src/lib/match.ts:157:      c.tolerances?.some((t) => t.metric === profile.tolerance!.metric)
./viewer/src/lib/match.ts:202:  const oracles = splitCsv('oracle');
./viewer/src/lib/match.ts:216:    oracles,
./viewer/src/lib/match.ts:228:  if (profile.oracles?.length) params.set('oracle', profile.oracles.join(','));
./typed-trust-codex-review-2.md:238:+enum ClaimKind {
./typed-trust-codex-review-2.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-2.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-2.md:368:+                                        // tolerances
./typed-trust-codex-review-2.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-2.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-2.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-2.md:614:+pub enum ClaimKind {
./typed-trust-codex-review-2.md:995:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-2.md:1064:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-2.md:1128:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-2.md:1604:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-2.md:1627:+///   `last_verified`).
./typed-trust-codex-review-2.md:2052:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-2.md:2053:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-2.md:2054:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-2.md:2062:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-2.md:2092:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-2.md:2104:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-2.md:2107:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-2.md:2139:+    pub oracle: Vec<String>,
./typed-trust-codex-review-2.md:2160:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-2.md:2269:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-2.md:2273:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-2.md:2274:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-2.md:2276:+pub fn translate_tolerances(
./typed-trust-codex-review-2.md:2279:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-2.md:2280:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-2.md:2281:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-2.md:2287:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-2.md:2295:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review-2.md:2307:+    single_oracle: &Option<String>,
./typed-trust-codex-review-2.md:2319:+            against: single_oracle.clone(),
./typed-trust-codex-review-2.md:2332:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-2.md:2333:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-2.md:2335:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-2.md:2350:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-2.md:2352:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-2.md:2362:+    let reruns = translate_last_verified(
./typed-trust-codex-review-2.md:2363:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-2.md:2400:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-2.md:2402:+/// - `last_verified` is absent;
./typed-trust-codex-review-2.md:2403:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-2.md:2404:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-2.md:2409:+fn translate_last_verified(
./typed-trust-codex-review-2.md:2435:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-2.md:2502:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-2.md:2503:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-2.md:2510:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-2.md:2713:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-2.md:3024:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-2.md:3046:+    // populated to the oracle name.
./typed-trust-codex-review-2.md:3092:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-2.md:3200:+               within tier-specific tolerances."
./typed-trust-codex-review-2.md:3212:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-2.md:3259:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-2.md:3276:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-2.md:3287:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-2.md:3371:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-2.md:3377:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-2.md:3487:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-2.md:3493:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-2.md:3494:+/// translation working) with last_verified populated.
./typed-trust-codex-review-2.md:3508:+    tolerances:
./typed-trust-codex-review-2.md:3516:+      oracle:
./typed-trust-codex-review-2.md:3521:+    last_verified:
./typed-trust-codex-review-2.md:3530:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-2.md:3544:+    tolerances:
./typed-trust-codex-review-2.md:3551:+      oracle:
./typed-trust-codex-review-2.md:3556:+    last_verified:
./typed-trust-codex-review-2.md:3576:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-2.md:3622:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-2.md:3624:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-2.md:3625:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-2.md:4142:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-2.md:4147:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-2.md:4148:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-2.md:4166:+    tolerances:
./typed-trust-codex-review-2.md:4174:+      oracle:
./typed-trust-codex-review-2.md:4179:+    last_verified:
./typed-trust-codex-review-2.md:4187:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-2.md:4191:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-2.md:4192:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-2.md:4193:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-2.md:4209:+    tolerances:
./typed-trust-codex-review-2.md:4217:+      oracle:
./typed-trust-codex-review-2.md:4222:+    last_verified:
./typed-trust-codex-review-2.md:4236:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-2.md:4237:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-2.md:4247:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-2.md:4249:+      oracle:
./typed-trust-codex-review-2.md:4251:+      command: pytest tests/oracle -v
./typed-trust-codex-review-2.md:4275:+    tolerances:
./typed-trust-codex-review-2.md:4295:+      oracle:
./typed-trust-codex-review-2.md:4297:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-2.md:4322:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-2.md:4343:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-2.md:4345:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-2.md:4353:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-2.md:4354:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-2.md:4359:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-2.md:4374:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-2.md:4382:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-2.md:4386:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-2.md:4408:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-2.md:4438:+    tolerances:
./typed-trust-codex-review-2.md:4444:+      oracle: [SomeOracle]
./typed-trust-codex-review-2.md:4449:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-2.md:4475:+    tolerances:
./typed-trust-codex-review-2.md:4481:+      oracle: [internal]
./typed-trust-codex-review-2.md:4482:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-2.md:4486:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-2.md:4524:+    tolerances:
./typed-trust-codex-review-2.md:4529:+      oracle: [Foo]
./typed-trust-codex-review-2.md:4534:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-2.md:4550:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-2.md:4553:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-2.md:4556:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-2.md:4559:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-2.md:4566:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-2.md:4595:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-2.md:4598:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-2.md:4615:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-2.md:4629:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-2.md:4643:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-2.md:4659: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-2.md:4665: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-2.md:4676:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-2.md:4698:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-2.md:4782:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-2.md:4851:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-2.md:4919://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-2.md:4920://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-2.md:4921://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-2.md:4929://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-2.md:4959:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-2.md:4971:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-2.md:4974:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-2.md:5006:    pub oracle: Vec<String>,
./typed-trust-codex-review-2.md:5027:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-2.md:5136:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-2.md:5140:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-2.md:5141:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-2.md:5143:pub fn translate_tolerances(
./typed-trust-codex-review-2.md:5146:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-2.md:5147:        if e.oracle.len() == 1 {
./typed-trust-codex-review-2.md:5148:            Some(e.oracle[0].clone())
./typed-trust-codex-review-2.md:5154:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-2.md:5162:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review-2.md:5179:    single_oracle: &Option<String>,
./typed-trust-codex-review-2.md:5191:            against: single_oracle.clone(),
./typed-trust-codex-review-2.md:5204:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-2.md:5205:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-2.md:5207:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-2.md:5222:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-2.md:5224:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-2.md:5234:    let reruns = translate_last_verified(
./typed-trust-codex-review-2.md:5235:        mc.last_verified.as_ref(),
./typed-trust-codex-review-2.md:5272:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-2.md:5274:/// - `last_verified` is absent;
./typed-trust-codex-review-2.md:5275:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-2.md:5276:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-2.md:5281:fn translate_last_verified(
./typed-trust-codex-review-2.md:5307:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-2.md:5374:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-2.md:5375:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-2.md:5382:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-2.md:5403://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-2.md:5426:///   `last_verified`).
./typed-trust-codex-review-2.md:6060:pub enum ClaimKind {
./typed-trust-codex-review-2.md:6411:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-2.md:6480:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-2.md:6539://! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-2.md:6995://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-2.md:7018:///   `last_verified`).
./typed-trust-codex-review-2.md:7222://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-2.md:7223://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-2.md:7224://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-2.md:7232://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-2.md:7262:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-2.md:7274:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-2.md:7277:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-2.md:7309:    pub oracle: Vec<String>,
./typed-trust-codex-review-2.md:7330:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-2.md:7884:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-2.md:7890:/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-2.md:7891:/// translation working) with last_verified populated.
./typed-trust-codex-review-2.md:7905:    tolerances:
./typed-trust-codex-review-2.md:7913:      oracle:
./typed-trust-codex-review-2.md:7918:    last_verified:
./typed-trust-codex-review-2.md:7927:/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-2.md:7941:    tolerances:
./typed-trust-codex-review-2.md:7948:      oracle:
./typed-trust-codex-review-2.md:7953:    last_verified:
./typed-trust-codex-review-2.md:7973:    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-2.md:8019:    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-2.md:8021:        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-2.md:8022:        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-2.md:8488:  claim. These may install large oracle tools or use larger datasets.
./typed-trust-codex-review-2.md:8501:- What oracle or reference is used?
./typed-trust-codex-review-2.md:8514:- `case-proteon`: Proteon plus release-grade structural-bioinformatics oracles.
./typed-trust-codex-review-2.md:8535:This checks manifest structure and local paths. It does not run domain oracles.
./typed-trust-codex-review-2.md:8555:  oracle:
./typed-trust-codex-review-2.md:8577:      oracle:
./viewer/src/styles/global.css:248:ul.tolerances { list-style: none; padding-left: 0; }
./viewer/src/styles/global.css:249:ul.tolerances li {
./EVIDENT_AGENT_PHASE3.code-review.md:760:     translate_review_event, translate_tolerances, ManifestClaim, ManifestLastVerified,
./EVIDENT_AGENT_PHASE3.code-review.md:869:+    translate_tolerances, ManifestClaim, ManifestReviewEvent, ReviewEventSidecar,
./EVIDENT_AGENT_PHASE3.code-review.md:1064:+    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review.md:1066:+    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review.md:1309:+    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review.md:1310:+    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review.md:1333:+    _last_verified_path: Option<&str>,
./EVIDENT_AGENT_PHASE3.code-review.md:1349:+    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_AGENT_PHASE3.code-review.md:1400:+                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
./EVIDENT_AGENT_PHASE3.code-review.md:1690:+        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_AGENT_PHASE3.code-review.md:1697:+                "last_verified_sidecar": {"type": "string", "description": "Optional last_verified.json sidecar path"}
./EVIDENT_AGENT_PHASE3.code-review.md:1808:+                "last_verified_sidecar": {"type": "string"},
./EVIDENT_AGENT_PHASE3.code-review.md:1857:    24	    translate_tolerances, ManifestClaim, ManifestReviewEvent, ReviewEventSidecar,
./EVIDENT_AGENT_PHASE3.code-review.md:2052:   219	    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review.md:2054:   221	    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review.md:2298:   464	    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review.md:2299:   465	    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review.md:2322:   488	    _last_verified_path: Option<&str>,
./EVIDENT_AGENT_PHASE3.code-review.md:2338:   504	    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_AGENT_PHASE3.code-review.md:2389:   555	                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
./EVIDENT_AGENT_PHASE3.code-review.md:2910:    translate_review_event, translate_tolerances, ManifestClaim, ManifestLastVerified,
./EVIDENT_AGENT_PHASE3.code-review.md:2961:    // Overlay sidecar entries onto each claim's last_verified field
./EVIDENT_AGENT_PHASE3.code-review.md:2969:                        cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review.md:3038:        let criteria = match translate_tolerances(mc) {
./EVIDENT_AGENT_PHASE3.code-review.md:3041:                // All translate_tolerances errors at this point are
./EVIDENT_AGENT_PHASE3.code-review.md:3133:    29	    translate_review_event, translate_tolerances, ManifestClaim, ManifestLastVerified,
./EVIDENT_AGENT_PHASE3.code-review.md:3184:    80	    // Overlay sidecar entries onto each claim's last_verified field
./EVIDENT_AGENT_PHASE3.code-review.md:3192:    88	                        cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review.md:3261:   157	        let criteria = match translate_tolerances(mc) {
./EVIDENT_AGENT_PHASE3.code-review.md:3264:   160	                // All translate_tolerances errors at this point are
./EVIDENT_AGENT_PHASE3.code-review.md:3411:grep -R \"last_verified\" -n typed-trust/src/translate.rs typed-trust/src | head -50" in /scratch/TMAlign/evident
./EVIDENT_AGENT_PHASE3.code-review.md:3437:   363	    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./EVIDENT_AGENT_PHASE3.code-review.md:3505:typed-trust/src/translate.rs:23://! - Translating `last_verified` into a [`Rerun`].
./EVIDENT_AGENT_PHASE3.code-review.md:3506:typed-trust/src/translate.rs:57:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./EVIDENT_AGENT_PHASE3.code-review.md:3507:typed-trust/src/translate.rs:72:    pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_AGENT_PHASE3.code-review.md:3508:typed-trust/src/translate.rs:373:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./EVIDENT_AGENT_PHASE3.code-review.md:3509:typed-trust/src/translate.rs:375:///   criterion id (shipping convention: `last_verified.value` is the
./EVIDENT_AGENT_PHASE3.code-review.md:3510:typed-trust/src/translate.rs:396:    let reruns = translate_last_verified(
./EVIDENT_AGENT_PHASE3.code-review.md:3511:typed-trust/src/translate.rs:397:        mc.last_verified.as_ref(),
./EVIDENT_AGENT_PHASE3.code-review.md:3512:typed-trust/src/translate.rs:434:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./EVIDENT_AGENT_PHASE3.code-review.md:3513:typed-trust/src/translate.rs:436:/// - `last_verified` is absent;
./EVIDENT_AGENT_PHASE3.code-review.md:3514:typed-trust/src/translate.rs:437:/// - `last_verified.date` is null (replay loop hasn't run);
./EVIDENT_AGENT_PHASE3.code-review.md:3515:typed-trust/src/translate.rs:438:/// - `last_verified.value` is null (no primary observation).
./EVIDENT_AGENT_PHASE3.code-review.md:3516:typed-trust/src/translate.rs:443:fn translate_last_verified(
./EVIDENT_AGENT_PHASE3.code-review.md:3517:typed-trust/src/translate.rs:469:        // Shipping convention: a populated last_verified records a
./EVIDENT_AGENT_PHASE3.code-review.md:3518:typed-trust/src/provenance.rs:3://! Cf. shipping schema's `pinned_versions` + `last_verified` +
./EVIDENT_AGENT_PHASE3.code-review.md:3519:typed-trust/src/mcp/handlers.rs:219:    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review.md:3520:typed-trust/src/mcp/handlers.rs:221:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review.md:3521:typed-trust/src/mcp/handlers.rs:464:    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review.md:3522:typed-trust/src/mcp/handlers.rs:465:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review.md:3523:typed-trust/src/mcp/handlers.rs:488:    _last_verified_path: Option<&str>,
./EVIDENT_AGENT_PHASE3.code-review.md:3524:typed-trust/src/mcp/tools.rs:43:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_AGENT_PHASE3.code-review.md:3525:typed-trust/src/mcp/tools.rs:50:                "last_verified_sidecar": {"type": "string", "description": "Optional last_verified.json sidecar path"}
./EVIDENT_AGENT_PHASE3.code-review.md:3526:typed-trust/src/mcp/tools.rs:161:                "last_verified_sidecar": {"type": "string"},
./EVIDENT_AGENT_PHASE3.code-review.md:3527:typed-trust/src/translate.rs:23://! - Translating `last_verified` into a [`Rerun`].
./EVIDENT_AGENT_PHASE3.code-review.md:3528:typed-trust/src/translate.rs:57:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./EVIDENT_AGENT_PHASE3.code-review.md:3529:typed-trust/src/translate.rs:72:    pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_AGENT_PHASE3.code-review.md:3530:typed-trust/src/translate.rs:373:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./EVIDENT_AGENT_PHASE3.code-review.md:3531:typed-trust/src/translate.rs:375:///   criterion id (shipping convention: `last_verified.value` is the
./EVIDENT_AGENT_PHASE3.code-review.md:3532:typed-trust/src/translate.rs:396:    let reruns = translate_last_verified(
./EVIDENT_AGENT_PHASE3.code-review.md:3533:typed-trust/src/translate.rs:397:        mc.last_verified.as_ref(),
./EVIDENT_AGENT_PHASE3.code-review.md:3534:typed-trust/src/translate.rs:434:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./EVIDENT_AGENT_PHASE3.code-review.md:3535:typed-trust/src/translate.rs:436:/// - `last_verified` is absent;
./EVIDENT_AGENT_PHASE3.code-review.md:3536:typed-trust/src/translate.rs:437:/// - `last_verified.date` is null (replay loop hasn't run);
./EVIDENT_AGENT_PHASE3.code-review.md:3537:typed-trust/src/translate.rs:438:/// - `last_verified.value` is null (no primary observation).
./EVIDENT_AGENT_PHASE3.code-review.md:3538:typed-trust/src/translate.rs:443:fn translate_last_verified(
./EVIDENT_AGENT_PHASE3.code-review.md:3539:typed-trust/src/translate.rs:469:        // Shipping convention: a populated last_verified records a
./EVIDENT_AGENT_PHASE3.code-review.md:3540:typed-trust/src/synthesize.rs:30:///   `last_verified`).
./EVIDENT_AGENT_PHASE3.code-review.md:3541:typed-trust/src/main.rs:80:    // Overlay sidecar entries onto each claim's last_verified field
./EVIDENT_AGENT_PHASE3.code-review.md:3542:typed-trust/src/main.rs:88:                        cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review.md:3543:typed-trust/src/main.rs:363:    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./EVIDENT_AGENT_PHASE3.code-review.md:3544:typed-trust/src/main.rs:578:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./EVIDENT_AGENT_PHASE3.code-review.md:3557:   578	/// The shape matches `workflow/evident.py`'s `last_verified.json`
./EVIDENT_AGENT_PHASE3.code-review.md:3676:   697	        let bc_criteria = translate_tolerances(bc)
./EVIDENT_AGENT_PHASE3.code-review.md:3728:   488	    _last_verified_path: Option<&str>,
./EVIDENT_AGENT_PHASE3.code-review.md:3744:   504	    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_AGENT_PHASE3.code-review.md:3795:   555	                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
./EVIDENT_AGENT_PHASE3.code-review.md:3811:   219	    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review.md:3813:   221	    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review.md:3834:- [P2] Honor last_verified_sidecar in synthesis — /scratch/TMAlign/evident/typed-trust/src/mcp/handlers.rs:483-489
./EVIDENT_AGENT_PHASE3.code-review.md:3835:  When `read_report` or `render_report` is called with `last_verified_sidecar`, the argument is passed here but then ignored, so the MCP result never overlays `last_verified.json` into the manifest claims. This makes MCP reports disagree with the CLI for any corpus that relies on sidecar verification data, because `translate_evidence` only sees the original `claim.last_verified` fields.
./EVIDENT_AGENT_PHASE3.code-review.md:3846:- [P2] Honor last_verified_sidecar in synthesis — /scratch/TMAlign/evident/typed-trust/src/mcp/handlers.rs:483-489
./EVIDENT_AGENT_PHASE3.code-review.md:3847:  When `read_report` or `render_report` is called with `last_verified_sidecar`, the argument is passed here but then ignored, so the MCP result never overlays `last_verified.json` into the manifest claims. This makes MCP reports disagree with the CLI for any corpus that relies on sidecar verification data, because `translate_evidence` only sees the original `claim.last_verified` fields.
./EVIDENT_PR5B.code-review.md:14:Code-review this PR as an independent engineer. Context: EVIDENT adds a new typed claim kind  to typed-trust (Rust) plus a deterministic Python walker (extract_metadata) that reads pyproject.toml / Cargo.toml / package.json and emits claims. The two claim kinds (measurement vs metadata_compatibility) are disjoint at the schema level — each rejects the other's fields. NO model call for metadata — declarative; the value declared IS the evidence.
./EVIDENT_PR5B.code-review.md:20:1) SCHEMA DISJOINTNESS — measurement claim with metadata block → MeasurementClaimCarriesMetadata. metadata claim with tolerances → MetadataClaimCarriesTolerances. What about subtler intersections: metadata claim with explicit ? Or measurement claim with  (explicit null vs absent)? The pre-PR5b serde used Option, so absent and null both become None; that's probably fine. But verify.
./EVIDENT_PR5B.code-review.md:24:3) MCP SURFACE — list_claims surfaces the claim id + tier + kind. For a metadata claim, the 'kind' field becomes 'metadata_compatibility' (string). Does any existing MCP consumer rely on kind being 'measurement'? If so, metadata claims would break their assumptions.
./EVIDENT_PR5B.code-review.md:80:+    root and emits `kind: metadata_compatibility` claims. No model
./EVIDENT_PR5B.code-review.md:144:+package.json) and emits ``kind: metadata_compatibility`` claims.
./EVIDENT_PR5B.code-review.md:183:+    """One emitted metadata_compatibility claim ready for manifest
./EVIDENT_PR5B.code-review.md:524:+    Output shape: one ``kind: metadata_compatibility`` claim per
./EVIDENT_PR5B.code-review.md:527:+    No ``tolerances`` or ``evidence.command`` — the declaration IS
./EVIDENT_PR5B.code-review.md:546:+                "kind": "metadata_compatibility",
./EVIDENT_PR5B.code-review.md:649:+  provenance + metadata block (no tolerances/evidence.command)
./EVIDENT_PR5B.code-review.md:784:+        assert c["kind"] == "metadata_compatibility"
./EVIDENT_PR5B.code-review.md:787:+        # Critical: no tolerances, no evidence.command — those
./EVIDENT_PR5B.code-review.md:790:+        assert "tolerances" not in c
./EVIDENT_PR5B.code-review.md:887:+        c["kind"] == "metadata_compatibility" for c in manifest["claims"]
./EVIDENT_PR5B.code-review.md:893:@@ -29,6 +29,12 @@ pub enum ClaimKind {
./EVIDENT_PR5B.code-review.md:911:     pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_PR5B.code-review.md:914:+    /// PR5b: required when ``kind == "metadata_compatibility"``.
./EVIDENT_PR5B.code-review.md:923:+/// PR5b: structured block for ``kind: metadata_compatibility``
./EVIDENT_PR5B.code-review.md:951:+    /// PR5b: `kind: metadata_compatibility` claim missing the
./EVIDENT_PR5B.code-review.md:954:+    /// PR5b: metadata claims must NOT carry tolerances; they're
./EVIDENT_PR5B.code-review.md:968:+                "claim {id}: kind=metadata_compatibility requires a \
./EVIDENT_PR5B.code-review.md:974:+                "claim {id}: kind=metadata_compatibility must NOT carry \
./EVIDENT_PR5B.code-review.md:975:+                 tolerances; metadata is declarative, not empirical"
./EVIDENT_PR5B.code-review.md:980:+                 block; metadata belongs only to metadata_compatibility claims"
./EVIDENT_PR5B.code-review.md:992:+    // metadata_compatibility claims (PR5b — declarative
./EVIDENT_PR5B.code-review.md:994:+    if mc.kind != "measurement" && mc.kind != "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:1001:+    // PR5b: metadata_compatibility claims require the `metadata`
./EVIDENT_PR5B.code-review.md:1003:+    // NOT carry tolerances/evidence — those belong to the empirical
./EVIDENT_PR5B.code-review.md:1005:+    if mc.kind == "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:1011:+        if mc.tolerances.is_some() {
./EVIDENT_PR5B.code-review.md:1024:+    let kind = if mc.kind == "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:1043:         last_verified: None,
./EVIDENT_PR5B.code-review.md:1060:+// PR5b: metadata_compatibility claim kind
./EVIDENT_PR5B.code-review.md:1069:+    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:1094:+    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:1112:+fn metadata_claim_with_tolerances_is_rejected() {
./EVIDENT_PR5B.code-review.md:1116:+    title: metadata claim with tolerances
./EVIDENT_PR5B.code-review.md:1117:+    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:1126:+    tolerances:
./EVIDENT_PR5B.code-review.md:1153:+    tolerances:
./EVIDENT_PR5B.code-review.md:1159:+      oracle: [Manual]
./EVIDENT_PR5B.code-review.md:1184:+    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:1196:+    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1209:+    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:1222:+    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1236:+    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:1298:/bin/bash -lc "rg -n \"pub struct ManifestClaim|translate_tolerances|translate_evidence|deny_unknown_fields|serde\\(\" typed-trust/src typed-trust/tests -g '*.rs'" in /scratch/TMAlign/evident
./EVIDENT_PR5B.code-review.md:1300:typed-trust/tests/human_render.rs:9:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_PR5B.code-review.md:1301:typed-trust/tests/human_render.rs:61:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1303:typed-trust/tests/synthesize_and_render.rs:13:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./EVIDENT_PR5B.code-review.md:1304:typed-trust/tests/synthesize_and_render.rs:102:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1306:typed-trust/tests/html_render.rs:7:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_PR5B.code-review.md:1307:typed-trust/tests/html_render.rs:59:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1309:typed-trust/tests/translator.rs:10:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./EVIDENT_PR5B.code-review.md:1310:typed-trust/tests/translator.rs:213:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:1311:typed-trust/tests/translator.rs:254:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:1312:typed-trust/tests/translator.rs:317:    let result = translate_tolerances(&manifest.claims[0]);
./EVIDENT_PR5B.code-review.md:1313:typed-trust/tests/translator.rs:354:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:1315:typed-trust/tests/translator.rs:405:    let result = translate_tolerances(&manifest.claims[0]);
./EVIDENT_PR5B.code-review.md:1316:typed-trust/tests/translator.rs:435:    let result = translate_tolerances(&manifest.claims[0]);
./EVIDENT_PR5B.code-review.md:1317:typed-trust/tests/translator.rs:466:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:1319:typed-trust/tests/translator.rs:495:    let result = translate_tolerances(&manifest.claims[0]);
./EVIDENT_PR5B.code-review.md:1320:typed-trust/tests/translator.rs:522:    let result = translate_tolerances(&manifest.claims[0]);
./EVIDENT_PR5B.code-review.md:1321:typed-trust/tests/translator.rs:553:    let result = translate_tolerances(&manifest.claims[0]);
./EVIDENT_PR5B.code-review.md:1322:typed-trust/tests/translator.rs:578:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1324:typed-trust/tests/translator.rs:617:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1326:typed-trust/tests/translator.rs:662:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1328:typed-trust/tests/translator.rs:729:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1330:typed-trust/tests/translator.rs:750:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1332:typed-trust/tests/translator.rs:790:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1334:typed-trust/tests/translator.rs:831:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1336:typed-trust/tests/translator.rs:870:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1338:typed-trust/tests/translator.rs:912:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1340:typed-trust/tests/translator.rs:956:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1342:typed-trust/tests/translator.rs:1014:        let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1347:typed-trust/tests/translator.rs:1355:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1349:typed-trust/tests/translator.rs:1512:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1350:typed-trust/tests/translator.rs:1538:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1374:typed-trust/src/main.rs:29:    translate_review_event, translate_tolerances, ManifestClaim, ManifestLastVerified,
./EVIDENT_PR5B.code-review.md:1375:typed-trust/src/main.rs:181:        let criteria = match translate_tolerances(mc) {
./EVIDENT_PR5B.code-review.md:1376:typed-trust/src/main.rs:184:                // All translate_tolerances errors at this point are
./EVIDENT_PR5B.code-review.md:1378:typed-trust/src/main.rs:734:        let bc_criteria = translate_tolerances(bc)
./EVIDENT_PR5B.code-review.md:1408:typed-trust/src/mcp/handlers.rs:24:    translate_tolerances, ManifestClaim, ManifestReviewEvent, ReviewEventSidecar,
./EVIDENT_PR5B.code-review.md:1409:typed-trust/src/mcp/handlers.rs:536:    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_PR5B.code-review.md:1411:typed-trust/src/mcp/handlers.rs:604:                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
./EVIDENT_PR5B.code-review.md:1427:typed-trust/src/translate.rs:539:pub fn translate_tolerances(
./EVIDENT_PR5B.code-review.md:1454:evident-agent/tests/test_extract_metadata.py:146:        assert c["kind"] == "metadata_compatibility"
./EVIDENT_PR5B.code-review.md:1456:evident-agent/tests/test_extract_metadata.py:249:        c["kind"] == "metadata_compatibility" for c in manifest["claims"]
./EVIDENT_PR5B.code-review.md:1462:typed-trust/tests/human_render.rs:61:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1574:typed-trust/tests/synthesize_and_render.rs:102:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1804:typed-trust/tests/html_render.rs:59:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1880:evident-agent/tests/test_extract_validator.py:94:    assert exc.value.kind == "comparator_bound_to_wrong_subject"
./EVIDENT_PR5B.code-review.md:1885:evident-agent/tests/test_extract_validator.py:195:    assert exc.value.kind == "missing_subject"
./EVIDENT_PR5B.code-review.md:1891:evident-agent/tests/test_extract_validator.py:492:    assert exc.value.kind == "comparator_bound_to_wrong_subject"
./EVIDENT_PR5B.code-review.md:1907:typed-trust/tests/translator.rs:190:    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./EVIDENT_PR5B.code-review.md:1909:typed-trust/tests/translator.rs:213:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:1915:typed-trust/tests/translator.rs:254:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:1926:typed-trust/tests/translator.rs:354:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:1940:typed-trust/tests/translator.rs:417:    // Codex round 5: kind: measurement requires non-empty tolerances per
./EVIDENT_PR5B.code-review.md:1947:typed-trust/tests/translator.rs:466:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:1952:typed-trust/tests/translator.rs:569:    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./EVIDENT_PR5B.code-review.md:1953:typed-trust/tests/translator.rs:578:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1957:typed-trust/tests/translator.rs:617:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1959:typed-trust/tests/translator.rs:662:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1962:typed-trust/tests/translator.rs:729:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1964:typed-trust/tests/translator.rs:750:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1967:typed-trust/tests/translator.rs:790:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1970:typed-trust/tests/translator.rs:831:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1973:typed-trust/tests/translator.rs:870:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1976:typed-trust/tests/translator.rs:912:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1979:typed-trust/tests/translator.rs:956:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:1982:typed-trust/tests/translator.rs:1014:        let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:2006:typed-trust/tests/translator.rs:1355:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:2013:typed-trust/tests/translator.rs:1376:// PR5b: metadata_compatibility claim kind
./EVIDENT_PR5B.code-review.md:2014:typed-trust/tests/translator.rs:1385:    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:2016:typed-trust/tests/translator.rs:1410:    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:2017:typed-trust/tests/translator.rs:1433:    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:2020:typed-trust/tests/translator.rs:1500:    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:2021:typed-trust/tests/translator.rs:1512:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:2024:typed-trust/tests/translator.rs:1525:    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:2025:typed-trust/tests/translator.rs:1538:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:2027:typed-trust/tests/translator.rs:1552:    kind: metadata_compatibility
./EVIDENT_PR5B.code-review.md:2107:typed-trust/tests/mcp_tools.rs:433:            "  - id: claim-{i}\n    kind: measurement\n    tier: ci\n    source: .\n    title: t\n    claim: c\n    tolerances:\n      - metric: m\n        op: \"<\"\n        value: 0.02\n        prose: x\n    evidence:\n      oracle: [Test]\n      command: \"true\"\n      artifact: out.json\n"
./EVIDENT_PR5B.code-review.md:2171:typed-trust/src/claim.rs:25:pub enum ClaimKind {
./EVIDENT_PR5B.code-review.md:2173:evident-agent/evident_agent/violation.py:73:      target's ``tolerances`` list (no inventing criteria).
./EVIDENT_PR5B.code-review.md:2183:evident-agent/evident_agent/cli.py:1094:    root and emits `kind: metadata_compatibility` claims. No model
./EVIDENT_PR5B.code-review.md:2276:typed-trust/src/main.rs:181:        let criteria = match translate_tolerances(mc) {
./EVIDENT_PR5B.code-review.md:2290:typed-trust/src/main.rs:734:        let bc_criteria = translate_tolerances(bc)
./EVIDENT_PR5B.code-review.md:2384:typed-trust/src/mcp/tools.rs:43:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_PR5B.code-review.md:2459:typed-trust/src/translate.rs:77:    /// PR5b: required when ``kind == "metadata_compatibility"``.
./EVIDENT_PR5B.code-review.md:2460:typed-trust/src/translate.rs:86:/// PR5b: structured block for ``kind: metadata_compatibility``
./EVIDENT_PR5B.code-review.md:2475:typed-trust/src/translate.rs:318:    /// A `kind: measurement` claim omitted `tolerances` or provided
./EVIDENT_PR5B.code-review.md:2480:typed-trust/src/translate.rs:344:    /// PR5b: `kind: metadata_compatibility` claim missing the
./EVIDENT_PR5B.code-review.md:2483:typed-trust/src/translate.rs:377:                "claim {id}: kind=measurement requires non-empty tolerances; \
./EVIDENT_PR5B.code-review.md:2484:typed-trust/src/translate.rs:378:                 add tolerances or change to kind: policy / reference"
./EVIDENT_PR5B.code-review.md:2487:typed-trust/src/translate.rs:402:                "claim {id}: kind=metadata_compatibility requires a \
./EVIDENT_PR5B.code-review.md:2488:typed-trust/src/translate.rs:408:                "claim {id}: kind=metadata_compatibility must NOT carry \
./EVIDENT_PR5B.code-review.md:2490:typed-trust/src/translate.rs:443:    if mc.kind != "measurement" && mc.kind != "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:2492:typed-trust/src/translate.rs:454:    if mc.kind == "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:2493:typed-trust/src/translate.rs:473:    let kind = if mc.kind == "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:2575:evident-agent/evident_agent/extract/metadata.py:4:package.json) and emits ``kind: metadata_compatibility`` claims.
./EVIDENT_PR5B.code-review.md:2576:evident-agent/evident_agent/extract/metadata.py:384:    Output shape: one ``kind: metadata_compatibility`` claim per
./EVIDENT_PR5B.code-review.md:2577:evident-agent/evident_agent/extract/metadata.py:406:                "kind": "metadata_compatibility",
./EVIDENT_PR5B.code-review.md:2598:typed-trust/src/mcp/handlers.rs:536:    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_PR5B.code-review.md:2602:typed-trust/src/mcp/handlers.rs:604:                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
./EVIDENT_PR5B.code-review.md:2624://! observed values against tolerances, applies the §8 rule for
./EVIDENT_PR5B.code-review.md:2647:///   `last_verified`).
./EVIDENT_PR5B.code-review.md:2757:    _last_verified_path: Option<&str>,
./EVIDENT_PR5B.code-review.md:2773:    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_PR5B.code-review.md:2841:                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
./EVIDENT_PR5B.code-review.md:3029:pub enum ClaimKind {
./EVIDENT_PR5B.code-review.md:3091:        let criteria = match translate_tolerances(mc) {
./EVIDENT_PR5B.code-review.md:3094:                // All translate_tolerances errors at this point are
./EVIDENT_PR5B.code-review.md:3157:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./EVIDENT_PR5B.code-review.md:3161:/// When the claim's `evidence.oracle` is a single entry, populate
./EVIDENT_PR5B.code-review.md:3162:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./EVIDENT_PR5B.code-review.md:3164:pub fn translate_tolerances(
./EVIDENT_PR5B.code-review.md:3167:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./EVIDENT_PR5B.code-review.md:3168:        if e.oracle.len() == 1 {
./EVIDENT_PR5B.code-review.md:3169:            Some(e.oracle[0].clone())
./EVIDENT_PR5B.code-review.md:3175:    let Some(ts) = mc.tolerances.as_ref() else {
./EVIDENT_PR5B.code-review.md:3176:        // Measurement claims require non-empty tolerances per
./EVIDENT_PR5B.code-review.md:3196:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./EVIDENT_PR5B.code-review.md:3208:    single_oracle: &Option<String>,
./EVIDENT_PR5B.code-review.md:3217:        // tolerances; allowing them to translate would let
./EVIDENT_PR5B.code-review.md:3234:            against: single_oracle.clone(),
./EVIDENT_PR5B.code-review.md:3247:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./EVIDENT_PR5B.code-review.md:3248:///   collapsed into a single Evidence; oracle identity per tolerance
./EVIDENT_PR5B.code-review.md:3250:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./EVIDENT_PR5B.code-review.md:3265:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./EVIDENT_PR5B.code-review.md:3267:///   criterion id (shipping convention: `last_verified.value` is the
./EVIDENT_PR5B.code-review.md:3289:    let reruns = translate_last_verified(
./EVIDENT_PR5B.code-review.md:3290:        mc.last_verified.as_ref(),
./EVIDENT_PR5B.code-review.md:3494:        let bc_criteria = translate_tolerances(bc)
./EVIDENT_PR5B.code-review.md:3526:typed-trust/tests/human_render.rs:9:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_PR5B.code-review.md:3530:typed-trust/tests/human_render.rs:61:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3552:typed-trust/src/main.rs:181:        let criteria = match translate_tolerances(mc) {
./EVIDENT_PR5B.code-review.md:3572:typed-trust/src/main.rs:734:        let bc_criteria = translate_tolerances(bc)
./EVIDENT_PR5B.code-review.md:3578:typed-trust/tests/html_render.rs:7:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_PR5B.code-review.md:3582:typed-trust/tests/html_render.rs:59:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3596:typed-trust/tests/translator.rs:10:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./EVIDENT_PR5B.code-review.md:3601:typed-trust/tests/translator.rs:213:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:3604:typed-trust/tests/translator.rs:222:    // entry in `evidence.oracle`.
./EVIDENT_PR5B.code-review.md:3606:typed-trust/tests/translator.rs:254:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:3617:typed-trust/tests/translator.rs:354:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:3636:typed-trust/tests/translator.rs:466:    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./EVIDENT_PR5B.code-review.md:3642:typed-trust/tests/translator.rs:578:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3648:typed-trust/tests/translator.rs:617:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3653:typed-trust/tests/translator.rs:662:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3659:typed-trust/tests/translator.rs:729:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3666:typed-trust/tests/translator.rs:750:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3673:typed-trust/tests/translator.rs:790:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3677:typed-trust/tests/translator.rs:831:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3681:typed-trust/tests/translator.rs:870:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3685:typed-trust/tests/translator.rs:912:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3689:typed-trust/tests/translator.rs:956:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3694:typed-trust/tests/translator.rs:1014:        let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3705:typed-trust/tests/translator.rs:1355:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3711:typed-trust/tests/translator.rs:1512:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3715:typed-trust/tests/translator.rs:1538:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:3761:typed-trust/tests/synthesize_and_render.rs:13:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./EVIDENT_PR5B.code-review.md:3764:typed-trust/tests/synthesize_and_render.rs:102:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PR5B.code-review.md:4060:typed-trust/tests/mcp_tools.rs:433:            "  - id: claim-{i}\n    kind: measurement\n    tier: ci\n    source: .\n    title: t\n    claim: c\n    tolerances:\n      - metric: m\n        op: \"<\"\n        value: 0.02\n        prose: x\n    evidence:\n      oracle: [Test]\n      command: \"true\"\n      artifact: out.json\n"
./EVIDENT_PR5B.code-review.md:4125:typed-trust/src/evidence.rs:37:///   `evidence.command`. Sidecar `last_verified` gets populated by
./EVIDENT_PR5B.code-review.md:4142:typed-trust/src/translate.rs:452:    // NOT carry tolerances/evidence — those belong to the empirical
./EVIDENT_PR5B.code-review.md:4145:typed-trust/src/translate.rs:536:/// When the claim's `evidence.oracle` is a single entry, populate
./EVIDENT_PR5B.code-review.md:4146:typed-trust/src/translate.rs:542:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./EVIDENT_PR5B.code-review.md:4149:typed-trust/src/translate.rs:622:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./EVIDENT_PR5B.code-review.md:4164:typed-trust/src/translate.rs:875:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./EVIDENT_PR5B.code-review.md:4207:typed-trust/src/mcp/handlers.rs:536:    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_PR5B.code-review.md:4209:typed-trust/src/mcp/handlers.rs:604:                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
./EVIDENT_PR5B.code-review.md:4228:typed-trust/src/mcp/tools.rs:43:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_PR5B.code-review.md:4506:   181	        let criteria = match translate_tolerances(mc) {
./EVIDENT_PR5B.code-review.md:4509:   184	                // All translate_tolerances errors at this point are
./EVIDENT_PR5B.code-review.md:4651:   406	                "kind": "metadata_compatibility",
./EVIDENT_PR5B.code-review.md:4682:    71	    pub tolerances: Option<Vec<ManifestTolerance>>,
./EVIDENT_PR5B.code-review.md:4685:    74	    pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_PR5B.code-review.md:4688:    77	    /// PR5b: required when ``kind == "metadata_compatibility"``.
./EVIDENT_PR5B.code-review.md:4697:    86	/// PR5b: structured block for ``kind: metadata_compatibility``
./EVIDENT_PR5B.code-review.md:4711:   441	    // metadata_compatibility claims (PR5b — declarative
./EVIDENT_PR5B.code-review.md:4713:   443	    if mc.kind != "measurement" && mc.kind != "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:4720:   450	    // PR5b: metadata_compatibility claims require the `metadata`
./EVIDENT_PR5B.code-review.md:4722:   452	    // NOT carry tolerances/evidence — those belong to the empirical
./EVIDENT_PR5B.code-review.md:4724:   454	    if mc.kind == "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:4730:   460	        if mc.tolerances.is_some() {
./EVIDENT_PR5B.code-review.md:4737:   539	pub fn translate_tolerances(
./EVIDENT_PR5B.code-review.md:4740:   542	    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./EVIDENT_PR5B.code-review.md:4741:   543	        if e.oracle.len() == 1 {
./EVIDENT_PR5B.code-review.md:4742:   544	            Some(e.oracle[0].clone())
./EVIDENT_PR5B.code-review.md:4748:   550	    let Some(ts) = mc.tolerances.as_ref() else {
./EVIDENT_PR5B.code-review.md:4749:   551	        // Measurement claims require non-empty tolerances per
./EVIDENT_PR5B.code-review.md:4875:   536	    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_PR5B.code-review.md:4882:One stronger schema issue surfaced: the new translator comment says metadata claims must not carry `evidence`, but the implementation only rejects `tolerances`. That means a metadata claim can still include an empirical `evidence.command` block and pass through, which weakens the stated disjointness.
./EVIDENT_PR5B.code-review.md:4886:   454	    if mc.kind == "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:4892:   460	        if mc.tolerances.is_some() {
./EVIDENT_PR5B.code-review.md:4905:   473	    let kind = if mc.kind == "metadata_compatibility" {
./EVIDENT_PR5B.code-review.md:4943:   664	    let reruns = translate_last_verified(
./EVIDENT_PR5B.code-review.md:4944:   665	        mc.last_verified.as_ref(),
./EVIDENT_PR5B.code-review.md:5050:    pub tolerances: Option<Vec<ManifestTolerance>>,
./EVIDENT_PR5B.code-review.md:5053:    pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_PR5B.code-review.md:5056:    /// PR5b: required when ``kind == "metadata_compatibility"``.
./EVIDENT_PR5B.code-review.md:5065:/// PR5b: structured block for ``kind: metadata_compatibility``
./EVIDENT_PR5B.code-review.md:5160:P2: `metadata_compatibility` still accepts an `evidence` block. The comment says metadata claims must not carry tolerances/evidence, but `translate_claim` only rejects `mc.tolerances.is_some()` and never rejects `mc.evidence.is_some()` ([typed-trust/src/translate.rs](/scratch/TMAlign/evident/typed-trust/src/translate.rs:454)). Then `translate_evidence` will happily build `Evidence` for non-measurement claims if the block is present ([typed-trust/src/translate.rs](/scratch/TMAlign/evident/typed-trust/src/translate.rs:644)). That breaks the stated schema disjointness: a metadata claim can carry `evidence.command`. Add `MetadataClaimCarriesEvidence` or fold evidence into the existing metadata rejection path, with a regression test.
./EVIDENT_PR5B.code-review.md:5168:P3: MCP `list_claims` now surfaces raw `kind`, so metadata rows will return `"metadata_compatibility"` ([handlers.rs](/scratch/TMAlign/evident/typed-trust/src/mcp/handlers.rs:191)). I don’t see in-repo MCP consumers assuming only `"measurement"`, and `render_report` should synthesize metadata because `translate_tolerances`/`translate_evidence` allow empty outputs. External consumers may still have hardcoded kind enums. Also, `replay_status: "not_attempted"` for metadata is misleading because replay is inapplicable, not merely unattempted.
./EVIDENT_PR5B.code-review.md:5172:Absent vs explicit `null`: yes, `Option` means absent and `null` both become `None`. So `tolerances: null` on metadata is accepted like absent, and `metadata: null` on measurement is accepted like absent. That seems consistent with the pre-PR5b behavior you called out.
./EVIDENT_PR5B.code-review.md:5182:What’s right: the Rust block uses `deny_unknown_fields`, measurement-with-metadata is rejected, metadata-with-tolerances is rejected, Python emits no tolerances/evidence for generated metadata, and the walker is deterministic for a single repo/output.
./EVIDENT_PR5B.code-review.md:5187:P2: `metadata_compatibility` still accepts an `evidence` block. The comment says metadata claims must not carry tolerances/evidence, but `translate_claim` only rejects `mc.tolerances.is_some()` and never rejects `mc.evidence.is_some()` ([typed-trust/src/translate.rs](/scratch/TMAlign/evident/typed-trust/src/translate.rs:454)). Then `translate_evidence` will happily build `Evidence` for non-measurement claims if the block is present ([typed-trust/src/translate.rs](/scratch/TMAlign/evident/typed-trust/src/translate.rs:644)). That breaks the stated schema disjointness: a metadata claim can carry `evidence.command`. Add `MetadataClaimCarriesEvidence` or fold evidence into the existing metadata rejection path, with a regression test.
./EVIDENT_PR5B.code-review.md:5195:P3: MCP `list_claims` now surfaces raw `kind`, so metadata rows will return `"metadata_compatibility"` ([handlers.rs](/scratch/TMAlign/evident/typed-trust/src/mcp/handlers.rs:191)). I don’t see in-repo MCP consumers assuming only `"measurement"`, and `render_report` should synthesize metadata because `translate_tolerances`/`translate_evidence` allow empty outputs. External consumers may still have hardcoded kind enums. Also, `replay_status: "not_attempted"` for metadata is misleading because replay is inapplicable, not merely unattempted.
./EVIDENT_PR5B.code-review.md:5199:Absent vs explicit `null`: yes, `Option` means absent and `null` both become `None`. So `tolerances: null` on metadata is accepted like absent, and `metadata: null` on measurement is accepted like absent. That seems consistent with the pre-PR5b behavior you called out.
./EVIDENT_PR5B.code-review.md:5209:What’s right: the Rust block uses `deny_unknown_fields`, measurement-with-metadata is rejected, metadata-with-tolerances is rejected, Python emits no tolerances/evidence for generated metadata, and the walker is deterministic for a single repo/output.
./LICENSE:10:furnished to do so, subject to the following conditions:
./EVIDENT_DESIGN_v0.6_DRAFT.md:210:enum ClaimKind {
./viewer/src/pages/index.astro:8:const oracles = uniqueSorted(allClaims.flatMap((c) => c.evidence?.oracle ?? []));
./viewer/src/pages/index.astro:24:    ...(c.evidence?.oracle ?? []),
./viewer/src/pages/index.astro:26:    ...(c.tolerances?.flatMap((t) => [t.metric, t.prose]) ?? []),
./viewer/src/pages/index.astro:46:      placeholder="Search id, title, oracles, claim text, assumptions, failure modes…"
./viewer/src/pages/index.astro:78:      {oracles.length > 0 && (
./viewer/src/pages/index.astro:80:          {oracles.map((o) => (
./viewer/src/pages/index.astro:81:            <label><input type="checkbox" data-filter="oracle" value={o} checked />{o}</label>
./viewer/src/pages/index.astro:96:        <th data-sort-key="oracles" class="sortable">Oracles</th>
./viewer/src/pages/index.astro:108:          data-oracles={(c.evidence?.oracle ?? []).join(',')}
./viewer/src/pages/index.astro:117:          <td>{(c.evidence?.oracle ?? []).join(', ')}</td>
./viewer/src/pages/index.astro:156:          const oracles = (row.dataset.oracles || '').split(',').filter(Boolean);
./viewer/src/pages/index.astro:165:          if (visible && sel.oracle && oracles.length > 0) {
./viewer/src/pages/index.astro:166:            const hit = oracles.some((o) => sel.oracle.has(o));
./viewer/src/pages/index.astro:187:        if (key === 'oracles') return row.dataset.oracles || '';
./viewer/src/pages/claims/[id].astro:19:const lv = claim.last_verified;
./viewer/src/pages/claims/[id].astro:42:    {claim.tolerances?.length ? (
./viewer/src/pages/claims/[id].astro:45:        <ul class="tolerances">
./viewer/src/pages/claims/[id].astro:46:          {claim.tolerances.map((t) => (
./viewer/src/pages/claims/[id].astro:117:        <dd>{claim.evidence.oracle.join(', ')}</dd>
./viewer/src/pages/composer.astro:8:const oracles = uniqueSorted(measurement.flatMap((c) => c.evidence?.oracle ?? []));
./viewer/src/pages/composer.astro:11:const tolMetrics = uniqueSorted(measurement.flatMap((c) => c.tolerances?.map((t) => t.metric).filter(Boolean) as string[] ?? []));
./viewer/src/pages/composer.astro:21:  evidence: { oracle: c.evidence?.oracle ?? [] },
./viewer/src/pages/composer.astro:22:  tolerances: c.tolerances ?? [],
./viewer/src/pages/composer.astro:59:    {oracles.length > 0 && (
./viewer/src/pages/composer.astro:62:        <div class="chips" data-field="oracle">
./viewer/src/pages/composer.astro:63:          {oracles.map((o) => (
./viewer/src/pages/composer.astro:177:      const oracles = collect('oracle');
./viewer/src/pages/composer.astro:197:        oracles: oracles.length ? oracles : undefined,
./viewer/src/pages/composer.astro:216:      setBoxes('oracle', profile.oracles);
./viewer/src/pages/composer.astro:248:          <div class="meta">${tier} · ${sub} · ${escape(c.project)} · oracles: ${escape((c.evidence?.oracle ?? []).join(', '))}</div>
./EVIDENT_PHASE5.codex-review.md:17:- The framework's claim schema requires structured tolerances (metric, op, value), evidence (oracle, command, artifact), tier (research|ci|release).
./EVIDENT_PHASE5.codex-review.md:162:    tolerances:
./EVIDENT_PHASE5.codex-review.md:173:      oracle: [Paper-Authority]
./EVIDENT_PHASE5.codex-review.md:180:    last_verified:
./EVIDENT_PHASE5.codex-review.md:300:   draft to populate last_verified.value where the cited evidence
./EVIDENT_PHASE5.codex-review.md:320:> exactly one claim with `tolerances[0].metric == 'rmsd'`,
./EVIDENT_PHASE5.codex-review.md:347:  manifest's `tolerances[].prose` references a section visible
./EVIDENT_PHASE5.codex-review.md:397:   as `echo "no-replay-path"` and `last_verified.value: null`.
./concepts/typed-trust-json-shape.md:161:        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./concepts/typed-trust-json-shape.md:295:| Cross-project query | Rollup + report-level `status` + criteria tolerances | Full prose, rationales, `_graph` |
./EVIDENT_WALKTHROUGH_LADDER.code-review.md:341:+    for i, t in enumerate(claim.get("tolerances") or []):
./EVIDENT_WALKTHROUGH_LADDER.code-review.md:839:+                "tolerances": [
./EVIDENT_WALKTHROUGH_LADDER.code-review.md:848:+                    "oracle": ["Paper-Authority"],
./EVIDENT_WALKTHROUGH_LADDER.code-review.md:872:+                "tolerances": [
./EVIDENT_WALKTHROUGH_LADDER.code-review.md:881:+                    "oracle": ["Paper-Authority"],
./EVIDENT_WALKTHROUGH_LADDER.code-review.md:905:+                "tolerances": [
./EVIDENT_WALKTHROUGH_LADDER.code-review.md:914:+                    "oracle": ["Paper-Authority"],
./EVIDENT_WALKTHROUGH_LADDER.code-review.md:1683:    for i, t in enumerate(claim.get("tolerances") or []):
./latex/evident-paper/README.md:30:external oracles, tolerances, reproducible commands, and visible gaps.
./latex/evident-paper/main.tex:89:would falsify it. Each claim is linked to a trust strategy, oracle or
./latex/evident-paper/main.tex:100:oracles and tolerances, while release-grade claims also expose unresolved
./latex/evident-paper/main.tex:158:generate both implementation and tests. Independent oracles, reference
./latex/evident-paper/main.tex:172:and documentation of the oracles and references that shaped the final system.
./latex/evident-paper/main.tex:184:        oracles, tolerances, commands, artifacts, assumptions, and failure modes.
./latex/evident-paper/main.tex:216:appropriate oracle, a justified decision rule, a replay command, a preserved
./latex/evident-paper/main.tex:287:external oracles, replayable artifacts, and explicit failure modes become more
./latex/evident-paper/main.tex:304:claim -> trust strategy -> oracle/reference \\
./latex/evident-paper/main.tex:315:        against an oracle, reference, property, or benchmark.
./latex/evident-paper/main.tex:385:tier, a subsystem, inputs, outputs, pinned versions, tolerances, evidence
./latex/evident-paper/main.tex:387:machine-queryable: claims can be filtered by subsystem, oracle, tier,
./latex/evident-paper/main.tex:400:\texttt{oracle} & External tool, reference implementation, or benchmark \\
./latex/evident-paper/main.tex:401:\texttt{tolerances} & Decision rule for accepting or rejecting evidence \\
./latex/evident-paper/main.tex:418:        or oracle coverage.
./latex/evident-paper/main.tex:420:        claims should pin the project version, oracle versions, input corpus,
./latex/evident-paper/main.tex:424:        claims whose oracles are still incomplete.
./latex/evident-paper/main.tex:438:  \item Does the oracle or reference actually bear on the claim?
./latex/evident-paper/main.tex:471:Command & \texttt{python validation/welford\_oracle.py}. \\
./latex/evident-paper/main.tex:472:Artifact & \texttt{validation/artifacts/welford\_oracle.json}. \\
./latex/evident-paper/main.tex:483:oracle is named; the tolerance is explicit; the command can be replayed; the
./latex/evident-paper/main.tex:503:  oracle: [NumPy]
./latex/evident-paper/main.tex:504:  command: python validation/welford_oracle.py
./latex/evident-paper/main.tex:505:  artifact: validation/artifacts/welford_oracle.json
./latex/evident-paper/main.tex:506:tolerances:
./latex/evident-paper/main.tex:572:  \item sequence/search-related behavior against MMseqs2-style oracles
./latex/evident-paper/main.tex:576:Each claim should state the oracle, tolerance, command, artifact, assumptions,
./latex/evident-paper/main.tex:577:and failure modes. The important point is not that any one oracle proves
./latex/evident-paper/main.tex:578:correctness. The important point is that each claim says what the oracle can and
./latex/evident-paper/main.tex:583:Proteon illustrates why tolerances must be first-class evidence, not informal
./latex/evident-paper/main.tex:599:Proteon also shows a common practical split. Some oracles are cheap enough to
./latex/evident-paper/main.tex:614:scientifically useful because they show where a claim was too broad, an oracle
./latex/evident-paper/main.tex:632:  \item some oracle versions are placeholders awaiting release pinning;
./latex/evident-paper/main.tex:633:  \item some tests skip when heavy oracles are not installed;
./latex/evident-paper/main.tex:647:by CI oracles. Some release claims are supported by larger validation runs but
./latex/evident-paper/main.tex:659:  \item Is the oracle meaningful?
./latex/evident-paper/main.tex:675:unit tests, benchmark scripts, Docker files, oracle comparisons, frozen
./latex/evident-paper/main.tex:689:claim without an oracle is incomplete, a benchmark without a decision rule is
./latex/evident-paper/main.tex:706:detail. They can, however, inspect the claims being made, the oracles used, the
./latex/evident-paper/main.tex:713:record an oracle, but it cannot guarantee that the oracle is correct. It can
./latex/evident-paper/main.tex:719:oracles drift, corpora change, or commands stop replaying. A stale claim is
./latex/evident-paper/main.tex:726:so prematurely. Different domains need different oracles, tolerances, and
./concepts/ai-assisted-coding.md:11:**EVIDENT's claim-with-tolerances pattern is a forcing function. The
./concepts/ai-assisted-coding.md:50:  `last_verified.{commit, date, value, corpus_sha}` block is the
./concepts/ai-assisted-coding.md:78:tolerances are LLM-equally-easy to write — the schema constrains the
./concepts/ai-assisted-coding.md:91:Without a structured place for "we don't have an oracle for this
./concepts/ai-assisted-coding.md:142:Today every claim has `evidence.command` but `last_verified` is
./concepts/ai-assisted-coding.md:150:- writes back `last_verified.{date, commit, value, corpus_sha}`,
./concepts/ai-assisted-coding.md:153:Once `last_verified` is live, claims stop being aspirational text
./concepts/ai-assisted-coding.md:162:evident draft --from-test tests/oracle/test_x.py --tier ci
./concepts/ai-assisted-coding.md:223:  `last_verified` says the cited assertion held; it does not say the
./concepts/ai-assisted-coding.md:226:  draft a claim file from a test. Whether the tolerance, oracle,
./concepts/ai-assisted-coding.md:239:- Should `last_verified` be a sidecar (one JSON file keyed by claim
./concepts/ai-assisted-coding.md:261:swallowed by claim rot if `last_verified` is null.**
./EVIDENT_AGENT_PHASE2C.code-review.md:117:     last_verified_sidecar_path: Optional[Path],
./EVIDENT_AGENT_PHASE2C.code-review.md:152:             last_verified_sidecar_path = candidate
./EVIDENT_AGENT_PHASE2C.code-review.md:414:             sidecar_path=last_verified_sidecar_path,
./EVIDENT_AGENT_PHASE2C.code-review.md:1384:            sidecar_path=last_verified_sidecar_path,
./EVIDENT_AGENT_PHASE2C.code-review.md:1402:    Precedence: sidecar entry > inline manifest last_verified.commit
./EVIDENT_AGENT_PHASE2C.code-review.md:1409:    inline = (claim_raw.get("last_verified") or {}).get("commit")
./EVIDENT_AGENT_PHASE2C.code-review.md:1416:    missing tolerances."""
./EVIDENT_AGENT_PHASE2C.code-review.md:1417:    tols = claim_raw.get("tolerances") or []
./EVIDENT_AGENT_PHASE2C.code-review.md:1766:        let criteria = match translate_tolerances(mc) {
./EVIDENT_AGENT_PHASE2C.code-review.md:1769:                // All translate_tolerances errors at this point are
./EVIDENT_AGENT_PHASE2C.code-review.md:1860:Mirrors Phase 1's ``sidecar.py`` (last_verified) but with two extra
./EVIDENT_AGENT_PHASE2C.code-review.md:2293:@@ -151,6 +151,93 @@ def test_last_verified_commit_reaches_the_digest_header(tmp_path: Path) -> None:
./EVIDENT_AGENT_PHASE2C.code-review.md:2651:     """Two tolerances share metric=relative_error; the violation against
./EVIDENT_AGENT_PHASE2C.code-review.md:2769:     last_verified_sidecar_path: Optional[Path],
./EVIDENT_AGENT_PHASE2C.code-review.md:2804:             last_verified_sidecar_path = candidate
./EVIDENT_AGENT_PHASE2C.code-review.md:3132:    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./EVIDENT_AGENT_PHASE2C.code-review.md:3170:        sidecar_path = manifest_path.parent / "last_verified.json"
./EVIDENT_AGENT_PHASE2_DRAFT.md:6:`last_verified.json` by running the cited verification command in
./EVIDENT_AGENT_PHASE2_DRAFT.md:40:1. Agent reads claim YAML + rendered evidence + last_verified.
./EVIDENT_AGENT_PHASE2_DRAFT.md:73:evidence and last_verified, and emits either:
./EVIDENT_AGENT_PHASE2_DRAFT.md:263:  last_verified, *all* criteria — multi-criterion claims must reveal
./EVIDENT_AGENT_PHASE2_DRAFT.md:431:   b. Read claim's last_verified (from sidecar if present, else
./typed-trust-codex-review-7.md:238:+enum ClaimKind {
./typed-trust-codex-review-7.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-7.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-7.md:368:+                                        // tolerances
./typed-trust-codex-review-7.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-7.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-7.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-7.md:614:+pub enum ClaimKind {
./typed-trust-codex-review-7.md:1225:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-7.md:1292:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-7.md:1534:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-7.md:2030:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-7.md:2053:+///   `last_verified`).
./typed-trust-codex-review-7.md:2654:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-7.md:2655:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-7.md:2656:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-7.md:2664:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-7.md:2698:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-7.md:2710:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-7.md:2713:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-7.md:2745:+    pub oracle: Vec<String>,
./typed-trust-codex-review-7.md:2766:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-7.md:2775:+    /// release claims must carry structured tolerances.
./typed-trust-codex-review-7.md:2777:+    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-7.md:2779:+    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-7.md:2808:+                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-7.md:2809:+                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-7.md:2896:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-7.md:2900:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-7.md:2901:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-7.md:2903:+pub fn translate_tolerances(
./typed-trust-codex-review-7.md:2906:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-7.md:2907:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-7.md:2908:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-7.md:2914:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-7.md:2915:+        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-7.md:2935:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-7.md:2947:+    single_oracle: &Option<String>,
./typed-trust-codex-review-7.md:2956:+        // tolerances; allowing them to translate would let
./typed-trust-codex-review-7.md:2973:+            against: single_oracle.clone(),
./typed-trust-codex-review-7.md:2986:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-7.md:2987:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-7.md:2989:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-7.md:3004:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-7.md:3006:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-7.md:3016:+    let reruns = translate_last_verified(
./typed-trust-codex-review-7.md:3017:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-7.md:3054:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-7.md:3056:+/// - `last_verified` is absent;
./typed-trust-codex-review-7.md:3057:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-7.md:3058:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-7.md:3063:+fn translate_last_verified(
./typed-trust-codex-review-7.md:3089:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-7.md:3156:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-7.md:3157:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-7.md:3164:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-7.md:3431:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-7.md:3593:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-7.md:3611:+    tolerances:
./typed-trust-codex-review-7.md:3619:+      oracle:
./typed-trust-codex-review-7.md:3624:+    last_verified:
./typed-trust-codex-review-7.md:3645:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-7.md:3912:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-7.md:3934:+    // populated to the oracle name.
./typed-trust-codex-review-7.md:3980:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-7.md:4088:+               within tier-specific tolerances."
./typed-trust-codex-review-7.md:4100:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-7.md:4147:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-7.md:4164:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-7.md:4175:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-7.md:4259:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-7.md:4265:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-7.md:4375:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-7.md:4381:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-7.md:4382:+/// translation working) with last_verified populated.
./typed-trust-codex-review-7.md:4396:+    tolerances:
./typed-trust-codex-review-7.md:4404:+      oracle:
./typed-trust-codex-review-7.md:4409:+    last_verified:
./typed-trust-codex-review-7.md:4418:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-7.md:4432:+    tolerances:
./typed-trust-codex-review-7.md:4439:+      oracle:
./typed-trust-codex-review-7.md:4444:+    last_verified:
./typed-trust-codex-review-7.md:4464:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-7.md:4512:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-7.md:4514:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-7.md:4515:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-7.md:5620:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-7.md:5625:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-7.md:5626:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-7.md:5644:+    tolerances:
./typed-trust-codex-review-7.md:5652:+      oracle:
./typed-trust-codex-review-7.md:5657:+    last_verified:
./typed-trust-codex-review-7.md:5665:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-7.md:5669:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-7.md:5670:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-7.md:5671:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-7.md:5687:+    tolerances:
./typed-trust-codex-review-7.md:5695:+      oracle:
./typed-trust-codex-review-7.md:5700:+    last_verified:
./typed-trust-codex-review-7.md:5714:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-7.md:5715:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-7.md:5725:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-7.md:5727:+      oracle:
./typed-trust-codex-review-7.md:5729:+      command: pytest tests/oracle -v
./typed-trust-codex-review-7.md:5753:+    tolerances:
./typed-trust-codex-review-7.md:5773:+      oracle:
./typed-trust-codex-review-7.md:5775:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-7.md:5800:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-7.md:5821:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-7.md:5823:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-7.md:5831:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-7.md:5832:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-7.md:5837:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-7.md:5852:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-7.md:5860:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-7.md:5864:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-7.md:5886:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-7.md:5916:+    tolerances:
./typed-trust-codex-review-7.md:5922:+      oracle: [SomeOracle]
./typed-trust-codex-review-7.md:5927:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-7.md:5953:+    tolerances:
./typed-trust-codex-review-7.md:5959:+      oracle: [internal]
./typed-trust-codex-review-7.md:5960:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-7.md:5964:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-7.md:6006:+    tolerances:
./typed-trust-codex-review-7.md:6010:+      oracle: [Foo]
./typed-trust-codex-review-7.md:6011:+      command: pytest tests/oracle
./typed-trust-codex-review-7.md:6015:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-7.md:6026:+fn rejects_measurement_claim_without_tolerances() {
./typed-trust-codex-review-7.md:6027:+    // Codex round 5: kind: measurement requires non-empty tolerances per
./typed-trust-codex-review-7.md:6032:+  - id: measurement-no-tolerances
./typed-trust-codex-review-7.md:6033:+    title: missing tolerances
./typed-trust-codex-review-7.md:6040:+      oracle: [Foo]
./typed-trust-codex-review-7.md:6045:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-7.md:6048:+            assert_eq!(id, "measurement-no-tolerances");
./typed-trust-codex-review-7.md:6055:+fn rejects_measurement_claim_with_empty_tolerances_list() {
./typed-trust-codex-review-7.md:6058:+  - id: measurement-empty-tolerances
./typed-trust-codex-review-7.md:6059:+    title: empty tolerances
./typed-trust-codex-review-7.md:6065:+    tolerances: []
./typed-trust-codex-review-7.md:6067:+      oracle: [Foo]
./typed-trust-codex-review-7.md:6072:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-7.md:6090:+    tolerances:
./typed-trust-codex-review-7.md:6094:+      oracle: [Foo]
./typed-trust-codex-review-7.md:6095:+      command: pytest tests/oracle
./typed-trust-codex-review-7.md:6099:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-7.md:6120:+    tolerances:
./typed-trust-codex-review-7.md:6125:+      oracle: [Foo]
./typed-trust-codex-review-7.md:6130:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-7.md:6146:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-7.md:6149:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-7.md:6152:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-7.md:6155:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-7.md:6162:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-7.md:6191:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-7.md:6194:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-7.md:6211:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-7.md:6225:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-7.md:6239:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-7.md:6255: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-7.md:6261: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-7.md:6272:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-7.md:6294:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-7.md:6373://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-7.md:6374://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-7.md:6375://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-7.md:6383://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-7.md:6417:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-7.md:6429:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-7.md:6432:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-7.md:6464:    pub oracle: Vec<String>,
./typed-trust-codex-review-7.md:6485:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-7.md:6494:    /// release claims must carry structured tolerances.
./typed-trust-codex-review-7.md:6496:    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-7.md:6498:    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-7.md:6527:                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-7.md:6528:                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-7.md:6607://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-7.md:6630:///   `last_verified`).
./typed-trust-codex-review-7.md:6860:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-7.md:6864:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-7.md:6865:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-7.md:6867:pub fn translate_tolerances(
./typed-trust-codex-review-7.md:6870:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-7.md:6871:        if e.oracle.len() == 1 {
./typed-trust-codex-review-7.md:6872:            Some(e.oracle[0].clone())
./typed-trust-codex-review-7.md:6878:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-7.md:6879:        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-7.md:6899:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-7.md:6911:    single_oracle: &Option<String>,
./typed-trust-codex-review-7.md:6920:        // tolerances; allowing them to translate would let
./typed-trust-codex-review-7.md:6937:            against: single_oracle.clone(),
./typed-trust-codex-review-7.md:6950:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-7.md:6951:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-7.md:6953:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-7.md:6968:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-7.md:6970:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-7.md:6980:    let reruns = translate_last_verified(
./typed-trust-codex-review-7.md:6981:        mc.last_verified.as_ref(),
./typed-trust-codex-review-7.md:7018:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-7.md:7020:/// - `last_verified` is absent;
./typed-trust-codex-review-7.md:7021:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-7.md:7022:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-7.md:7027:fn translate_last_verified(
./typed-trust-codex-review-7.md:7053:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-7.md:7120:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-7.md:7121:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-7.md:7454:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-7.md:7553:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-7.md:7620:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-7.md:8782:pub enum ClaimKind {
./typed-trust-codex-review-7.md:8805:/bin/bash -lc "grep -n \"evidence\\|tolerances\\|tier\" -n workflow/SCHEMA.md | head -80" in /scratch/TMAlign/evident
./typed-trust-codex-review-7.md:8812:89:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-7.md:8813:90:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-7.md:8814:99:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-7.md:8815:111:  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review-7.md:8818:120:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-7.md:8819:129:tolerances:
./typed-trust-codex-review-7.md:8822:168:Each named entry can be referenced by `tolerances[].output` so a single
./typed-trust-codex-review-7.md:8823:177:  Biopython: "1.83"              # one entry per oracle named in evidence.oracle
./typed-trust-codex-review-7.md:8824:181:The validator requires that every name in `evidence.oracle` appears as a
./typed-trust-codex-review-7.md:8825:199:underlying numbers are correct (that is what `tolerances` and
./typed-trust-codex-review-7.md:8828:219:replace the underlying `tolerances` and `evidence` fields.
./typed-trust-codex-review-7.md:8830:267:- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-7.md:8833:307:1. Lift `evidence.tolerance` into one or more `tolerances:` entries.
./typed-trust-codex-review-7.md:8834:312:   measurements (and they may then drop `subsystem` and `tolerances`).
./typed-trust-codex-review-7.md:8857:| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
./typed-trust-codex-review-7.md:8859:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-7.md:8860:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-7.md:8863:| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-7.md:8869:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-7.md:8898:   `subsystem`, `oracle`, and `capability` are declared per-manifest;
./typed-trust-codex-review-7.md:8917:  oracle:       [...]
./typed-trust-codex-review-7.md:8939:| `oracle`           | empty — consumer must define                                       |
./typed-trust-codex-review-7.md:8985:    95	        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-7.md:9005:   332	            against: single_oracle.clone(),
./typed-trust-codex-review-7.md:9018:   345	/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-7.md:9019:   346	///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-7.md:9021:   348	///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-7.md:9036:   363	/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-7.md:9038:   365	///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-7.md:9048:   375	    let reruns = translate_last_verified(
./typed-trust-codex-review-7.md:9049:   376	        mc.last_verified.as_ref(),
./cases/proteon.md:31:The strongest evidence is that Proteon treats oracle validation as a release
./cases/proteon.md:34:- `docs/ORACLE_SETUP.md` gives pinned oracle versions and reproducibility
./cases/proteon.md:36:- `tests/oracle/README.md` lists current oracles, what each covers, and how
./cases/proteon.md:39:  between oracle tests and frozen fixtures.
./cases/proteon.md:42:  oracles.
./cases/proteon.md:51:- Heavy oracles may be skipped in fast CI, provided release-quality validation
./cases/proteon.md:56:- Agreement with one oracle can hide a shared convention or modeling error.
./cases/proteon.md:57:- Golden fixtures can drift if they are not tied back to live oracle
./cases/proteon.md:59:- Release claims can outgrow CI if heavy oracle checks are not run before tags.
./cases/proteon.md:66:oracle, a tolerance, an installation recipe, and diagnostic failure output. The
./cases/proteon.md:88:- Release readiness should be tied to oracle-backed claims, not only test
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:14:Review v3 of this DESIGN DRAFT for a new EVIDENT claim kind ('behavioral_concordance').
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:20:- Dropped evidence.oracle for concordance claims (pattern_kind IS the oracle)
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:31:5. The artifact_schema_version + last_concorded.json sidecar pair — anything load-bearing about their interaction with last_verified.json?
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:36:# Design — `kind: behavioral_concordance` (v3 DRAFT)
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:49:> - **Concordance claims do NOT carry `evidence.oracle`.** The
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:50:>   pattern_kind IS the oracle; the `[BALL, FreeSASA, …]` shape
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:68:>   existing `last_verified` shape (v1 falsely claimed they were
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:90:>   `last_verified.json`. `last_verified.value` (scalar) cannot
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:108:paper's **own** subject. The validator correctly rejected six of
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:109:the seven as `comparator_bound_to_wrong_subject`. The one that
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:127:  kind: behavioral_concordance
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:158:    # Concordance claims do NOT carry `oracle` — the comparator
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:159:    # primitive IS the oracle. They DO carry the docker contract.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:186:5. **No `evidence.oracle`** — the pattern_kind is the oracle.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:359:  kind: behavioral_concordance
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:378:versioned contract**, distinct from `last_verified.json`.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:421:`last_verified.json` is **not** extended. A separate
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:422:`last_concorded.json` carries concordance results:
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:448:Codex's reasoning, applied: `last_verified.value` is a scalar
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:491:  `nix-replay` oracle is a follow-up.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:517:  dispatch on `pattern_kind`, write `last_concorded.json`.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:519:- A new sidecar shape (`last_concorded.json`) that typed-trust
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:520:  reads alongside `last_verified.json`.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:526:  `measurement` / `metadata_compatibility` paths changes.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:617:   enum ConcordancePattern {
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:650:7. **`artifact_schema_version` and `last_concorded.json` need a clearer boundary.**  
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:651:   `artifact_schema_version` belongs to the docker-produced artifact contract. `last_concorded.json` is framework output. Do not imply `last_concorded` inherits or mirrors the artifact schema version.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:653:   Interaction with `last_verified.json`: the load-bearing issue is aggregation. Typed-trust now reads both sidecars, so claim status resolution must be keyed by `claim.kind`: measurement claims consult `last_verified`, concordance claims consult `last_concorded`. A missing `last_concorded` for a concordance claim should not be confused with missing replay for measurement. Also guard against duplicate claim IDs across sidecars; one ID should have one kind and one result source.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:657:v3 is directionally solid. I would keep `paper_locator`, defer `third_party_observation`, and implement pattern-typed priors as variant-specific Rust structs validated after `pattern_kind` dispatch. The main fixes needed before implementation are schema consistency around `metric_path`, realistic rustims success criteria, and explicit typed-trust behavior when `last_verified.json` and `last_concorded.json` coexist.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:678:   enum ConcordancePattern {
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:711:7. **`artifact_schema_version` and `last_concorded.json` need a clearer boundary.**  
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:712:   `artifact_schema_version` belongs to the docker-produced artifact contract. `last_concorded.json` is framework output. Do not imply `last_concorded` inherits or mirrors the artifact schema version.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:714:   Interaction with `last_verified.json`: the load-bearing issue is aggregation. Typed-trust now reads both sidecars, so claim status resolution must be keyed by `claim.kind`: measurement claims consult `last_verified`, concordance claims consult `last_concorded`. A missing `last_concorded` for a concordance claim should not be confused with missing replay for measurement. Also guard against duplicate claim IDs across sidecars; one ID should have one kind and one result source.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review-v3.md:718:v3 is directionally solid. I would keep `paper_locator`, defer `third_party_observation`, and implement pattern-typed priors as variant-specific Rust structs validated after `pattern_kind` dispatch. The main fixes needed before implementation are schema consistency around `metric_path`, realistic rustims success criteria, and explicit typed-trust behavior when `last_verified.json` and `last_concorded.json` coexist.
./README.md:28:  -> oracle/reference
./README.md:118:- Tighten an oracle or tolerance
./concepts/README.md:42:4. Multi-oracle — agrees with references that disagree with each other
./concepts/README.md:104:An oracle is an external reference used to validate behavior.
./concepts/README.md:120:- audit trail — oracle version, fixture commit, environment, run
./cases/README.md:8:- the oracle, tolerance, command, and artifact
./cases/README.md:16:- [Proteon](proteon.md) - oracle-backed release readiness for a structural
./cases/cu-ims-primitives.md:14:known ground truth, real data has no perfect oracle, and GPU acceleration adds
./cases/cu-ims-primitives.md:24:- Real-data proxy oracles: mature tools such as DiaTracer, DiaNN, FragPipe,
./cases/cu-ims-primitives.md:27:  by primitive, with tiered tolerances.
./cases/cu-ims-primitives.md:75:floating-point tolerances, some are algorithmic substitutions, and some can
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:12:   oracle pattern described in `cases/proteon.md`.
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:34:    oracle: [TimSim, timsseek, DiaTracer, DiaNN, FragPipe]
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:149:  tolerances:
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:156:    oracle: [Biopython]
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:165:  last_verified:
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:202:                    from oracle-test README.",
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:239:                    id: ProtocolId("proteon-oracle-review-v1"),
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:240:                    name: "Proteon oracle-test review guideline".into(),
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:262:                command: "git log --follow tests/oracle/test_sasa_biopython.py",
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:313:The shipping manifest's `last_verified` carries `{commit, date, value,
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:333:`last_verified` natively expressible.
./EVIDENT_DESIGN_v0.4_FIT_TEST.md:370:The shipping manifest's `tolerances: [{metric, op, value, prose}]` is
./EVIDENT_AGENT_PHASE2.code-review.md:69:+    "last_verified_sidecar_path",
./EVIDENT_AGENT_PHASE2.code-review.md:73:+        "Path to a last_verified.json sidecar to overlay onto each claim "
./EVIDENT_AGENT_PHASE2.code-review.md:74:+        "before producing the digest. Default: manifest.parent / 'last_verified.json' if it exists."
./EVIDENT_AGENT_PHASE2.code-review.md:100:+    last_verified_sidecar_path: Optional[Path],
./EVIDENT_AGENT_PHASE2.code-review.md:118:+    if last_verified_sidecar_path is None:
./EVIDENT_AGENT_PHASE2.code-review.md:119:+        candidate = manifest_path.parent / "last_verified.json"
./EVIDENT_AGENT_PHASE2.code-review.md:121:+            last_verified_sidecar_path = candidate
./EVIDENT_AGENT_PHASE2.code-review.md:220:+            sidecar_path=last_verified_sidecar_path,
./EVIDENT_AGENT_PHASE2.code-review.md:235:+    missing tolerances."""
./EVIDENT_AGENT_PHASE2.code-review.md:236:+    tols = claim_raw.get("tolerances") or []
./EVIDENT_AGENT_PHASE2.code-review.md:248:+    tols = claim_raw.get("tolerances") or []
./EVIDENT_AGENT_PHASE2.code-review.md:258:+    The model sees the full structured claim (tier, all tolerances,
./EVIDENT_AGENT_PHASE2.code-review.md:259:+    evidence pointer, last_verified) — multi-criterion claims must
./EVIDENT_AGENT_PHASE2.code-review.md:1263:+Mirrors Phase 1's ``sidecar.py`` (last_verified) but with two extra
./EVIDENT_AGENT_PHASE2.code-review.md:1590:+                tolerances:
./EVIDENT_AGENT_PHASE2.code-review.md:1596:+                  oracle: [Test]
./EVIDENT_AGENT_PHASE2.code-review.md:1670:+                tolerances:
./EVIDENT_AGENT_PHASE2.code-review.md:2069:+                tolerances:
./EVIDENT_AGENT_PHASE2.code-review.md:2075:+                  oracle: [Test]
./EVIDENT_AGENT_PHASE2.code-review.md:2130:+        "tolerances": [
./EVIDENT_AGENT_PHASE2.code-review.md:2145:+            "oracle": ["Test"],
./EVIDENT_AGENT_PHASE2.code-review.md:2831:+        "    tolerances:\n"
./EVIDENT_AGENT_PHASE2.code-review.md:2837:+        "      oracle: [Test]\n"
./EVIDENT_AGENT_PHASE2.code-review.md:3129:-    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./EVIDENT_AGENT_PHASE2.code-review.md:3132:+    translate_tolerances, ManifestClaim, ManifestLastVerified, ReviewEventSidecar,
./EVIDENT_AGENT_PHASE2.code-review.md:3914: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./EVIDENT_AGENT_PHASE2.code-review.md:3920: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./EVIDENT_AGENT_PHASE2.code-review.md:3931:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./EVIDENT_AGENT_PHASE2.code-review.md:3953:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./EVIDENT_AGENT_PHASE2.code-review.md:4155:    translate_tolerances, ManifestClaim, ManifestLastVerified, ReviewEventSidecar,
./EVIDENT_AGENT_PHASE2.code-review.md:4205:    // Overlay sidecar entries onto each claim's last_verified field
./EVIDENT_AGENT_PHASE2.code-review.md:4213:                        cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE2.code-review.md:4281:        let criteria = match translate_tolerances(mc) {
./EVIDENT_AGENT_PHASE2.code-review.md:4284:                // All translate_tolerances errors at this point are
./EVIDENT_AGENT_PHASE2.code-review.md:4466:    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./EVIDENT_AGENT_PHASE2.code-review.md:4718:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./EVIDENT_AGENT_PHASE2.code-review.md:5218:Mirrors Phase 1's ``sidecar.py`` (last_verified) but with two extra
./EVIDENT_AGENT_PHASE2.code-review.md:6516:    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./EVIDENT_AGENT_PHASE2.code-review.md:6554:        sidecar_path = manifest_path.parent / "last_verified.json"
./EVIDENT_AGENT_PHASE2.code-review.md:6693:    "last_verified_sidecar_path",
./EVIDENT_AGENT_PHASE2.code-review.md:6700:"""Sidecar ``last_verified.json`` read/write.
./EVIDENT_AGENT_PHASE2.code-review.md:7643://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./EVIDENT_AGENT_PHASE2.code-review.md:7644://!   values, populating `against` from a single-oracle heuristic
./EVIDENT_AGENT_PHASE2.code-review.md:7645://!   (multi-oracle disambiguation needs schema work or convention).
./EVIDENT_AGENT_PHASE2.code-review.md:7653://! - Translating `last_verified` into a [`Rerun`].
./EVIDENT_AGENT_PHASE2.code-review.md:7687:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./EVIDENT_AGENT_PHASE2.code-review.md:7699:    pub tolerances: Option<Vec<ManifestTolerance>>,
./EVIDENT_AGENT_PHASE2.code-review.md:7702:    pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_AGENT_PHASE2.code-review.md:7734:    pub oracle: Vec<String>,
./EVIDENT_AGENT_PHASE2.code-review.md:7755:    /// An unknown comparison operator in `tolerances[].op`.
./EVIDENT_AGENT_PHASE2.code-review.md:7764:    /// release claims must carry structured tolerances.
./EVIDENT_AGENT_PHASE2.code-review.md:7766:    /// A `kind: measurement` claim omitted `tolerances` or provided
./EVIDENT_AGENT_PHASE2.code-review.md:7768:    /// tolerances on measurement claims; without them the
./EVIDENT_AGENT_PHASE2.code-review.md:7802:                "claim {id}: kind=measurement requires non-empty tolerances; \
./EVIDENT_AGENT_PHASE2.code-review.md:7803:                 add tolerances or change to kind: policy / reference"
./EVIDENT_AGENT_PHASE2.code-review.md:7895:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./EVIDENT_AGENT_PHASE2.code-review.md:7899:/// When the claim's `evidence.oracle` is a single entry, populate
./EVIDENT_AGENT_PHASE2.code-review.md:7900:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./EVIDENT_AGENT_PHASE2.code-review.md:7902:pub fn translate_tolerances(
./EVIDENT_AGENT_PHASE2.code-review.md:7905:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./EVIDENT_AGENT_PHASE2.code-review.md:7906:        if e.oracle.len() == 1 {
./EVIDENT_AGENT_PHASE2.code-review.md:7907:            Some(e.oracle[0].clone())
./EVIDENT_AGENT_PHASE2.code-review.md:7913:    let Some(ts) = mc.tolerances.as_ref() else {
./EVIDENT_AGENT_PHASE2.code-review.md:7914:        // Measurement claims require non-empty tolerances per
./EVIDENT_AGENT_PHASE2.code-review.md:7934:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./EVIDENT_AGENT_PHASE2.code-review.md:7946:    single_oracle: &Option<String>,
./EVIDENT_AGENT_PHASE2.code-review.md:7955:        // tolerances; allowing them to translate would let
./EVIDENT_AGENT_PHASE2.code-review.md:7972:            against: single_oracle.clone(),
./EVIDENT_AGENT_PHASE2.code-review.md:7985:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./EVIDENT_AGENT_PHASE2.code-review.md:7986:///   collapsed into a single Evidence; oracle identity per tolerance
./EVIDENT_AGENT_PHASE2.code-review.md:7988:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./EVIDENT_AGENT_PHASE2.code-review.md:8008:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./EVIDENT_AGENT_PHASE2.code-review.md:8010:///   criterion id (shipping convention: `last_verified.value` is the
./EVIDENT_AGENT_PHASE2.code-review.md:8031:    let reruns = translate_last_verified(
./EVIDENT_AGENT_PHASE2.code-review.md:8032:        mc.last_verified.as_ref(),
./EVIDENT_AGENT_PHASE2.code-review.md:8069:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./EVIDENT_AGENT_PHASE2.code-review.md:8071:/// - `last_verified` is absent;
./EVIDENT_AGENT_PHASE2.code-review.md:8072:/// - `last_verified.date` is null (replay loop hasn't run);
./EVIDENT_AGENT_PHASE2.code-review.md:8073:/// - `last_verified.value` is null (no primary observation).
./EVIDENT_AGENT_PHASE2.code-review.md:8078:fn translate_last_verified(
./EVIDENT_AGENT_PHASE2.code-review.md:8104:        // Shipping convention: a populated last_verified records a
./EVIDENT_AGENT_PHASE2.code-review.md:8171:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./EVIDENT_AGENT_PHASE2.code-review.md:8172:///   (most measurement claims compare against an oracle).
./EVIDENT_AGENT_PHASE2.code-review.md:8179:        .is_some_and(|e| !e.oracle.is_empty())
./EVIDENT_AGENT_PHASE2.code-review.md:8691:   279	    last_verified_sidecar_path: Optional[Path],
./EVIDENT_AGENT_PHASE2.code-review.md:8709:   297	    if last_verified_sidecar_path is None:
./EVIDENT_AGENT_PHASE2.code-review.md:8710:   298	        candidate = manifest_path.parent / "last_verified.json"
./EVIDENT_AGENT_PHASE2.code-review.md:8712:   300	            last_verified_sidecar_path = candidate
./EVIDENT_AGENT_PHASE2.code-review.md:8900:                tolerances:
./EVIDENT_AGENT_PHASE2.code-review.md:8906:                  oracle: [Test]
./EVIDENT_AGENT_PHASE2.code-review.md:8980:                tolerances:
./EVIDENT_AGENT_PHASE2.code-review.md:9028:- [P2] Ensure review prompts consume last_verified sidecar — /scratch/TMAlign/evident/evident-agent/evident_agent/cli.py:330-334
./EVIDENT_AGENT_PHASE2.code-review.md:9029:  When a `last_verified.json` exists or `--last-verified-sidecar` is supplied, this path is only forwarded to the optional render step and is never read before building the digest/prompt. As a result, even claims with recorded verification metadata reach the model with no overlaid `last_verified` and no `commit` in the digest header, so the required reproducible-chain check cannot be verified and endorsements are forced to dissent or to hallucinate a pass.
./EVIDENT_AGENT_PHASE2.code-review.md:9040:- [P2] Ensure review prompts consume last_verified sidecar — /scratch/TMAlign/evident/evident-agent/evident_agent/cli.py:330-334
./EVIDENT_AGENT_PHASE2.code-review.md:9041:  When a `last_verified.json` exists or `--last-verified-sidecar` is supplied, this path is only forwarded to the optional render step and is never read before building the digest/prompt. As a result, even claims with recorded verification metadata reach the model with no overlaid `last_verified` and no `commit` in the digest header, so the required reproducible-chain check cannot be verified and endorsements are forced to dissent or to hallucinate a pass.
./EVIDENT_DESIGN_v0.4_DRAFT.md:27:| Calibration subject | model identity | judge identity (vague) | `(Principal, Protocol)` pair |
./EVIDENT_DESIGN_v0.4_DRAFT.md:193:enum ClaimKind {
./EVIDENT_DESIGN_v0.4_DRAFT.md:395:one calibration subject; the same model under a different rubric is a
./EVIDENT_DESIGN_v0.4_DRAFT.md:396:different subject. A human reviewer panel under one journal's guidelines is
./EVIDENT_DESIGN_v0.4_DRAFT.md:403:its calibration subject explicitly. The honest framing is that
./EVIDENT_DESIGN_v0.4_DRAFT.md:434:calibration subject undefined when None. Should the spec forbid None at
./concepts/not-just-a-unit-test.md:48:`last_verified: {commit, date, corpus_sha}` survives a year.
./concepts/not-just-a-unit-test.md:62:`kind: reference` documents "we do not have an oracle for X yet." A
./concepts/not-just-a-unit-test.md:72:operator, the value, the oracle, and the corpus. Unit tests are happy
./concepts/not-just-a-unit-test.md:112:  oracles.
./concepts/not-just-a-unit-test.md:123:tolerances, "agrees with X" assertions that nobody can audit.
./concepts/not-just-a-unit-test.md:142:  rules and rationale, including why structured tolerances are not
./EVIDENT_AGENT_PHASE2B_DRAFT.md:47:`last_verified.value = V` that satisfies the inverted comparator. The
./EVIDENT_AGENT_PHASE2B_DRAFT.md:152:          "tolerances": [
./EVIDENT_AGENT_PHASE2B_DRAFT.md:161:            "oracle": ["BALL"],
./EVIDENT_AGENT_PHASE2B_DRAFT.md:165:          "last_verified": { "value": 0.025 }
./EVIDENT_AGENT_PHASE2B_DRAFT.md:229:    `translate_claim` / `translate_tolerances` / `translate_evidence`
./EVIDENT_AGENT_PHASE2B_DRAFT.md:287:    > target claim's stated tolerances. Report the violation as
./EVIDENT_AGENT_PHASE2B_DRAFT.md:311:    target's bound; `last_verified.value` is the model's
./EVIDENT_AGENT_PHASE2B_DRAFT.md:445:   tolerances using its own metric, bound, and comparator.
./EVIDENT_AGENT_PHASE2B_DRAFT.md:457:> claim's stated tolerances. Report the violation as
./EVIDENT_AGENT_PHASE2B_DRAFT.md:566:   tolerances with `op: "="` on floats are brittle. Recommend
./EVIDENT_PHASE5_PR2.codex-review.md:65:     pub tolerances: Option<Vec<ManifestTolerance>>,
./EVIDENT_PHASE5_PR2.codex-review.md:69:     pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_PHASE5_PR2.codex-review.md:208:     let reruns = translate_last_verified(
./EVIDENT_PHASE5_PR2.codex-review.md:209:         mc.last_verified.as_ref(),
./EVIDENT_PHASE5_PR2.codex-review.md:274:+    tolerances:
./EVIDENT_PHASE5_PR2.codex-review.md:280:+      oracle: [Biopython]
./EVIDENT_PHASE5_PR2.codex-review.md:290:+    tolerances:
./EVIDENT_PHASE5_PR2.codex-review.md:296:+      oracle: [Repo-README]
./EVIDENT_PHASE5_PR2.codex-review.md:400:+    tolerances:
./EVIDENT_PHASE5_PR2.codex-review.md:408:+      oracle: [Repo-README]
./EVIDENT_PHASE5_PR2.codex-review.md:452:+    tolerances:
./EVIDENT_PHASE5_PR2.codex-review.md:458:+      oracle: [Manual]
./EVIDENT_PHASE5_PR2.codex-review.md:486:+    tolerances:
./EVIDENT_PHASE5_PR2.codex-review.md:492:+      oracle: [Manual]
./EVIDENT_PHASE5_PR2.codex-review.md:503:+    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PHASE5_PR2.codex-review.md:533:+    tolerances:
./EVIDENT_PHASE5_PR2.codex-review.md:539:+      oracle: [Manual]
./EVIDENT_PHASE5_PR2.codex-review.md:550:+    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PHASE5_PR2.codex-review.md:577:typed-trust/tests/human_render.rs:9:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_PHASE5_PR2.codex-review.md:580:typed-trust/tests/translator.rs:10:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./EVIDENT_PHASE5_PR2.codex-review.md:627:typed-trust/tests/synthesize_and_render.rs:13:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./EVIDENT_PHASE5_PR2.codex-review.md:630:typed-trust/tests/html_render.rs:7:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_PHASE5_PR2.codex-review.md:660:typed-trust/src/main.rs:29:    translate_review_event, translate_tolerances, ManifestClaim, ManifestLastVerified,
./EVIDENT_PHASE5_PR2.codex-review.md:669:typed-trust/src/mcp/handlers.rs:24:    translate_tolerances, ManifestClaim, ManifestReviewEvent, ReviewEventSidecar,
./EVIDENT_PHASE5_PR2.codex-review.md:866:   548	    let reruns = translate_last_verified(
./EVIDENT_PHASE5_PR2.codex-review.md:867:   549	        mc.last_verified.as_ref(),
./EVIDENT_PHASE5_PR2.codex-review.md:1018:   415	/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./EVIDENT_PHASE5_PR2.codex-review.md:1022:   419	/// When the claim's `evidence.oracle` is a single entry, populate
./EVIDENT_PHASE5_PR2.codex-review.md:1023:   420	/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./EVIDENT_PHASE5_PR2.codex-review.md:1025:   422	pub fn translate_tolerances(
./EVIDENT_PHASE5_PR2.codex-review.md:1028:   425	    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./EVIDENT_PHASE5_PR2.codex-review.md:1029:   426	        if e.oracle.len() == 1 {
./EVIDENT_PHASE5_PR2.codex-review.md:1030:   427	            Some(e.oracle[0].clone())
./EVIDENT_PHASE5_PR2.codex-review.md:1036:   433	    let Some(ts) = mc.tolerances.as_ref() else {
./EVIDENT_PHASE5_PR2.codex-review.md:1037:   434	        // Measurement claims require non-empty tolerances per
./EVIDENT_PHASE5_PR2.codex-review.md:1057:   454	            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./EVIDENT_PHASE5_PR2.codex-review.md:1069:   466	    single_oracle: &Option<String>,
./EVIDENT_PHASE5_PR2.codex-review.md:1097:   779	/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./EVIDENT_PHASE5_PR2.codex-review.md:1098:   780	///   (most measurement claims compare against an oracle).
./EVIDENT_PHASE5_PR2.codex-review.md:1105:   787	        .is_some_and(|e| !e.oracle.is_empty())
./EVIDENT_PHASE5_PR2.codex-review.md:1249:   157	        let criteria = match translate_tolerances(mc) {
./EVIDENT_PHASE5_PR2.codex-review.md:1252:   160	                // All translate_tolerances errors at this point are
./EVIDENT_PHASE5_PR2.codex-review.md:1282:   520	    _last_verified_path: Option<&str>,
./EVIDENT_PHASE5_PR2.codex-review.md:1298:   536	    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_PHASE5_PR2.codex-review.md:1321:    71	    pub tolerances: Option<Vec<ManifestTolerance>>,
./EVIDENT_PHASE5_PR2.codex-review.md:1324:    74	    pub last_verified: Option<ManifestLastVerified>,
./workflow/evident.py:8:  replay      re-execute claims' evidence.command and update last_verified
./workflow/evident.py:46:        "oracles": list(evidence.get("oracle") or []),
./workflow/evident.py:53:def _filter(rows: list[dict], tier: str | None, oracle: str | None, id_sub: str | None) -> list[dict]:
./workflow/evident.py:60:        if oracle:
./workflow/evident.py:61:            needle = oracle.lower()
./workflow/evident.py:62:            if not any(needle in o.lower() for o in r["oracles"]):
./workflow/evident.py:76:    cols = ["id", "tier", "oracles", "title"]
./workflow/evident.py:77:    headers = {"id": "ID", "tier": "TIER", "oracles": "ORACLES", "title": "TITLE"}
./workflow/evident.py:78:    caps = {"id": 50, "tier": 10, "oracles": 35, "title": 60}
./workflow/evident.py:102:    print("id\ttier\toracles\ttitle\tcommand\tartifact")
./workflow/evident.py:107:            ",".join(r["oracles"]),
./workflow/evident.py:132:    rows = _filter(rows, args.tier, args.oracle, args.id)
./workflow/evident.py:135:    # `--stale DAYS`. Reads `last_verified.json` next to the manifest;
./workflow/evident.py:171:# regexes cover the patterns that show up in the existing oracle tests, and
./workflow/evident.py:174:_RE_PYTEST_MARK_ORACLE = re.compile(r'pytest\.mark\.oracle\(\s*["\']([^"\']+)["\']\s*\)')
./workflow/evident.py:183:      oracles: list[str]                  from pytest.mark.oracle("…") calls
./workflow/evident.py:192:    oracles = sorted(set(_RE_PYTEST_MARK_ORACLE.findall(text)))
./workflow/evident.py:210:        "oracles": oracles,
./workflow/evident.py:245:    oracles: list[str],
./workflow/evident.py:254:    has not yet filled in real values — vocabulary (subsystem, oracle),
./workflow/evident.py:255:    path existence (case), and the project-vs-oracle pinned-version
./workflow/evident.py:290:        for oracle in oracles or ["TODO-oracle"]:
./workflow/evident.py:291:            add(f"      {oracle}: PENDING-PIN")
./workflow/evident.py:299:        add("    tolerances:")
./workflow/evident.py:318:    add("      oracle:")
./workflow/evident.py:319:    for oracle in oracles or ["TODO-oracle"]:
./workflow/evident.py:320:        add(f"        - {oracle}")
./workflow/evident.py:327:    add("    last_verified:")
./workflow/evident.py:336:    add("        the claim. Assumptions about the oracle, the corpus, the engine")
./workflow/evident.py:342:    add("        passes. Compensating-component bugs, fixture drift, oracle drift,")
./workflow/evident.py:349:    hints: dict = {"oracles": [], "tolerance_bounds": [], "module_title": None, "command": ""}
./workflow/evident.py:358:    oracles = args.oracle or hints["oracles"] or []
./workflow/evident.py:376:        oracles=oracles,
./workflow/evident.py:393:# `replay` — re-execute claims' evidence.command and update last_verified
./workflow/evident.py:395:# Closes the loop the schema's last_verified.{date, commit, value, corpus_sha}
./workflow/evident.py:400:# Storage decision: a sidecar `last_verified.json` file next to the manifest,
./workflow/evident.py:417:# etc.) and is its own design. Until that lands, last_verified.value stays
./workflow/evident.py:423:SIDECAR_FILENAME = "last_verified.json"
./workflow/evident.py:453:    Used to fill `commit` in last_verified entries. Returning None when
./workflow/evident.py:614:        # last_verified.commit as the SOURCE SHA where the claim
./workflow/evident.py:695:    p_list.add_argument("--oracle", default=None, help="filter: oracle name substring")
./workflow/evident.py:704:            "Filter to claims whose sidecar last_verified.date is older than "
./workflow/evident.py:705:            "DAYS (or absent). Reads `last_verified.json` next to the "
./workflow/evident.py:713:        help="Re-execute claims' evidence.command and update last_verified",
./workflow/evident.py:717:            "write to a sidecar `last_verified.json`. The sidecar is the "
./workflow/evident.py:794:        "--oracle",
./workflow/evident.py:796:        help="Add an oracle name to evidence.oracle (repeatable)",
./workflow/evident.py:805:            "Pytest file to scrape for hints (oracle markers, tolerance "
./workflow/SCHEMA.md:23:   `subsystem`, `oracle`, and `capability` are declared per-manifest;
./workflow/SCHEMA.md:42:  oracle:       [...]
./workflow/SCHEMA.md:64:| `oracle`           | empty — consumer must define                                       |
./workflow/SCHEMA.md:87:| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
./workflow/SCHEMA.md:89:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./workflow/SCHEMA.md:90:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./workflow/SCHEMA.md:93:| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./workflow/SCHEMA.md:98:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./workflow/SCHEMA.md:109:- **Oracles**: `pinned_versions` must include every oracle named in
./workflow/SCHEMA.md:110:  `evidence.oracle` with the version used to produce the cited result
./workflow/SCHEMA.md:119:  `tolerances[].output` should reference an entry by name.
./workflow/SCHEMA.md:128:tolerances:
./workflow/SCHEMA.md:167:Each named entry can be referenced by `tolerances[].output` so a single
./workflow/SCHEMA.md:176:  Biopython: "1.83"              # one entry per oracle named in evidence.oracle
./workflow/SCHEMA.md:180:The validator requires that every name in `evidence.oracle` appears as a
./workflow/SCHEMA.md:198:underlying numbers are correct (that is what `tolerances` and
./workflow/SCHEMA.md:199:`last_verified` are for). The three levels:
./workflow/SCHEMA.md:201:- `automatic` — produced by a runner (CI, an oracle script, a benchmark).
./workflow/SCHEMA.md:218:replace the underlying `tolerances` and `evidence` fields.
./workflow/SCHEMA.md:223:last_verified:
./workflow/SCHEMA.md:233:typically held in a sidecar (`last_verified.json`) keyed by claim id so
./workflow/SCHEMA.md:243:AND oracle ∩ {OpenMM, BALL} != ∅
./workflow/SCHEMA.md:258:1. Lift `evidence.tolerance` into one or more `tolerances:` entries.
./workflow/SCHEMA.md:263:   measurements (and they may then drop `subsystem` and `tolerances`).
./EVIDENT_DESIGN_v0.5.codex-review.md:63:| Calibration subject | model identity | judge identity (vague) | `(Principal, Protocol)` pair |
./EVIDENT_DESIGN_v0.5.codex-review.md:229:enum ClaimKind {
./EVIDENT_DESIGN_v0.5.codex-review.md:431:one calibration subject; the same model under a different rubric is a
./EVIDENT_DESIGN_v0.5.codex-review.md:432:different subject. A human reviewer panel under one journal's guidelines is
./EVIDENT_DESIGN_v0.5.codex-review.md:439:its calibration subject explicitly. The honest framing is that
./EVIDENT_DESIGN_v0.5.codex-review.md:470:calibration subject undefined when None. Should the spec forbid None at
./EVIDENT_DESIGN_v0.5.codex-review.md:514:   oracle pattern described in `cases/proteon.md`.
./EVIDENT_DESIGN_v0.5.codex-review.md:536:    oracle: [TimSim, timsseek, DiaTracer, DiaNN, FragPipe]
./EVIDENT_DESIGN_v0.5.codex-review.md:651:  tolerances:
./EVIDENT_DESIGN_v0.5.codex-review.md:658:    oracle: [Biopython]
./EVIDENT_DESIGN_v0.5.codex-review.md:667:  last_verified:
./EVIDENT_DESIGN_v0.5.codex-review.md:704:                    from oracle-test README.",
./EVIDENT_DESIGN_v0.5.codex-review.md:741:                    id: ProtocolId("proteon-oracle-review-v1"),
./EVIDENT_DESIGN_v0.5.codex-review.md:742:                    name: "Proteon oracle-test review guideline".into(),
./EVIDENT_DESIGN_v0.5.codex-review.md:764:                command: "git log --follow tests/oracle/test_sasa_biopython.py",
./EVIDENT_DESIGN_v0.5.codex-review.md:815:The shipping manifest's `last_verified` carries `{commit, date, value,
./EVIDENT_DESIGN_v0.5.codex-review.md:835:`last_verified` natively expressible.
./EVIDENT_DESIGN_v0.5.codex-review.md:872:The shipping manifest's `tolerances: [{metric, op, value, prose}]` is
./EVIDENT_DESIGN_v0.5.codex-review.md:1105:The shipping `last_verified: {commit, date, value, corpus_sha}` maps to
./EVIDENT_DESIGN_v0.5.codex-review.md:1113:This is the change that makes `last_verified` natively expressible.
./EVIDENT_DESIGN_v0.5.codex-review.md:1170:enum ClaimKind {
./EVIDENT_DESIGN_v0.5.codex-review.md:1292:The relationship to the shipping manifest's `tolerances` list:
./EVIDENT_DESIGN_v0.5.codex-review.md:1296:| `tolerances[].metric` | `Criterion.tolerance.metric` |
./EVIDENT_DESIGN_v0.5.codex-review.md:1297:| `tolerances[].op` | `Criterion.tolerance.op` |
./EVIDENT_DESIGN_v0.5.codex-review.md:1298:| `tolerances[].value` | `Criterion.tolerance.value` |
./EVIDENT_DESIGN_v0.5.codex-review.md:1299:| `tolerances[].output` | `Criterion.tolerance.output` |
./EVIDENT_DESIGN_v0.5.codex-review.md:1300:| `tolerances[].prose` | `Criterion.tolerance.prose` |
./EVIDENT_DESIGN_v0.5.codex-review.md:1354:| `tolerances[].*` | `Criterion.tolerance: Tolerance` |
./EVIDENT_DESIGN_v0.5.codex-review.md:1355:| `evidence.{oracle, command, artifact}` | `Evidence { kind, locator, extraction: Verified }` |
./EVIDENT_DESIGN_v0.5.codex-review.md:1357:| `last_verified.*` | latest entry of `Verified.reproductions` |
./EVIDENT_DESIGN_v0.5.codex-review.md:1585:`evidence.oracle` -> `Evidence`:
./EVIDENT_DESIGN_v0.5.codex-review.md:1586:Where does oracle identity live? `ToolInvocation.env`? `EvidenceKind`? `Locator`? This matters for claims like “agrees with Biopython.”
./EVIDENT_DESIGN_v0.5.codex-review.md:1798:`evidence.oracle` -> `Evidence`:
./EVIDENT_DESIGN_v0.5.codex-review.md:1799:Where does oracle identity live? `ToolInvocation.env`? `EvidenceKind`? `Locator`? This matters for claims like “agrees with Biopython.”
./concepts/typed-trust-proteon-fit.md:10:1000-PDB, two oracles, peer-reviewable). Four other claim shapes
./concepts/typed-trust-proteon-fit.md:12:× 7 tolerances), DSSP classification (mixed equality and inequality
./concepts/typed-trust-proteon-fit.md:13:tolerances), cross-path parity (self-reference oracle), and the release
./concepts/typed-trust-proteon-fit.md:40:tolerances:
./concepts/typed-trust-proteon-fit.md:45:  oracle: [Biopython, FreeSASA]
./concepts/typed-trust-proteon-fit.md:49:last_verified:
./concepts/typed-trust-proteon-fit.md:68:        kind: ClaimKind::Comparison,            // multi-oracle parity
./concepts/typed-trust-proteon-fit.md:147:Two oracles, one shared artifact (validation/results.json contains
./concepts/typed-trust-proteon-fit.md:222:against *two different oracles* with *two different tolerances*.
./concepts/typed-trust-proteon-fit.md:223:Today, the YAML carries the oracle identity in `evidence.oracle` (a
./concepts/typed-trust-proteon-fit.md:224:list shared across all tolerances in the claim) and the prose carries
./concepts/typed-trust-proteon-fit.md:226:binding from a `Tolerance` to a specific oracle when multiple oracles
./concepts/typed-trust-proteon-fit.md:230:  to the oracle it's measured against.
./concepts/typed-trust-proteon-fit.md:231:- (b) Use distinct `output` names per oracle (e.g.,
./concepts/typed-trust-proteon-fit.md:238:Lean (a). The shipping `evidence.oracle` list already names oracles;
./concepts/typed-trust-proteon-fit.md:263:                match claim tolerances.",
./concepts/typed-trust-proteon-fit.md:322:Build the TrustReport. Three tolerances → three Criteria, each with a
./concepts/typed-trust-proteon-fit.md:374:### `forcefield_charmm19_ball.yaml` — 7 outputs × 7 tolerances
./concepts/typed-trust-proteon-fit.md:408:### `cross_path_parity.yaml` — self-reference oracle
./concepts/typed-trust-proteon-fit.md:410:This claim's oracle is `proteon-slow-path` — the project's own exact
./concepts/typed-trust-proteon-fit.md:411:O(N²) reference path, used as the oracle for the NBL fast path. The
./concepts/typed-trust-proteon-fit.md:417:attestation backed by a self-referential oracle. The framework doesn't
./concepts/typed-trust-proteon-fit.md:420:`vocabularies.oracle` list legitimizes `proteon-slow-path` as a named
./concepts/typed-trust-proteon-fit.md:421:oracle for composability.
./concepts/typed-trust-proteon-fit.md:428:references oracles ([OpenMM, BALL, USAlign, ...] — 8 of them) and a
./concepts/typed-trust-proteon-fit.md:429:command (`pytest tests/oracle -v`), but the assertion is normative
./concepts/typed-trust-proteon-fit.md:430:("Core claims should be release-gated by external oracles") rather
./concepts/typed-trust-proteon-fit.md:452:| F-PR3 | Multi-oracle claims have no typed binding from Tolerance to specific oracle | real gap | Add `Tolerance.against: Option<String>` (oracle name from manifest vocab) |
./concepts/typed-trust-proteon-fit.md:459:| F-PR9 | Self-reference oracles work via SupportRelation strength | confirm | none |
./concepts/typed-trust-proteon-fit.md:478:2. **Add `Tolerance.against: Option<String>`** for multi-oracle claims
./concepts/typed-trust-proteon-fit.md:479:   so each tolerance binds to the oracle name it's measured against.
./concepts/typed-trust-proteon-fit.md:480:   The string is project-vocab-extensible (matches `evidence.oracle`
./concepts/typed-trust-proteon-fit.md:481:   entries from the manifest's `vocabularies.oracle`).
./concepts/typed-trust-proteon-fit.md:505:oracle-test runner (discover) → existing peer-review attestation
./concepts/typed-trust-proteon-fit.md:518:The first walkthrough was an oracle-parity claim (proteon vs Biopython).
./concepts/typed-trust-proteon-fit.md:527:— 1000 PDBs, two implementations of AMBER96, three tolerances mixing
./concepts/typed-trust-proteon-fit.md:546:   `pass_rate` tolerances on `tm_score`:
./concepts/typed-trust-proteon-fit.md:564:   typed model handles this transparently: `Evidence { oracle: OpenMM,
./concepts/typed-trust-proteon-fit.md:607:| Peer-implementation "oracle" handled via SupportRelation strength + rationale | F-PR9 (reconfirmed) |
./concepts/typed-trust-proteon-fit.md:763:            command: "pytest tests/oracle/test_charmm19_3way_triangulation.py".into(),
./concepts/typed-trust-proteon-fit.md:832:            "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./EVIDENT_PHASE5_PR3.codex-review.md:378:+    tolerances:
./EVIDENT_PHASE5_PR3.codex-review.md:385:+      oracle: [Paper-Authority]
./EVIDENT_PHASE5_PR3.codex-review.md:409:+    tolerances:
./EVIDENT_PHASE5_PR3.codex-review.md:416:+      oracle: [Paper-Authority]
./EVIDENT_PHASE5_PR3.codex-review.md:544:+    tolerances:
./EVIDENT_PHASE5_PR3.codex-review.md:550:+      oracle: [Biopython]
./evident.yaml:14:  oracle:
./evident.yaml:36:      oracle:
./EVIDENT_MULTISTEP.code-review.md:533:+    tolerances:
./EVIDENT_MULTISTEP.code-review.md:540:+      oracle: [Paper-Authority]
./EVIDENT_PHASE5_PR5.codex-review-v2.md:28:5) FUTURE_TENSE_REPO — 'We will achieve <0.5s latency in v2'. The plan says 'rejection reason roadmap_claim' but is that a NEW rejection reason that needs to be added to PR4's enum? PR4's reason enum is: bound_not_stated, comparator_bound_to_wrong_subject, value_only_in_image_table, metric_not_named, ranking_language_only, hedged_qualitative_only, cited_external_artifact. None of those are 'roadmap_claim'. Either pick one of those or extend the enum.
./EVIDENT_PHASE5_PR5.codex-review-v2.md:30:6) CONFLICT_REPO — README says >1000, CHANGELOG says >5000. Plan says 'both extracted; conflict flagged in EXTRACTION.md.' But what if the curator wants to know WHICH was authoritative? Should the manifest emit two claims with different ids? One claim with two tolerances? Worth being concrete.
./EVIDENT_PHASE5_PR5.codex-review-v2.md:225:  the claim's `subject_aliases`.
./EVIDENT_PHASE5_PR5.codex-review-v2.md:228:- If a claim ends up with zero valid tolerances: drops the claim
./EVIDENT_PHASE5_PR5.codex-review-v2.md:232:emit anything, but only validator-approved tolerances reach the
./EVIDENT_PHASE5_PR5.codex-review-v2.md:293:- model-response processor moves invalid tolerances to rejections
./EVIDENT_PHASE5_PR5.codex-review-v2.md:295:- claim with zero valid tolerances is dropped
./EVIDENT_PHASE5_PR5.codex-review-v2.md:303:  (needs a `metadata_compatibility` claim kind that doesn't fit
./EVIDENT_PHASE5_PR5.codex-review-v2.md:416:- Emit two distinct claims with different ids and source spans, not one merged claim with multiple incompatible tolerances.
./EVIDENT_PHASE5_PR5.codex-review-v2.md:472:- Emit two distinct claims with different ids and source spans, not one merged claim with multiple incompatible tolerances.
./workflow/README.md:41:  claim. These may install large oracle tools or use larger datasets.
./workflow/README.md:54:- What oracle or reference is used?
./workflow/README.md:67:- `case-proteon`: Proteon plus release-grade structural-bioinformatics oracles.
./workflow/README.md:88:This checks manifest structure and local paths. It does not run domain oracles.
./workflow/GRAMMAR.md:42:of the claim from `subsystem`, `outputs`, `inputs`, `tolerances`, and
./workflow/GRAMMAR.md:76:Project-specific axes (`subsystem`, `oracle`, `capability`) are open,
./workflow/GRAMMAR.md:94:Research claims may carry prose-only tolerances and unspecified
./workflow/GRAMMAR.md:108:| `pinned_versions` covers source + oracles   | required   | required| required  |
./workflow/GRAMMAR.md:110:| `last_verified` populated                   | optional   | recommended | required |
./workflow/GRAMMAR.md:134:  is comparable to the recorded `last_verified.value`. If the command
./workflow/GRAMMAR.md:169:  `pinned_versions` and `evidence.oracle`, not free-form in
./workflow/GRAMMAR.md:188:  from `tolerances`+`last_verified`, conditioning from
./concepts/typed-trust-overview.md:192:contract. Given a claim, its criteria (from tolerances), its evidence,
./concepts/typed-trust-overview.md:209:- Measurement claims without tolerances (`MeasurementWithoutTolerances`).
./concepts/typed-trust-overview.md:211:- Prose-only tolerances outside research tier (`ProseOnlyOutsideResearch`).
./concepts/typed-trust-overview.md:254:- 1 policy claim (`proteon-oracle-backed-release-gate`) — explicitly
./concepts/typed-trust-overview.md:259:  measurement claims have prose-only tolerances at CI or release tier,
./concepts/typed-trust-overview.md:286:| 5 | 2 P2 | Schema completeness (missing tolerances) + cycle propagation to top-level |
./concepts/typed-trust-overview.md:316:- **Per-tolerance oracle binding for multi-oracle claims.** The
./concepts/typed-trust-overview.md:317:  shipping schema lists oracles at the claim level
./concepts/typed-trust-overview.md:318:  (`evidence.oracle: [Biopython, FreeSASA]`) but doesn't bind which
./concepts/typed-trust-overview.md:319:  tolerance is checked against which oracle. The typed model supports
./concepts/typed-trust-overview.md:321:  unambiguously when there's a single oracle. Multi-oracle claims
./concepts/typed-trust-overview.md:322:  leave `against: None` and the oracle binding lives only in
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:14:Review this DESIGN DRAFT for a new EVIDENT claim kind. Context: the user (paper author) ran EVIDENT's Phase 5 input-side extractor on their own preprint and 0–1 of 7 candidates passed because the paper's natural claims are benchmark-concordance, not subject-bounded measurements. This draft proposes a new `kind: behavioral_concordance` with framework-owned pattern primitives (numeric_band, same_order_of_magnitude, ordinal_match, monotone_with) and docker-owned data artifacts. The draft itself lists six open questions at the bottom — feel free to answer any you think the draft already has enough info to answer, but please also flag anything NOT in those open questions that you think is load-bearing.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:21:5. Anything in the existing typed-trust schema (Tolerance, Criterion, last_verified, Evidence, replay_status) this would inadvertently overlap with or contradict.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:26:# Design — `kind: behavioral_concordance` (v1 DRAFT)
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:45:paper's **own** subject. The validator correctly rejected six of
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:46:the seven as `comparator_bound_to_wrong_subject`. The one that
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:64:  kind: behavioral_concordance
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:84:    oracle: [docker-replay]
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:196:  identical to the existing `last_verified` shape so consumers
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:237:  so a `nix-replay` oracle is a follow-up not a redesign.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:258:  `last_verified.json` sidecar shape.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:265:  `measurement` / `metadata_compatibility` paths changes.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:307:Comparable to PR5b (`metadata_compatibility`):
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:335:I’ll check the existing typed-trust structures before reviewing the draft, especially `Tolerance`, `Criterion`, `last_verified`, `Evidence`, and replay status, so the overlap points are grounded in the repo rather than inferred.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:415:tests/fixtures/extract/paper/wrong_subject_binding.md
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:431:/bin/bash -lc 'rg -n "struct Tolerance|enum Tolerance|Tolerance|struct Criterion|enum Criterion|Criterion|last_verified|replay_status|struct Evidence|enum Evidence|Evidence" -S .' in /scratch/TMAlign/evident/evident-agent
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:433:./tests/test_cli_dry_run.py:46:    sidecar = tmp_path / "last_verified.json"
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:434:./tests/test_cli_dry_run.py:70:    sidecar = tmp_path / "last_verified.json"
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:437:./tests/test_review.py:505:    # last_verified.value = violation.observed_value (so backing sustains).
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:438:./tests/test_review.py:506:    assert backing["last_verified"]["value"] == 0.025
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:439:./tests/test_review.py:507:    assert backing["last_verified"]["date"]
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:440:./tests/test_violation.py:12:  is the logical inverse of the target's, whose last_verified.value
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:441:./tests/test_violation.py:85:    # last_verified.value = observed; satisfies the inverse tolerance.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:442:./tests/test_violation.py:86:    assert backing["last_verified"]["value"] == 0.025
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:443:./tests/test_violation.py:87:    assert backing["last_verified"]["value"] >= tol["value"]
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:444:./tests/test_violation.py:88:    # Date is present so typed-trust's translate_last_verified binds.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:445:./tests/test_violation.py:89:    assert backing["last_verified"]["date"]
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:446:./tests/test_violation.py:445:    assert backing["last_verified"]["value"] == observed
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:447:./tests/fixtures/adversarial/sasa_endorse/evident.yaml:26:    last_verified:
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:449:./tests/test_rephrase.py:262:    """Codex F-REPHRASE-CR1 P1: a curator adding ``last_verified:
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:450:./tests/test_rephrase.py:271:        assert "last_verified" not in parsed
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:451:./tests/test_rephrase.py:272:        parsed["last_verified"] = None  # add null
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:452:./tests/test_rephrase.py:281:    assert "last_verified" in str(exc.value)
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:458:./tests/fixtures/adversarial/ball_challenge/evident.yaml:24:    last_verified:
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:459:./tests/test_sidecar.py:16:    path = tmp_path / "last_verified.json"
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:460:./tests/test_cli_review.py:103:def test_last_verified_commit_reaches_the_digest_header(tmp_path: Path) -> None:
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:461:./tests/test_cli_review.py:105:    from the last_verified sidecar so the reproducible_chain check
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:462:./tests/test_cli_review.py:117:    last_verified = tmp_path / "last_verified.json"
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:463:./tests/test_cli_review.py:118:    last_verified.write_text(
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:464:./tests/test_cli_review.py:137:    entries = sidecar_mod.read(last_verified)
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:465:./tests/test_cli_review.py:140:        {"last_verified": None}, entries.get("claim-A")
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:469:./tests/fixtures/adversarial/ball_dissent/evident.yaml:24:    last_verified:
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:470:./tests/test_review_sidecar.py:201:                "last_verified": {"date": "2026-06-02", "value": 0.025},
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:471:./tests/test_required_fixtures.py:585:    target["last_verified"] = {
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:472:./tests/test_required_fixtures.py:609:        "    last_verified:\n"
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:473:./tests/test_required_fixtures.py:820:                                "last_verified": {
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:474:./examples/proteon_sasa_release.sh:19:#  - Writes a sidecar entry in workflow/evident.py's last_verified.json
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:475:./examples/proteon_sasa_release.sh:28:SIDECAR="/scratch/TMAlign/proteon/evident/last_verified.json"
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:476:./evident_agent/review_sidecar.py:3:Mirrors Phase 1's ``sidecar.py`` (last_verified) but with two extra
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:479:./evident_agent/curator.py:154:    "last_verified",
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:480:./evident_agent/curator.py:540:      ``last_verified`` must not change. Those fields require typed
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:481:./evident_agent/curator.py:641:# add `last_verified: null` to bypass the lock. Comparing via
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:482:./evident_agent/curator.py:653:    Absent-vs-null distinction: a curator inserting ``last_verified:
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:483:./evident_agent/cli.py:84:    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:484:./evident_agent/cli.py:122:        sidecar_path = manifest_path.parent / "last_verified.json"
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:485:./evident_agent/cli.py:274:    "last_verified_sidecar_path",
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:486:./evident_agent/cli.py:278:        "Path to a last_verified.json sidecar to overlay onto each claim "
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:487:./evident_agent/cli.py:279:        "before producing the digest. Default: manifest.parent / 'last_verified.json' if it exists."
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:488:./evident_agent/cli.py:317:    last_verified_sidecar_path: Optional[Path],
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:489:./evident_agent/cli.py:346:    if last_verified_sidecar_path is None:
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:490:./evident_agent/cli.py:347:        candidate = manifest_path.parent / "last_verified.json"
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:491:./evident_agent/cli.py:349:            last_verified_sidecar_path = candidate
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:492:./evident_agent/cli.py:365:    # Load the last_verified sidecar (if any) so per-claim verification
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:493:./evident_agent/cli.py:369:    last_verified_by_claim: dict[str, sidecar.LastVerifiedEntry] = {}
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:494:./evident_agent/cli.py:370:    if last_verified_sidecar_path is not None and last_verified_sidecar_path.is_file():
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:495:./evident_agent/cli.py:371:        last_verified_by_claim = sidecar.read(last_verified_sidecar_path)
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:496:./evident_agent/cli.py:390:        # Per-claim commit comes from (in order): the last_verified
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:497:./evident_agent/cli.py:391:        # sidecar entry > the manifest's inline last_verified.commit
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:498:./evident_agent/cli.py:394:        commit = _resolve_commit_for_claim(claim.raw, last_verified_by_claim.get(claim.id))
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:499:./evident_agent/cli.py:586:            sidecar_path=last_verified_sidecar_path,
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:500:./evident_agent/cli.py:604:    Precedence: sidecar entry > inline manifest last_verified.commit
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:501:./evident_agent/cli.py:611:    inline = (claim_raw.get("last_verified") or {}).get("commit")
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:503:./evident_agent/cli.py:642:    evidence pointer, last_verified) — multi-criterion claims must
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:504:./evident_agent/cli.py:1036:    evidence, provenance, last_verified) must NOT be changed —
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:505:./evident_agent/sidecar.py:1:"""Sidecar ``last_verified.json`` read/write.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:507:./evident_agent/violation.py:21:  is the target's bound; `last_verified.value` is the violation's
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:509:./evident_agent/violation.py:223:      observed_value as ``last_verified.value``, the backing's
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:510:./evident_agent/violation.py:229:    - Includes ``last_verified.date`` (today's date in UTC) AND
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:511:./evident_agent/violation.py:230:      ``last_verified.value``. Both fields are required for
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:512:./evident_agent/violation.py:231:      typed-trust's ``translate_last_verified`` to bind the
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:513:./evident_agent/violation.py:282:        "last_verified": {
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:514:./evident_agent/__init__.py:7:``last_verified.json`` (framework convention) which typed-trust then
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:518:./evident_agent/extract/render.py:122:        "last_verified": {
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:520:./evident_agent/extract/render.py:198:                f"  Tolerances: {len(c.tolerances)}; "
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:523:The first pass shows this repo already treats `last_verified` as a per-claim sidecar overlay with `date`, `value`, and optional commit semantics, and `replay_status` currently appears under extracted evidence. I’m opening the schema paths now to catch naming and ownership collisions.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:632:"""Sidecar ``last_verified.json`` read/write.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:758:    subject_aliases: list[str]
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:759:    tolerances: list[dict]
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:814:        "tolerances": [_tolerance_block(t) for t in claim.tolerances],
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:816:            "oracle": ["Paper-Authority"],
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:839:        "last_verified": {
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:850:    extractor-side fields (``source_span``, ``subject_aliases``)
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:887:        for i, t in enumerate(claim.tolerances):
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:915:                f"  Tolerances: {len(c.tolerances)}; "
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:916:                f"subject aliases: {c.subject_aliases}"
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1062:A concrete collision is emerging: the draft’s artifact `provenance` wants `produced_at`, `image_digest`, and `command`, but the current sidecar reader only knows `commit`, `date`, `value`, `corpus_sha`. That does not kill the design, but it means “identical to existing `last_verified` shape” is not true in this repo.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1357:    tolerances:
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1363:      oracle: [Manual]
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1403:                subject_aliases=["our method", "we"],
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1404:                tolerances=[
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1502:    Precedence: sidecar entry > inline manifest last_verified.commit
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1509:    inline = (claim_raw.get("last_verified") or {}).get("commit")
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1516:    missing tolerances."""
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1517:    tols = claim_raw.get("tolerances") or []
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1529:    tols = claim_raw.get("tolerances") or []
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1539:    The model sees the full structured claim (tier, all tolerances,
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1540:    evidence pointer, last_verified) — multi-criterion claims must
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1667:                subject_aliases=["our method", "we", "ours"],
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1668:                tolerances=[
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1752:    t = manifest["claims"][0]["tolerances"][0]
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1763:    v3: default-deny means honest emptiness > invented tolerances)."""
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1809:**Potential bug: `last_verified` shape does not match the draft.** In this repo, `last_verified.json` is per-claim and currently carries `commit`, `date`, `value`, `corpus_sha`. The draft’s artifact `provenance` block wants `corpus_sha`, `image_digest`, `command`, `produced_at` and says it is “identical to existing `last_verified` shape.” That is false and load-bearing. Add a distinct `artifact_provenance` schema or extend sidecar deliberately.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1833:On typed-trust overlap: `numeric_band` is conceptually close to `Tolerance`, but it is not the same thing. `Tolerance` binds an observed value to the paper’s own asserted bound. Concordance binds an observed replay value to an external prior. Do not reuse `Tolerance` without a discriminator, or the subject-binding validator will keep being confused. `Criterion` may be the better abstraction point: behavioral concordance is a new criterion family, not just a new tolerance.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1835:`last_verified.value` being scalar is also a mismatch for `ordinal_match` and `monotone_with`. Those need a comparison result plus possibly observed vectors/ranks. Do not overload `value`; add `comparison_status`, `observed_value(s)`, and `diagnostics` or store a concordance-specific result sidecar.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1844:**Potential bug: `last_verified` shape does not match the draft.** In this repo, `last_verified.json` is per-claim and currently carries `commit`, `date`, `value`, `corpus_sha`. The draft’s artifact `provenance` block wants `corpus_sha`, `image_digest`, `command`, `produced_at` and says it is “identical to existing `last_verified` shape.” That is false and load-bearing. Add a distinct `artifact_provenance` schema or extend sidecar deliberately.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1868:On typed-trust overlap: `numeric_band` is conceptually close to `Tolerance`, but it is not the same thing. `Tolerance` binds an observed value to the paper’s own asserted bound. Concordance binds an observed replay value to an external prior. Do not reuse `Tolerance` without a discriminator, or the subject-binding validator will keep being confused. `Criterion` may be the better abstraction point: behavioral concordance is a new criterion family, not just a new tolerance.
./EVIDENT_BEHAVIORAL_CONCORDANCE.codex-review.md:1870:`last_verified.value` being scalar is also a mismatch for `ordinal_match` and `monotone_with`. Those need a comparison result plus possibly observed vectors/ranks. Do not overload `value`; add `comparison_status`, `observed_value(s)`, and `diagnostics` or store a concordance-specific result sidecar.
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:284:+    for i, t in enumerate(claim.get("tolerances") or []):
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:618:+                "tolerances": [
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:627:+                    "oracle": ["Paper-Authority"],
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:651:+                "tolerances": [
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:660:+                    "oracle": ["Paper-Authority"],
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:684:+                "tolerances": [
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:693:+                    "oracle": ["Paper-Authority"],
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:1278:   123	    for i, t in enumerate(claim.get("tolerances") or []):
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:2060:    39	  - reason: bound_not_stated | comparator_bound_to_wrong_subject | ...
./EVIDENT_REVIEW_WALKTHROUGH.code-review.md:2575:evident-agent/tests/test_extract_framing.py:26:def test_tool_schema_claim_requires_subject_aliases_and_source_span():
./concepts/typed-trust.md:216:enum ClaimKind {
./concepts/typed-trust.md:342:    against: Option<String>,            // names the oracle from manifest
./concepts/typed-trust.md:345:                                        // multiple oracles with different
./concepts/typed-trust.md:346:                                        // tolerances
./concepts/typed-trust.md:474:- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./concepts/typed-trust.md:498:- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./concepts/typed-trust.md:499:  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-5.md:238:+enum ClaimKind {
./typed-trust-codex-review-5.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-5.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-5.md:368:+                                        // tolerances
./typed-trust-codex-review-5.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-5.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-5.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-5.md:614:+pub enum ClaimKind {
./typed-trust-codex-review-5.md:1224:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-5.md:1291:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-5.md:1530:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-5.md:2009:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-5.md:2032:+///   `last_verified`).
./typed-trust-codex-review-5.md:2599:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-5.md:2600:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-5.md:2601:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-5.md:2609:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-5.md:2643:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-5.md:2655:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-5.md:2658:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-5.md:2690:+    pub oracle: Vec<String>,
./typed-trust-codex-review-5.md:2711:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-5.md:2720:+    /// release claims must carry structured tolerances.
./typed-trust-codex-review-5.md:2830:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-5.md:2834:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-5.md:2835:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-5.md:2837:+pub fn translate_tolerances(
./typed-trust-codex-review-5.md:2840:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-5.md:2841:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-5.md:2842:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-5.md:2848:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-5.md:2856:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-5.md:2868:+    single_oracle: &Option<String>,
./typed-trust-codex-review-5.md:2877:+        // tolerances; allowing them to translate would let
./typed-trust-codex-review-5.md:2894:+            against: single_oracle.clone(),
./typed-trust-codex-review-5.md:2907:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-5.md:2908:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-5.md:2910:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-5.md:2925:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-5.md:2927:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-5.md:2937:+    let reruns = translate_last_verified(
./typed-trust-codex-review-5.md:2938:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-5.md:2975:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-5.md:2977:+/// - `last_verified` is absent;
./typed-trust-codex-review-5.md:2978:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-5.md:2979:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-5.md:2984:+fn translate_last_verified(
./typed-trust-codex-review-5.md:3010:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-5.md:3077:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-5.md:3078:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-5.md:3085:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-5.md:3352:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-5.md:3514:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-5.md:3532:+    tolerances:
./typed-trust-codex-review-5.md:3540:+      oracle:
./typed-trust-codex-review-5.md:3545:+    last_verified:
./typed-trust-codex-review-5.md:3566:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-5.md:3831:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-5.md:3853:+    // populated to the oracle name.
./typed-trust-codex-review-5.md:3899:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-5.md:4007:+               within tier-specific tolerances."
./typed-trust-codex-review-5.md:4019:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-5.md:4066:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-5.md:4083:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-5.md:4094:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-5.md:4178:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-5.md:4184:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-5.md:4294:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-5.md:4300:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-5.md:4301:+/// translation working) with last_verified populated.
./typed-trust-codex-review-5.md:4315:+    tolerances:
./typed-trust-codex-review-5.md:4323:+      oracle:
./typed-trust-codex-review-5.md:4328:+    last_verified:
./typed-trust-codex-review-5.md:4337:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-5.md:4351:+    tolerances:
./typed-trust-codex-review-5.md:4358:+      oracle:
./typed-trust-codex-review-5.md:4363:+    last_verified:
./typed-trust-codex-review-5.md:4383:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-5.md:4429:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-5.md:4431:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-5.md:4432:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-5.md:5316:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-5.md:5321:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-5.md:5322:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-5.md:5340:+    tolerances:
./typed-trust-codex-review-5.md:5348:+      oracle:
./typed-trust-codex-review-5.md:5353:+    last_verified:
./typed-trust-codex-review-5.md:5361:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-5.md:5365:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-5.md:5366:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-5.md:5367:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-5.md:5383:+    tolerances:
./typed-trust-codex-review-5.md:5391:+      oracle:
./typed-trust-codex-review-5.md:5396:+    last_verified:
./typed-trust-codex-review-5.md:5410:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-5.md:5411:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-5.md:5421:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-5.md:5423:+      oracle:
./typed-trust-codex-review-5.md:5425:+      command: pytest tests/oracle -v
./typed-trust-codex-review-5.md:5449:+    tolerances:
./typed-trust-codex-review-5.md:5469:+      oracle:
./typed-trust-codex-review-5.md:5471:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-5.md:5496:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-5.md:5517:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-5.md:5519:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-5.md:5527:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-5.md:5528:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-5.md:5533:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-5.md:5548:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-5.md:5556:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-5.md:5560:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-5.md:5582:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-5.md:5612:+    tolerances:
./typed-trust-codex-review-5.md:5618:+      oracle: [SomeOracle]
./typed-trust-codex-review-5.md:5623:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-5.md:5649:+    tolerances:
./typed-trust-codex-review-5.md:5655:+      oracle: [internal]
./typed-trust-codex-review-5.md:5656:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-5.md:5660:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-5.md:5701:+    tolerances:
./typed-trust-codex-review-5.md:5705:+      oracle: [Foo]
./typed-trust-codex-review-5.md:5706:+      command: pytest tests/oracle
./typed-trust-codex-review-5.md:5710:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-5.md:5731:+    tolerances:
./typed-trust-codex-review-5.md:5735:+      oracle: [Foo]
./typed-trust-codex-review-5.md:5736:+      command: pytest tests/oracle
./typed-trust-codex-review-5.md:5740:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-5.md:5761:+    tolerances:
./typed-trust-codex-review-5.md:5766:+      oracle: [Foo]
./typed-trust-codex-review-5.md:5771:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-5.md:5787:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-5.md:5790:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-5.md:5793:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-5.md:5796:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-5.md:5803:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-5.md:5832:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-5.md:5835:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-5.md:5852:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-5.md:5866:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-5.md:5880:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-5.md:5896: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-5.md:5902: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-5.md:5913:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-5.md:5935:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-5.md:6059:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-5.md:6126:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-5.md:6264://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-5.md:6265://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-5.md:6266://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-5.md:6274://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-5.md:6308:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-5.md:6320:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-5.md:6323:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-5.md:6355:    pub oracle: Vec<String>,
./typed-trust-codex-review-5.md:6376:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-5.md:6385:    /// release claims must carry structured tolerances.
./typed-trust-codex-review-5.md:6495:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-5.md:6499:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-5.md:6500:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-5.md:6502:pub fn translate_tolerances(
./typed-trust-codex-review-5.md:6505:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-5.md:6506:        if e.oracle.len() == 1 {
./typed-trust-codex-review-5.md:6507:            Some(e.oracle[0].clone())
./typed-trust-codex-review-5.md:6540:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-5.md:6544:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-5.md:6545:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-5.md:6547:pub fn translate_tolerances(
./typed-trust-codex-review-5.md:6550:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-5.md:6551:        if e.oracle.len() == 1 {
./typed-trust-codex-review-5.md:6552:            Some(e.oracle[0].clone())
./typed-trust-codex-review-5.md:6558:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-5.md:6566:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-5.md:6578:    single_oracle: &Option<String>,
./typed-trust-codex-review-5.md:6587:        // tolerances; allowing them to translate would let
./typed-trust-codex-review-5.md:6604:            against: single_oracle.clone(),
./typed-trust-codex-review-5.md:6617:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-5.md:6618:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-5.md:6620:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-5.md:6635:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-5.md:6637:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-5.md:6647:    let reruns = translate_last_verified(
./typed-trust-codex-review-5.md:6648:        mc.last_verified.as_ref(),
./typed-trust-codex-review-5.md:6685:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-5.md:6687:/// - `last_verified` is absent;
./typed-trust-codex-review-5.md:6688:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-5.md:6689:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-5.md:6694:fn translate_last_verified(
./typed-trust-codex-review-5.md:6720:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-5.md:6787:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-5.md:6788:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-5.md:6795:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-5.md:6926://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-5.md:6949:///   `last_verified`).
./typed-trust-codex-review-5.md:8339:   `subsystem`, `oracle`, and `capability` are declared per-manifest;
./typed-trust-codex-review-5.md:8358:  oracle:       [...]
./typed-trust-codex-review-5.md:8380:| `oracle`           | empty — consumer must define                                       |
./typed-trust-codex-review-5.md:8403:| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
./typed-trust-codex-review-5.md:8405:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-5.md:8406:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-5.md:8409:| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-5.md:8415:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-5.md:8426:- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review-5.md:8427:  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review-5.md:8436:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-5.md:8445:tolerances:
./typed-trust-codex-review-5.md:8735:/bin/bash -lc "grep -R \"last_verified\\|tolerances\\|prose-only\\|provenance\" -n workflow | head -100" in /scratch/TMAlign/evident
./typed-trust-codex-review-5.md:8740:workflow/GRAMMAR.md:42:of the claim from `subsystem`, `outputs`, `inputs`, `tolerances`, and
./typed-trust-codex-review-5.md:8743:workflow/GRAMMAR.md:94:Research claims may carry prose-only tolerances and unspecified
./typed-trust-codex-review-5.md:8744:workflow/GRAMMAR.md:110:| `last_verified` populated                   | optional   | recommended | required |
./typed-trust-codex-review-5.md:8746:workflow/GRAMMAR.md:134:  is comparable to the recorded `last_verified.value`. If the command
./typed-trust-codex-review-5.md:8748:workflow/GRAMMAR.md:188:  from `tolerances`+`last_verified`, conditioning from
./typed-trust-codex-review-5.md:8749:workflow/validate_manifest.py:67:    "tolerances",
./typed-trust-codex-review-5.md:8750:workflow/validate_manifest.py:131:            f"move tolerance text into the top-level tolerances: list "
./typed-trust-codex-review-5.md:8751:workflow/validate_manifest.py:144:def validate_tolerances(
./typed-trust-codex-review-5.md:8752:workflow/validate_manifest.py:148:        fail(f"claim {claim_id}: tolerances must be a non-empty list")
./typed-trust-codex-review-5.md:8753:workflow/validate_manifest.py:151:            fail(f"claim {claim_id}: tolerances[{i}] must be a mapping")
./typed-trust-codex-review-5.md:8754:workflow/validate_manifest.py:154:            fail(f"claim {claim_id}: tolerances[{i}].prose is required")
./typed-trust-codex-review-5.md:8755:workflow/validate_manifest.py:162:                f"claim {claim_id}: tolerances[{i}] metric/op/value are "
./typed-trust-codex-review-5.md:8756:workflow/validate_manifest.py:170:                f"tolerances[{i}].metric",
./typed-trust-codex-review-5.md:8757:workflow/validate_manifest.py:177:                f"tolerances[{i}].op",
./typed-trust-codex-review-5.md:8758:workflow/validate_manifest.py:183:                fail(f"claim {claim_id}: tolerances[{i}].value must be numeric")
./typed-trust-codex-review-5.md:8766:workflow/validate_manifest.py:283:def validate_last_verified(value: Any, claim_id: str) -> None:
./typed-trust-codex-review-5.md:8767:workflow/validate_manifest.py:285:        fail(f"claim {claim_id}: last_verified must be a mapping")
./typed-trust-codex-review-5.md:8768:workflow/validate_manifest.py:293:                fail(f"claim {claim_id}: last_verified.value must be numeric or null")
./typed-trust-codex-review-5.md:8769:workflow/validate_manifest.py:295:            fail(f"claim {claim_id}: last_verified.{key} must be a string or null")
./typed-trust-codex-review-5.md:8770:workflow/validate_manifest.py:434:        if "last_verified" in claim:
./typed-trust-codex-review-5.md:8771:workflow/validate_manifest.py:435:            validate_last_verified(claim["last_verified"], claim_id)
./typed-trust-codex-review-5.md:8773:workflow/validate_manifest.py:452:            validate_tolerances(claim["tolerances"], claim_id, vocabularies)
./typed-trust-codex-review-5.md:8775:workflow/SCHEMA.md:89:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-5.md:8777:workflow/SCHEMA.md:93:| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-5.md:8778:workflow/SCHEMA.md:99:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-5.md:8779:workflow/SCHEMA.md:120:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-5.md:8780:workflow/SCHEMA.md:129:tolerances:
./typed-trust-codex-review-5.md:8781:workflow/SCHEMA.md:168:Each named entry can be referenced by `tolerances[].output` so a single
./typed-trust-codex-review-5.md:8785:workflow/SCHEMA.md:199:underlying numbers are correct (that is what `tolerances` and
./typed-trust-codex-review-5.md:8786:workflow/SCHEMA.md:200:`last_verified` are for). The three levels:
./typed-trust-codex-review-5.md:8789:workflow/SCHEMA.md:219:replace the underlying `tolerances` and `evidence` fields.
./typed-trust-codex-review-5.md:8790:workflow/SCHEMA.md:224:last_verified:
./typed-trust-codex-review-5.md:8791:workflow/SCHEMA.md:234:typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-5.md:8792:workflow/SCHEMA.md:267:- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-5.md:8793:workflow/SCHEMA.md:307:1. Lift `evidence.tolerance` into one or more `tolerances:` entries.
./typed-trust-codex-review-5.md:8794:workflow/SCHEMA.md:312:   measurements (and they may then drop `subsystem` and `tolerances`).
./typed-trust-codex-review-5.md:8795:workflow/evident.py:8:  replay      re-execute claims' evidence.command and update last_verified
./typed-trust-codex-review-5.md:8796:workflow/evident.py:135:    # `--stale DAYS`. Reads `last_verified.json` next to the manifest;
./typed-trust-codex-review-5.md:8797:workflow/evident.py:299:        add("    tolerances:")
./typed-trust-codex-review-5.md:8798:workflow/evident.py:327:    add("    last_verified:")
./typed-trust-codex-review-5.md:8799:workflow/evident.py:393:# `replay` — re-execute claims' evidence.command and update last_verified
./typed-trust-codex-review-5.md:8800:workflow/evident.py:395:# Closes the loop the schema's last_verified.{date, commit, value, corpus_sha}
./typed-trust-codex-review-5.md:8801:workflow/evident.py:400:# Storage decision: a sidecar `last_verified.json` file next to the manifest,
./typed-trust-codex-review-5.md:8802:workflow/evident.py:417:# etc.) and is its own design. Until that lands, last_verified.value stays
./typed-trust-codex-review-5.md:8803:workflow/evident.py:423:SIDECAR_FILENAME = "last_verified.json"
./typed-trust-codex-review-5.md:8804:workflow/evident.py:453:    Used to fill `commit` in last_verified entries. Returning None when
./typed-trust-codex-review-5.md:8805:workflow/evident.py:614:        # last_verified.commit as the SOURCE SHA where the claim
./typed-trust-codex-review-5.md:8806:workflow/evident.py:704:            "Filter to claims whose sidecar last_verified.date is older than "
./typed-trust-codex-review-5.md:8807:workflow/evident.py:705:            "DAYS (or absent). Reads `last_verified.json` next to the "
./typed-trust-codex-review-5.md:8808:workflow/evident.py:713:        help="Re-execute claims' evidence.command and update last_verified",
./typed-trust-codex-review-5.md:8809:workflow/evident.py:717:            "write to a sidecar `last_verified.json`. The sidecar is the "
./typed-trust-codex-review-5.md:8815:            f"move tolerance text into the top-level tolerances: list "
./typed-trust-codex-review-5.md:8818:    oracles = require_string_list(value["oracle"], "evidence.oracle", claim_id)
./typed-trust-codex-review-5.md:8819:    for oracle_name in oracles:
./typed-trust-codex-review-5.md:8821:            oracle_name, "oracle", vocabularies, "evidence.oracle[]", claim_id
./typed-trust-codex-review-5.md:8825:    return oracles
./typed-trust-codex-review-5.md:8828:def validate_tolerances(
./typed-trust-codex-review-5.md:8832:        fail(f"claim {claim_id}: tolerances must be a non-empty list")
./typed-trust-codex-review-5.md:8835:            fail(f"claim {claim_id}: tolerances[{i}] must be a mapping")
./typed-trust-codex-review-5.md:8838:            fail(f"claim {claim_id}: tolerances[{i}].prose is required")
./typed-trust-codex-review-5.md:8846:                f"claim {claim_id}: tolerances[{i}] metric/op/value are "
./typed-trust-codex-review-5.md:8854:                f"tolerances[{i}].metric",
./typed-trust-codex-review-5.md:8861:                f"tolerances[{i}].op",
./typed-trust-codex-review-5.md:8867:                fail(f"claim {claim_id}: tolerances[{i}].value must be numeric")
./typed-trust-codex-review-5.md:8877:of the claim from `subsystem`, `outputs`, `inputs`, `tolerances`, and
./typed-trust-codex-review-5.md:8911:Project-specific axes (`subsystem`, `oracle`, `capability`) are open,
./typed-trust-codex-review-5.md:8929:Research claims may carry prose-only tolerances and unspecified
./typed-trust-codex-review-5.md:8943:| `pinned_versions` covers source + oracles   | required   | required| required  |
./typed-trust-codex-review-5.md:8945:| `last_verified` populated                   | optional   | recommended | required |
./typed-trust-codex-review-5.md:8951:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-5.md:8960:tolerances:
./typed-trust-codex-review-5.md:8999:/bin/bash -lc "sed -n '380,650p' workflow/evident.py && find . -name 'last_verified.json' -print" in /scratch/TMAlign/evident
./typed-trust-codex-review-5.md:9014:# `replay` — re-execute claims' evidence.command and update last_verified
./typed-trust-codex-review-5.md:9016:# Closes the loop the schema's last_verified.{date, commit, value, corpus_sha}
./typed-trust-codex-review-5.md:9021:# Storage decision: a sidecar `last_verified.json` file next to the manifest,
./typed-trust-codex-review-5.md:9038:# etc.) and is its own design. Until that lands, last_verified.value stays
./typed-trust-codex-review-5.md:9044:SIDECAR_FILENAME = "last_verified.json"
./typed-trust-codex-review-5.md:9074:    Used to fill `commit` in last_verified entries. Returning None when
./typed-trust-codex-review-5.md:9235:        # last_verified.commit as the SOURCE SHA where the claim
./typed-trust-codex-review-5.md:9295:   244	/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-5.md:9299:   248	/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-5.md:9300:   249	/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-5.md:9302:   251	pub fn translate_tolerances(
./typed-trust-codex-review-5.md:9305:   254	    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-5.md:9306:   255	        if e.oracle.len() == 1 {
./typed-trust-codex-review-5.md:9307:   256	            Some(e.oracle[0].clone())
./typed-trust-codex-review-5.md:9313:   262	    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-5.md:9321:   270	            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-5.md:9789:- [P2] Reject measurement claims without tolerances — /scratch/TMAlign/evident/typed-trust/src/translate.rs:262-264
./typed-trust-codex-review-5.md:9790:  When a measurement claim omits `tolerances` (or provides an empty list), this returns an empty criteria set; `synthesize()` then emits a `Current` report with nothing to assess. Because the shipping schema requires measurement claims to have non-empty tolerances, the translator should surface this as a translation error instead of allowing an under-specified CI/release claim to look current.
./typed-trust-codex-review-5.md:9798:- [P2] Reject measurement claims without tolerances — /scratch/TMAlign/evident/typed-trust/src/translate.rs:262-264
./typed-trust-codex-review-5.md:9799:  When a measurement claim omits `tolerances` (or provides an empty list), this returns an empty criteria set; `synthesize()` then emits a `Current` report with nothing to assess. Because the shipping schema requires measurement claims to have non-empty tolerances, the translator should surface this as a translation error instead of allowing an under-specified CI/release claim to look current.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:14:Review this small follow-up plan as an independent engineer. Context: PR5b just landed a new metadata_compatibility claim kind in EVIDENT's typed-trust. The codex review of PR5b noted two P3s: render doesn't surface the metadata fields (criteria=[] just renders empty), and MCP consumers can't structurally query metadata. This plan addresses both as a small follow-up PR.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:16:The change adds a MetadataDeclaration struct + Option<MetadataDeclaration> field on the typed Claim, lifts the manifest's metadata block onto it during translation, adds a Metadata declaration block to human_render + html_render, adds a query_metadata MCP tool, and surfaces kind+metadata in list_claims.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:56:pub struct MetadataDeclaration {
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:77:typed `MetadataDeclaration` when `kind == metadata_compatibility`.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:129:where `kind == metadata_compatibility` and applies the optional
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:282:./evident-agent/tests/fixtures/extract/paper/wrong_subject_binding.md
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:514:./typed-trust/src/mcp/tools.rs:43:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:521:./typed-trust/src/mcp/handlers.rs:253:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:528:./typed-trust/src/mcp/handlers.rs:497:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:752:./EVIDENT_AGENT_PHASE3.code-review.md:1066:+    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:759:./EVIDENT_AGENT_PHASE3.code-review.md:1310:+    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:770:./EVIDENT_AGENT_PHASE3.code-review.md:1690:+        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:777:./EVIDENT_AGENT_PHASE3.code-review.md:2054:   221	    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:784:./EVIDENT_AGENT_PHASE3.code-review.md:2299:   465	    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:811:./EVIDENT_AGENT_PHASE3.code-review.md:3520:typed-trust/src/mcp/handlers.rs:221:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:812:./EVIDENT_AGENT_PHASE3.code-review.md:3522:typed-trust/src/mcp/handlers.rs:465:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:813:./EVIDENT_AGENT_PHASE3.code-review.md:3524:typed-trust/src/mcp/tools.rs:43:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:814:./EVIDENT_AGENT_PHASE3.code-review.md:3540:typed-trust/src/synthesize.rs:30:///   `last_verified`).
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:821:./EVIDENT_AGENT_PHASE3.code-review.md:3813:   221	    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1155:./typed-trust-codex-review-5.md:9790:  When a measurement claim omits `tolerances` (or provides an empty list), this returns an empty criteria set; `synthesize()` then emits a `Current` report with nothing to assess. Because the shipping schema requires measurement claims to have non-empty tolerances, the translator should surface this as a translation error instead of allowing an under-specified CI/release claim to look current.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1158:./typed-trust-codex-review-5.md:9799:  When a measurement claim omits `tolerances` (or provides an empty list), this returns an empty criteria set; `synthesize()` then emits a `Current` report with nothing to assess. Because the shipping schema requires measurement claims to have non-empty tolerances, the translator should surface this as a translation error instead of allowing an under-specified CI/release claim to look current.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1565:./EVIDENT_PR5B.code-review.md:24:3) MCP SURFACE — list_claims surfaces the claim id + tier + kind. For a metadata claim, the 'kind' field becomes 'metadata_compatibility' (string). Does any existing MCP consumer rely on kind being 'measurement'? If so, metadata claims would break their assumptions.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1572:./EVIDENT_PR5B.code-review.md:1300:typed-trust/tests/human_render.rs:9:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1573:./EVIDENT_PR5B.code-review.md:1301:typed-trust/tests/human_render.rs:61:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1575:./EVIDENT_PR5B.code-review.md:1303:typed-trust/tests/synthesize_and_render.rs:13:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1576:./EVIDENT_PR5B.code-review.md:1304:typed-trust/tests/synthesize_and_render.rs:102:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1578:./EVIDENT_PR5B.code-review.md:1306:typed-trust/tests/html_render.rs:7:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1579:./EVIDENT_PR5B.code-review.md:1307:typed-trust/tests/html_render.rs:59:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1585:./EVIDENT_PR5B.code-review.md:1462:typed-trust/tests/human_render.rs:61:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1610:./EVIDENT_PR5B.code-review.md:1574:typed-trust/tests/synthesize_and_render.rs:102:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1810:./EVIDENT_PR5B.code-review.md:1804:typed-trust/tests/html_render.rs:59:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:1932:./EVIDENT_PR5B.code-review.md:2384:typed-trust/src/mcp/tools.rs:43:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2024:./EVIDENT_PR5B.code-review.md:3526:typed-trust/tests/human_render.rs:9:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2028:./EVIDENT_PR5B.code-review.md:3530:typed-trust/tests/human_render.rs:61:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2052:./EVIDENT_PR5B.code-review.md:3578:typed-trust/tests/html_render.rs:7:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2056:./EVIDENT_PR5B.code-review.md:3582:typed-trust/tests/html_render.rs:59:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2108:./EVIDENT_PR5B.code-review.md:3761:typed-trust/tests/synthesize_and_render.rs:13:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2111:./EVIDENT_PR5B.code-review.md:3764:typed-trust/tests/synthesize_and_render.rs:102:    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2372:./EVIDENT_PR5B.code-review.md:4228:typed-trust/src/mcp/tools.rs:43:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2385:./EVIDENT_PR5B.code-review.md:5168:P3: MCP `list_claims` now surfaces raw `kind`, so metadata rows will return `"metadata_compatibility"` ([handlers.rs](/scratch/TMAlign/evident/typed-trust/src/mcp/handlers.rs:191)). I don’t see in-repo MCP consumers assuming only `"measurement"`, and `render_report` should synthesize metadata because `translate_tolerances`/`translate_evidence` allow empty outputs. External consumers may still have hardcoded kind enums. Also, `replay_status: "not_attempted"` for metadata is misleading because replay is inapplicable, not merely unattempted.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2387:./EVIDENT_PR5B.code-review.md:5195:P3: MCP `list_claims` now surfaces raw `kind`, so metadata rows will return `"metadata_compatibility"` ([handlers.rs](/scratch/TMAlign/evident/typed-trust/src/mcp/handlers.rs:191)). I don’t see in-repo MCP consumers assuming only `"measurement"`, and `render_report` should synthesize metadata because `translate_tolerances`/`translate_evidence` allow empty outputs. External consumers may still have hardcoded kind enums. Also, `replay_status: "not_attempted"` for metadata is misleading because replay is inapplicable, not merely unattempted.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2872:./typed-trust-codex-review-4.md:9402:  This accepts `metric`/`op`/`value` all absent for every claim tier, even though the schema frames prose-only tolerances as the research-tier deferred-spec escape hatch. A CI or release claim with only prose will translate successfully, synthesize to `NotAssessed`, and still produce a `current` report instead of failing translation, which can let under-specified release claims pass through this CLI.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2875:./typed-trust-codex-review-4.md:9411:  This accepts `metric`/`op`/`value` all absent for every claim tier, even though the schema frames prose-only tolerances as the research-tier deferred-spec escape hatch. A CI or release claim with only prose will translate successfully, synthesize to `NotAssessed`, and still produce a `current` report instead of failing translation, which can let under-specified release claims pass through this CLI.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:2920:./EVIDENT_METADATA_RENDER_MCP.codex-review.md:16:The change adds a MetadataDeclaration struct + Option<MetadataDeclaration> field on the typed Claim, lifts the manifest's metadata block onto it during translation, adds a Metadata declaration block to human_render + html_render, adds a query_metadata MCP tool, and surfaces kind+metadata in list_claims.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:3378:./EVIDENT_PHASE5_PR2.codex-review.md:577:typed-trust/tests/human_render.rs:9:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:3381:./EVIDENT_PHASE5_PR2.codex-review.md:627:typed-trust/tests/synthesize_and_render.rs:13:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:3384:./EVIDENT_PHASE5_PR2.codex-review.md:630:typed-trust/tests/html_render.rs:7:    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4136:./EVIDENT_AGENT_PHASE3.code-review-2.md:31:    synthesize_for accepted last_verified_path but threaded it
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4157:./EVIDENT_AGENT_PHASE3.code-review-2.md:682:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4165:./EVIDENT_AGENT_PHASE3.code-review-2.md:933:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4173:./EVIDENT_AGENT_PHASE3.code-review-2.md:1107:typed-trust/src/mcp/handlers.rs:221:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4174:./EVIDENT_AGENT_PHASE3.code-review-2.md:1109:typed-trust/src/mcp/handlers.rs:472:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4175:./EVIDENT_AGENT_PHASE3.code-review-2.md:1122:typed-trust/src/mcp/tools.rs:43:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4176:./EVIDENT_AGENT_PHASE3.code-review-2.md:1139:typed-trust/src/synthesize.rs:30:///   `last_verified`).
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4177:./EVIDENT_AGENT_PHASE3.code-review-2.md:1167:typed-trust/tests/html_render.rs:38:    last_verified:
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4178:./EVIDENT_AGENT_PHASE3.code-review-2.md:1168:typed-trust/tests/synthesize_and_render.rs:20:/// translation working) with last_verified populated.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4179:./EVIDENT_AGENT_PHASE3.code-review-2.md:1169:typed-trust/tests/synthesize_and_render.rs:47:    last_verified:
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4180:./EVIDENT_AGENT_PHASE3.code-review-2.md:1170:typed-trust/tests/synthesize_and_render.rs:56:/// Same claim but with last_verified value set to 0.01 (over the
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4181:./EVIDENT_AGENT_PHASE3.code-review-2.md:1171:typed-trust/tests/synthesize_and_render.rs:82:    last_verified:
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4182:./EVIDENT_AGENT_PHASE3.code-review-2.md:1172:typed-trust/tests/synthesize_and_render.rs:150:    // Strip last_verified so the rerun has no observations.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4183:./EVIDENT_AGENT_PHASE3.code-review-2.md:1173:typed-trust/tests/synthesize_and_render.rs:152:        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4184:./EVIDENT_AGENT_PHASE3.code-review-2.md:1174:typed-trust/tests/synthesize_and_render.rs:153:        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4185:./EVIDENT_AGENT_PHASE3.code-review-2.md:1175:typed-trust/tests/human_render.rs:40:    last_verified:
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4198:./EVIDENT_AGENT_PHASE3.code-review-2.md:1885:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4454:pub enum ClaimKind {
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4484:    if mc.kind != "measurement" && mc.kind != "metadata_compatibility" {
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4491:    // PR5b: metadata_compatibility claims require the `metadata`
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4493:    // NOT carry tolerances/evidence — those belong to the empirical
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4495:    if mc.kind == "metadata_compatibility" {
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4501:        if mc.tolerances.is_some() {
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4522:    let kind = if mc.kind == "metadata_compatibility" {
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4745:    pub tolerances: Option<Vec<ManifestTolerance>>,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4748:    pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4751:    /// PR5b: required when ``kind == "metadata_compatibility"``.
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4760:/// PR5b: structured block for ``kind: metadata_compatibility``
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4848:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:4855:                "last_verified_sidecar": {"type": "string", "description": "Optional last_verified.json sidecar path"}
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:5488:               within tier-specific tolerances."
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:5500:    // §7 — Three Criteria from the YAML's three tolerances
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:5547:                against: Some("FreeSASA".into()), // same metric, different oracle
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:5564:                against: None, // pass rate isn't oracle-specific
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:5575:    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:5850:  oracle: string[];
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:5870:  tolerances?: Tolerance[];
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:5874:  last_verified?: LastVerified;
./EVIDENT_METADATA_RENDER_MCP.codex-review.md:5884:  oracle: string[];
./EVIDENT_PHASE5_PR6.codex-review-v2.md:62:>   that's pages apart from its subject.
./EVIDENT_PHASE5_PR6.codex-review-v2.md:73:papers, ask the model to extract structured tolerances, validate
./EVIDENT_PHASE5_PR6.codex-review-v2.md:241:  `comparator_bound_to_wrong_subject`.
./EVIDENT_PHASE5_PR6.codex-review-v2.md:242:- `wrong_subject_binding.md` — codex-flagged: the comparator
./EVIDENT_PHASE5_PR6.codex-review-v2.md:287:- `--paper wrong_subject_binding.md` produces 0 claims because
./typed-trust-codex-review-10.md:238:+enum ClaimKind {
./typed-trust-codex-review-10.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-10.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-10.md:368:+                                        // tolerances
./typed-trust-codex-review-10.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-10.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-10.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-10.md:580:+``last_verified.json`` (framework convention) which typed-trust then
./typed-trust-codex-review-10.md:668:+    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./typed-trust-codex-review-10.md:706:+        sidecar_path = manifest_path.parent / "last_verified.json"
./typed-trust-codex-review-10.md:822:+oracle binaries available.
./typed-trust-codex-review-10.md:1077:+        if not score.tolerances:
./typed-trust-codex-review-10.md:1080:+        first = score.tolerances[0]
./typed-trust-codex-review-10.md:1148:+"""Sidecar ``last_verified.json`` read/write.
./typed-trust-codex-review-10.md:1352:+#  - Writes a sidecar entry in workflow/evident.py's last_verified.json
./typed-trust-codex-review-10.md:1361:+SIDECAR="/scratch/TMAlign/proteon/evident/last_verified.json"
./typed-trust-codex-review-10.md:1443:+                tolerances:
./typed-trust-codex-review-10.md:1449:+                  oracle: [Test]
./typed-trust-codex-review-10.md:1461:+    sidecar = tmp_path / "last_verified.json"
./typed-trust-codex-review-10.md:1485:+    sidecar = tmp_path / "last_verified.json"
./typed-trust-codex-review-10.md:1763:+    path = tmp_path / "last_verified.json"
./typed-trust-codex-review-10.md:1945:+pub enum ClaimKind {
./typed-trust-codex-review-10.md:3131:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-10.md:3174:+    // Overlay sidecar entries onto each claim's last_verified field
./typed-trust-codex-review-10.md:3182:+                        cw.claim.last_verified = Some(lv.clone());
./typed-trust-codex-review-10.md:3222:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-10.md:3345:+    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./typed-trust-codex-review-10.md:3590:+/// The shape matches `workflow/evident.py`'s `last_verified.json`
./typed-trust-codex-review-10.md:3607:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-10.md:4103:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-10.md:4126:+///   `last_verified`).
./typed-trust-codex-review-10.md:4747:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-10.md:4748:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-10.md:4749:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-10.md:4757:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-10.md:4791:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-10.md:4803:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-10.md:4806:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-10.md:4838:+    pub oracle: Vec<String>,
./typed-trust-codex-review-10.md:4859:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-10.md:4868:+    /// release claims must carry structured tolerances.
./typed-trust-codex-review-10.md:4870:+    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-10.md:4872:+    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-10.md:4906:+                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-10.md:4907:+                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-10.md:4999:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-10.md:5003:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-10.md:5004:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-10.md:5006:+pub fn translate_tolerances(
./typed-trust-codex-review-10.md:5009:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-10.md:5010:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-10.md:5011:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-10.md:5017:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-10.md:5018:+        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-10.md:5038:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-10.md:5050:+    single_oracle: &Option<String>,
./typed-trust-codex-review-10.md:5059:+        // tolerances; allowing them to translate would let
./typed-trust-codex-review-10.md:5076:+            against: single_oracle.clone(),
./typed-trust-codex-review-10.md:5089:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-10.md:5090:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-10.md:5092:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-10.md:5107:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-10.md:5109:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-10.md:5130:+    let reruns = translate_last_verified(
./typed-trust-codex-review-10.md:5131:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-10.md:5168:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-10.md:5170:+/// - `last_verified` is absent;
./typed-trust-codex-review-10.md:5171:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-10.md:5172:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-10.md:5177:+fn translate_last_verified(
./typed-trust-codex-review-10.md:5203:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-10.md:5270:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-10.md:5271:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-10.md:5278:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-10.md:5689:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-10.md:5849:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-10.md:5867:+    tolerances:
./typed-trust-codex-review-10.md:5875:+      oracle:
./typed-trust-codex-review-10.md:5880:+    last_verified:
./typed-trust-codex-review-10.md:5901:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-10.md:6056:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-10.md:6074:+    tolerances:
./typed-trust-codex-review-10.md:6082:+      oracle:
./typed-trust-codex-review-10.md:6087:+    last_verified:
./typed-trust-codex-review-10.md:6108:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-10.md:6453:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-10.md:6475:+    // populated to the oracle name.
./typed-trust-codex-review-10.md:6521:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-10.md:6629:+               within tier-specific tolerances."
./typed-trust-codex-review-10.md:6641:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-10.md:6688:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-10.md:6705:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-10.md:6716:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-10.md:6800:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-10.md:6806:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-10.md:6916:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-10.md:6922:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-10.md:6923:+/// translation working) with last_verified populated.
./typed-trust-codex-review-10.md:6937:+    tolerances:
./typed-trust-codex-review-10.md:6945:+      oracle:
./typed-trust-codex-review-10.md:6950:+    last_verified:
./typed-trust-codex-review-10.md:6959:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-10.md:6973:+    tolerances:
./typed-trust-codex-review-10.md:6980:+      oracle:
./typed-trust-codex-review-10.md:6985:+    last_verified:
./typed-trust-codex-review-10.md:7005:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-10.md:7053:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-10.md:7055:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-10.md:7056:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-10.md:8375:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-10.md:8380:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-10.md:8381:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-10.md:8399:+    tolerances:
./typed-trust-codex-review-10.md:8407:+      oracle:
./typed-trust-codex-review-10.md:8412:+    last_verified:
./typed-trust-codex-review-10.md:8420:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-10.md:8424:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-10.md:8425:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-10.md:8426:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-10.md:8442:+    tolerances:
./typed-trust-codex-review-10.md:8450:+      oracle:
./typed-trust-codex-review-10.md:8455:+    last_verified:
./typed-trust-codex-review-10.md:8469:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-10.md:8470:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-10.md:8480:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-10.md:8482:+      oracle:
./typed-trust-codex-review-10.md:8484:+      command: pytest tests/oracle -v
./typed-trust-codex-review-10.md:8508:+    tolerances:
./typed-trust-codex-review-10.md:8528:+      oracle:
./typed-trust-codex-review-10.md:8530:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-10.md:8555:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-10.md:8576:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-10.md:8578:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-10.md:8586:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-10.md:8587:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-10.md:8592:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-10.md:8607:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-10.md:8615:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-10.md:8619:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-10.md:8641:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-10.md:8671:+    tolerances:
./typed-trust-codex-review-10.md:8677:+      oracle: [SomeOracle]
./typed-trust-codex-review-10.md:8682:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-10.md:8708:+    tolerances:
./typed-trust-codex-review-10.md:8714:+      oracle: [internal]
./typed-trust-codex-review-10.md:8715:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-10.md:8719:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-10.md:8761:+    tolerances:
./typed-trust-codex-review-10.md:8765:+      oracle: [Foo]
./typed-trust-codex-review-10.md:8766:+      command: pytest tests/oracle
./typed-trust-codex-review-10.md:8770:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-10.md:8781:+fn rejects_measurement_claim_without_tolerances() {
./typed-trust-codex-review-10.md:8782:+    // Codex round 5: kind: measurement requires non-empty tolerances per
./typed-trust-codex-review-10.md:8787:+  - id: measurement-no-tolerances
./typed-trust-codex-review-10.md:8788:+    title: missing tolerances
./typed-trust-codex-review-10.md:8795:+      oracle: [Foo]
./typed-trust-codex-review-10.md:8800:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-10.md:8803:+            assert_eq!(id, "measurement-no-tolerances");
./typed-trust-codex-review-10.md:8824:+    tolerances:
./typed-trust-codex-review-10.md:8831:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-10.md:8843:+fn rejects_measurement_claim_with_empty_tolerances_list() {
./typed-trust-codex-review-10.md:8846:+  - id: measurement-empty-tolerances
./typed-trust-codex-review-10.md:8847:+    title: empty tolerances
./typed-trust-codex-review-10.md:8853:+    tolerances: []
./typed-trust-codex-review-10.md:8855:+      oracle: [Foo]
./typed-trust-codex-review-10.md:8860:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-10.md:8878:+    tolerances:
./typed-trust-codex-review-10.md:8882:+      oracle: [Foo]
./typed-trust-codex-review-10.md:8883:+      command: pytest tests/oracle
./typed-trust-codex-review-10.md:8887:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-10.md:8908:+    tolerances:
./typed-trust-codex-review-10.md:8913:+      oracle: [Foo]
./typed-trust-codex-review-10.md:8918:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-10.md:8934:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-10.md:8937:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-10.md:8940:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-10.md:8943:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-10.md:8950:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-10.md:8979:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-10.md:8982:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-10.md:8999:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-10.md:9013:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-10.md:9027:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-10.md:9043: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-10.md:9049: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-10.md:9060:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-10.md:9082:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-10.md:9297:    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./typed-trust-codex-review-10.md:9335:        sidecar_path = manifest_path.parent / "last_verified.json"
./typed-trust-codex-review-10.md:9445:oracle binaries available.
./typed-trust-codex-review-10.md:9692:        if not score.tolerances:
./typed-trust-codex-review-10.md:9695:        first = score.tolerances[0]
./typed-trust-codex-review-10.md:9757:"""Sidecar ``last_verified.json`` read/write.
./typed-trust-codex-review-10.md:9963:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-10.md:10006:    // Overlay sidecar entries onto each claim's last_verified field
./typed-trust-codex-review-10.md:10014:                        cw.claim.last_verified = Some(lv.clone());
./typed-trust-codex-review-10.md:10054:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-10.md:10177:    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./typed-trust-codex-review-10.md:10207://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-10.md:10208://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-10.md:10209://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-10.md:10217://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-10.md:10251:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-10.md:10263:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-10.md:10266:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-10.md:10298:    pub oracle: Vec<String>,
./typed-trust-codex-review-10.md:10319:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-10.md:10328:    /// release claims must carry structured tolerances.
./typed-trust-codex-review-10.md:10330:    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-10.md:10332:    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-10.md:10366:                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-10.md:10367:                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-10.md:10687:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./typed-trust-codex-review-10.md:10701:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-10.md:10705:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-10.md:10706:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-10.md:10708:pub fn translate_tolerances(
./typed-trust-codex-review-10.md:10711:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-10.md:10712:        if e.oracle.len() == 1 {
./typed-trust-codex-review-10.md:10713:            Some(e.oracle[0].clone())
./typed-trust-codex-review-10.md:10719:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-10.md:10720:        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-10.md:10740:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-10.md:10752:    single_oracle: &Option<String>,
./typed-trust-codex-review-10.md:10761:        // tolerances; allowing them to translate would let
./typed-trust-codex-review-10.md:10778:            against: single_oracle.clone(),
./typed-trust-codex-review-10.md:10791:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-10.md:10792:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-10.md:10794:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-10.md:10809:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-10.md:10811:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-10.md:10832:    let reruns = translate_last_verified(
./typed-trust-codex-review-10.md:10833:        mc.last_verified.as_ref(),
./typed-trust-codex-review-10.md:10870:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-10.md:10872:/// - `last_verified` is absent;
./typed-trust-codex-review-10.md:10873:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-10.md:10874:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-10.md:10879:fn translate_last_verified(
./typed-trust-codex-review-10.md:10905:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-10.md:10972:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-10.md:10973:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-10.md:10980:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-10.md:11005://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-10.md:11028:///   `last_verified`).
./typed-trust-codex-review-10.md:12553:  oracle:       [...]
./typed-trust-codex-review-10.md:12575:| `oracle`           | empty — consumer must define                                       |
./typed-trust-codex-review-10.md:12598:| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
./typed-trust-codex-review-10.md:12600:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-10.md:12601:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-10.md:12604:| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-10.md:12610:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-10.md:12621:- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review-10.md:12622:  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review-10.md:12631:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-10.md:13047:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-10.md:13056:tolerances:
./typed-trust-codex-review-10.md:13095:Each named entry can be referenced by `tolerances[].output` so a single
./typed-trust-codex-review-10.md:13104:  Biopython: "1.83"              # one entry per oracle named in evidence.oracle
./typed-trust-codex-review-10.md:13108:The validator requires that every name in `evidence.oracle` appears as a
./typed-trust-codex-review-10.md:13126:underlying numbers are correct (that is what `tolerances` and
./typed-trust-codex-review-10.md:13127:`last_verified` are for). The three levels:
./typed-trust-codex-review-10.md:13129:- `automatic` — produced by a runner (CI, an oracle script, a benchmark).
./typed-trust-codex-review-10.md:13146:replace the underlying `tolerances` and `evidence` fields.
./typed-trust-codex-review-10.md:13207:        oracles = validate_evidence(claim["evidence"], claim_id, vocabularies)
./typed-trust-codex-review-10.md:13222:        if "last_verified" in claim:
./typed-trust-codex-review-10.md:13223:            validate_last_verified(claim["last_verified"], claim_id)
./typed-trust-codex-review-10.md:13240:            validate_tolerances(claim["tolerances"], claim_id, vocabularies)
./typed-trust-codex-review-10.md:13242:                claim["pinned_versions"], oracles, project, claim_id
./typed-trust-codex-review-10.md:13262:   109	        sidecar_path = manifest_path.parent / "last_verified.json"
./typed-trust-codex-review-10.md:13301:    72	    // Overlay sidecar entries onto each claim's last_verified field
./typed-trust-codex-review-10.md:13309:    80	                        cw.claim.last_verified = Some(lv.clone());
./typed-trust-codex-review-10.md:13354:   120	        let criteria = match translate_tolerances(mc) {
./EVIDENT_DESIGN_v0.3_DRAFT.md:203:enum ClaimKind {
./workflow/validate_manifest.py:6:still need their own oracle or benchmark commands.
./workflow/validate_manifest.py:42:    "oracle": set(),
./workflow/validate_manifest.py:67:    "tolerances",
./workflow/validate_manifest.py:125:    for field in ("oracle", "command", "artifact"):
./workflow/validate_manifest.py:131:            f"move tolerance text into the top-level tolerances: list "
./workflow/validate_manifest.py:134:    oracles = require_string_list(value["oracle"], "evidence.oracle", claim_id)
./workflow/validate_manifest.py:135:    for oracle_name in oracles:
./workflow/validate_manifest.py:137:            oracle_name, "oracle", vocabularies, "evidence.oracle[]", claim_id
./workflow/validate_manifest.py:141:    return oracles
./workflow/validate_manifest.py:144:def validate_tolerances(
./workflow/validate_manifest.py:148:        fail(f"claim {claim_id}: tolerances must be a non-empty list")
./workflow/validate_manifest.py:151:            fail(f"claim {claim_id}: tolerances[{i}] must be a mapping")
./workflow/validate_manifest.py:154:            fail(f"claim {claim_id}: tolerances[{i}].prose is required")
./workflow/validate_manifest.py:162:                f"claim {claim_id}: tolerances[{i}] metric/op/value are "
./workflow/validate_manifest.py:170:                f"tolerances[{i}].metric",
./workflow/validate_manifest.py:177:                f"tolerances[{i}].op",
./workflow/validate_manifest.py:183:                fail(f"claim {claim_id}: tolerances[{i}].value must be numeric")
./workflow/validate_manifest.py:206:    value: Any, oracle_names: list[str], project: str, claim_id: str
./workflow/validate_manifest.py:223:    missing_oracles = [o for o in oracle_names if o not in value]
./workflow/validate_manifest.py:224:    if missing_oracles:
./workflow/validate_manifest.py:226:            f"claim {claim_id}: pinned_versions must include every oracle "
./workflow/validate_manifest.py:227:            f"named in evidence.oracle; missing: {missing_oracles}"
./workflow/validate_manifest.py:283:def validate_last_verified(value: Any, claim_id: str) -> None:
./workflow/validate_manifest.py:285:        fail(f"claim {claim_id}: last_verified must be a mapping")
./workflow/validate_manifest.py:293:                fail(f"claim {claim_id}: last_verified.value must be numeric or null")
./workflow/validate_manifest.py:295:            fail(f"claim {claim_id}: last_verified.{key} must be a string or null")
./workflow/validate_manifest.py:419:        oracles = validate_evidence(claim["evidence"], claim_id, vocabularies)
./workflow/validate_manifest.py:434:        if "last_verified" in claim:
./workflow/validate_manifest.py:435:            validate_last_verified(claim["last_verified"], claim_id)
./workflow/validate_manifest.py:452:            validate_tolerances(claim["tolerances"], claim_id, vocabularies)
./workflow/validate_manifest.py:454:                claim["pinned_versions"], oracles, project, claim_id
./EVIDENT_PHASE5_PR6.code-review.md:115:-    sections), and asks the model to extract structured tolerances.
./EVIDENT_PHASE5_PR6.code-review.md:125:+    extract structured tolerances. Each tolerance is validated by
./EVIDENT_PHASE5_PR6.code-review.md:1559:diff --git a/evident-agent/tests/fixtures/extract/paper/wrong_subject_binding.md b/evident-agent/tests/fixtures/extract/paper/wrong_subject_binding.md
./EVIDENT_PHASE5_PR6.code-review.md:1563:+++ b/evident-agent/tests/fixtures/extract/paper/wrong_subject_binding.md
./EVIDENT_PHASE5_PR6.code-review.md:2030:+        "subject_aliases": ["our method", "we", "ours"],
./EVIDENT_PHASE5_PR6.code-review.md:2031:+        "tolerances": [
./EVIDENT_PHASE5_PR6.code-review.md:2066:+# Validator catches wrong-subject-binding
./EVIDENT_PHASE5_PR6.code-review.md:2070:+def test_end_to_end_wrong_subject_binding_paper_drops_to_zero(tmp_path: Path):
./EVIDENT_PHASE5_PR6.code-review.md:2082:+        "subject_aliases": ["our method", "we"],
./EVIDENT_PHASE5_PR6.code-review.md:2083:+        "tolerances": [
./EVIDENT_PHASE5_PR6.code-review.md:2106:+        paper_path=FIXTURES / "wrong_subject_binding.md",
./EVIDENT_PHASE5_PR6.code-review.md:2314:P3: Wrong-subject-binding test can pass for the wrong reason if the fixture changes.  
./EVIDENT_PHASE5_PR6.code-review.md:2315:`test_end_to_end_wrong_subject_binding_paper_drops_to_zero` asserts only that the claim is dropped. If a future fixture edit makes `source_span` no longer match verbatim, the validator may reject on span lookup rather than local subject binding and the test still passes. Assert the rejection reason/category, or assert the fixture contains both sentences exactly before running.
./EVIDENT_PHASE5_PR6.code-review.md:2358:P3: Wrong-subject-binding test can pass for the wrong reason if the fixture changes.  
./EVIDENT_PHASE5_PR6.code-review.md:2359:`test_end_to_end_wrong_subject_binding_paper_drops_to_zero` asserts only that the claim is dropped. If a future fixture edit makes `source_span` no longer match verbatim, the validator may reject on span lookup rather than local subject binding and the test still passes. Assert the rejection reason/category, or assert the fixture contains both sentences exactly before running.
./EVIDENT_PHASE5.codex-review-v2.md:190:  oracle: [Paper-Authority]
./EVIDENT_PHASE5.codex-review-v2.md:284:    tolerances:
./EVIDENT_PHASE5.codex-review-v2.md:299:      oracle: [Paper-Authority]
./EVIDENT_PHASE5.codex-review-v2.md:304:    last_verified:
./EVIDENT_PHASE5.codex-review-v2.md:318:  rejects tolerances without one.
./EVIDENT_PHASE5.codex-review-v2.md:509:   same source-span discipline as tolerances. Assumption strings
./EVIDENT_PHASE5.codex-review-v2.md:625:The extractor could emit `ours < 0.5`, and the validator would see both a comparator and numbers. The validator should require the comparator, bound value, metric token, and claimed subject/result to co-occur in the same local phrase or table cell/row context, not merely in the same span.
./EVIDENT_PHASE5.codex-review-v2.md:645:- `requires_human_evaluation`: replay depends on human raters or subjective evaluation.
./EVIDENT_PHASE5.codex-review-v2.md:680:Not quite ready to implement. The biggest remaining risk is still silent threshold invention, specifically false validator passes caused by comparator words appearing near the wrong number or subject. Fix the comparator coverage and local-binding rule, then v2 is ready for tight focused PRs.
./EVIDENT_PHASE5.codex-review-v2.md:727:The extractor could emit `ours < 0.5`, and the validator would see both a comparator and numbers. The validator should require the comparator, bound value, metric token, and claimed subject/result to co-occur in the same local phrase or table cell/row context, not merely in the same span.
./EVIDENT_PHASE5.codex-review-v2.md:747:- `requires_human_evaluation`: replay depends on human raters or subjective evaluation.
./EVIDENT_PHASE5.codex-review-v2.md:782:Not quite ready to implement. The biggest remaining risk is still silent threshold invention, specifically false validator passes caused by comparator words appearing near the wrong number or subject. Fix the comparator coverage and local-binding rule, then v2 is ready for tight focused PRs.
./evident-agent/tests/test_concordance.py:1:"""PR5g: tests for the concordance comparator + last_concorded sidecar.
./evident-agent/tests/test_concordance.py:13:from evident_agent import concordance, last_concorded
./evident-agent/tests/test_concordance.py:361:# last_concorded.json sidecar
./evident-agent/tests/test_concordance.py:365:def test_last_concorded_round_trip(tmp_path: Path):
./evident-agent/tests/test_concordance.py:366:    path = tmp_path / "last_concorded.json"
./evident-agent/tests/test_concordance.py:368:        "claim-a": last_concorded.LastConcordedEntry(
./evident-agent/tests/test_concordance.py:376:        "claim-b": last_concorded.LastConcordedEntry(
./evident-agent/tests/test_concordance.py:382:    last_concorded.write(path, entries)
./evident-agent/tests/test_concordance.py:383:    loaded = last_concorded.read(path)
./evident-agent/tests/test_concordance.py:392:def test_last_concorded_merge_new_wins(tmp_path: Path):
./evident-agent/tests/test_concordance.py:394:        "claim-a": last_concorded.LastConcordedEntry(comparison_status="fail"),
./evident-agent/tests/test_concordance.py:397:        "claim-a": last_concorded.LastConcordedEntry(comparison_status="pass"),
./evident-agent/tests/test_concordance.py:398:        "claim-b": last_concorded.LastConcordedEntry(
./evident-agent/tests/test_concordance.py:402:    merged = last_concorded.merge(existing, new)
./evident-agent/tests/test_concordance.py:407:def test_last_concorded_read_missing_returns_empty(tmp_path: Path):
./evident-agent/tests/test_concordance.py:409:    assert last_concorded.read(path) == {}
./evident-agent/tests/test_concordance.py:416:    path = tmp_path / "last_concorded.json"
./evident-agent/tests/test_concordance.py:418:        "rustims-fragpipe-fdr-10k-concords-meier": last_concorded.LastConcordedEntry(
./evident-agent/tests/test_concordance.py:427:    last_concorded.write(path, entries)
./evident-agent/tests/test_concordance.py:428:    loaded = last_concorded.read(path)
./evident-agent/tests/test_extract_framing.py:26:def test_tool_schema_claim_requires_subject_aliases_and_source_span():
./evident-agent/tests/test_extract_framing.py:33:    assert "subject_aliases" in claim_schema["required"]
./evident-agent/tests/test_extract_framing.py:34:    tolerance_schema = claim_schema["properties"]["tolerances"]["items"]
./evident-agent/tests/test_extract_framing.py:48:        "comparator_bound_to_wrong_subject",
./evident-agent/tests/test_extract_framing.py:73:def test_system_prompt_calls_out_wrong_subject_binding():
./evident-agent/tests/test_extract_framing.py:79:    assert "wrong subject" in prompt or "bound to" in prompt
./EVIDENT_CURATOR.code-review.md:569:+                "tolerances": [
./EVIDENT_CURATOR.code-review.md:578:+                    "oracle": ["Paper-Authority"],
./EVIDENT_CURATOR.code-review.md:602:+                "tolerances": [
./EVIDENT_CURATOR.code-review.md:611:+                    "oracle": ["Paper-Authority"],
./typed-trust-codex-review-8.md:238:+enum ClaimKind {
./typed-trust-codex-review-8.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-8.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-8.md:368:+                                        // tolerances
./typed-trust-codex-review-8.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-8.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-8.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-8.md:614:+pub enum ClaimKind {
./typed-trust-codex-review-8.md:1225:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-8.md:1292:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-8.md:1541:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-8.md:2037:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-8.md:2060:+///   `last_verified`).
./typed-trust-codex-review-8.md:2681:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-8.md:2682:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-8.md:2683:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-8.md:2691:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-8.md:2725:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-8.md:2737:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-8.md:2740:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-8.md:2772:+    pub oracle: Vec<String>,
./typed-trust-codex-review-8.md:2793:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-8.md:2802:+    /// release claims must carry structured tolerances.
./typed-trust-codex-review-8.md:2804:+    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-8.md:2806:+    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-8.md:2840:+                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-8.md:2841:+                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-8.md:2933:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-8.md:2937:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-8.md:2938:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-8.md:2940:+pub fn translate_tolerances(
./typed-trust-codex-review-8.md:2943:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-8.md:2944:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-8.md:2945:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-8.md:2951:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-8.md:2952:+        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-8.md:2972:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-8.md:2984:+    single_oracle: &Option<String>,
./typed-trust-codex-review-8.md:2993:+        // tolerances; allowing them to translate would let
./typed-trust-codex-review-8.md:3010:+            against: single_oracle.clone(),
./typed-trust-codex-review-8.md:3023:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-8.md:3024:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-8.md:3026:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-8.md:3041:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-8.md:3043:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-8.md:3064:+    let reruns = translate_last_verified(
./typed-trust-codex-review-8.md:3065:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-8.md:3102:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-8.md:3104:+/// - `last_verified` is absent;
./typed-trust-codex-review-8.md:3105:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-8.md:3106:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-8.md:3111:+fn translate_last_verified(
./typed-trust-codex-review-8.md:3137:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-8.md:3204:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-8.md:3205:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-8.md:3212:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-8.md:3479:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-8.md:3641:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-8.md:3659:+    tolerances:
./typed-trust-codex-review-8.md:3667:+      oracle:
./typed-trust-codex-review-8.md:3672:+    last_verified:
./typed-trust-codex-review-8.md:3693:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-8.md:3960:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-8.md:3982:+    // populated to the oracle name.
./typed-trust-codex-review-8.md:4028:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-8.md:4136:+               within tier-specific tolerances."
./typed-trust-codex-review-8.md:4148:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-8.md:4195:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-8.md:4212:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-8.md:4223:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-8.md:4307:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-8.md:4313:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-8.md:4423:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-8.md:4429:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-8.md:4430:+/// translation working) with last_verified populated.
./typed-trust-codex-review-8.md:4444:+    tolerances:
./typed-trust-codex-review-8.md:4452:+      oracle:
./typed-trust-codex-review-8.md:4457:+    last_verified:
./typed-trust-codex-review-8.md:4466:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-8.md:4480:+    tolerances:
./typed-trust-codex-review-8.md:4487:+      oracle:
./typed-trust-codex-review-8.md:4492:+    last_verified:
./typed-trust-codex-review-8.md:4512:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-8.md:4560:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-8.md:4562:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-8.md:4563:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-8.md:5882:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-8.md:5887:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-8.md:5888:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-8.md:5906:+    tolerances:
./typed-trust-codex-review-8.md:5914:+      oracle:
./typed-trust-codex-review-8.md:5919:+    last_verified:
./typed-trust-codex-review-8.md:5927:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-8.md:5931:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-8.md:5932:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-8.md:5933:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-8.md:5949:+    tolerances:
./typed-trust-codex-review-8.md:5957:+      oracle:
./typed-trust-codex-review-8.md:5962:+    last_verified:
./typed-trust-codex-review-8.md:5976:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-8.md:5977:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-8.md:5987:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-8.md:5989:+      oracle:
./typed-trust-codex-review-8.md:5991:+      command: pytest tests/oracle -v
./typed-trust-codex-review-8.md:6015:+    tolerances:
./typed-trust-codex-review-8.md:6035:+      oracle:
./typed-trust-codex-review-8.md:6037:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-8.md:6062:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-8.md:6083:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-8.md:6085:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-8.md:6093:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-8.md:6094:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-8.md:6099:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-8.md:6114:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-8.md:6122:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-8.md:6126:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-8.md:6148:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-8.md:6178:+    tolerances:
./typed-trust-codex-review-8.md:6184:+      oracle: [SomeOracle]
./typed-trust-codex-review-8.md:6189:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-8.md:6215:+    tolerances:
./typed-trust-codex-review-8.md:6221:+      oracle: [internal]
./typed-trust-codex-review-8.md:6222:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-8.md:6226:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-8.md:6268:+    tolerances:
./typed-trust-codex-review-8.md:6272:+      oracle: [Foo]
./typed-trust-codex-review-8.md:6273:+      command: pytest tests/oracle
./typed-trust-codex-review-8.md:6277:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-8.md:6288:+fn rejects_measurement_claim_without_tolerances() {
./typed-trust-codex-review-8.md:6289:+    // Codex round 5: kind: measurement requires non-empty tolerances per
./typed-trust-codex-review-8.md:6294:+  - id: measurement-no-tolerances
./typed-trust-codex-review-8.md:6295:+    title: missing tolerances
./typed-trust-codex-review-8.md:6302:+      oracle: [Foo]
./typed-trust-codex-review-8.md:6307:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-8.md:6310:+            assert_eq!(id, "measurement-no-tolerances");
./typed-trust-codex-review-8.md:6331:+    tolerances:
./typed-trust-codex-review-8.md:6338:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-8.md:6350:+fn rejects_measurement_claim_with_empty_tolerances_list() {
./typed-trust-codex-review-8.md:6353:+  - id: measurement-empty-tolerances
./typed-trust-codex-review-8.md:6354:+    title: empty tolerances
./typed-trust-codex-review-8.md:6360:+    tolerances: []
./typed-trust-codex-review-8.md:6362:+      oracle: [Foo]
./typed-trust-codex-review-8.md:6367:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-8.md:6385:+    tolerances:
./typed-trust-codex-review-8.md:6389:+      oracle: [Foo]
./typed-trust-codex-review-8.md:6390:+      command: pytest tests/oracle
./typed-trust-codex-review-8.md:6394:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-8.md:6415:+    tolerances:
./typed-trust-codex-review-8.md:6420:+      oracle: [Foo]
./typed-trust-codex-review-8.md:6425:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-8.md:6441:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-8.md:6444:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-8.md:6447:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-8.md:6450:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-8.md:6457:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-8.md:6486:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-8.md:6489:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-8.md:6506:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-8.md:6520:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-8.md:6534:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-8.md:6550: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-8.md:6556: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-8.md:6567:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-8.md:6589:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-8.md:6689://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-8.md:6690://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-8.md:6691://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-8.md:6699://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-8.md:6733:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-8.md:6745:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-8.md:6748:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-8.md:6780:    pub oracle: Vec<String>,
./typed-trust-codex-review-8.md:6801:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-8.md:6810:    /// release claims must carry structured tolerances.
./typed-trust-codex-review-8.md:6812:    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-8.md:6814:    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-8.md:6848:                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-8.md:6849:                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-8.md:6903://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-8.md:6926:///   `last_verified`).
./typed-trust-codex-review-8.md:7166:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-8.md:7170:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-8.md:7171:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-8.md:7173:pub fn translate_tolerances(
./typed-trust-codex-review-8.md:7176:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-8.md:7177:        if e.oracle.len() == 1 {
./typed-trust-codex-review-8.md:7178:            Some(e.oracle[0].clone())
./typed-trust-codex-review-8.md:7184:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-8.md:7185:        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-8.md:7205:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-8.md:7217:    single_oracle: &Option<String>,
./typed-trust-codex-review-8.md:7226:        // tolerances; allowing them to translate would let
./typed-trust-codex-review-8.md:7243:            against: single_oracle.clone(),
./typed-trust-codex-review-8.md:7256:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-8.md:7257:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-8.md:7259:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-8.md:7274:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-8.md:7276:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-8.md:7297:    let reruns = translate_last_verified(
./typed-trust-codex-review-8.md:7298:        mc.last_verified.as_ref(),
./typed-trust-codex-review-8.md:7335:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-8.md:7337:/// - `last_verified` is absent;
./typed-trust-codex-review-8.md:7338:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-8.md:7339:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-8.md:7344:fn translate_last_verified(
./typed-trust-codex-review-8.md:7370:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-8.md:7743:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-8.md:7744:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-8.md:7751:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-8.md:7919:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-8.md:7986:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-8.md:8671:87-| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
./typed-trust-codex-review-8.md:8673:89-| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-8.md:8674:90:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-8.md:8677:93-| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-8.md:8683:99-omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-8.md:8694:110-- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review-8.md:8695:111:  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review-8.md:8704:120-  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-8.md:8713:129-tolerances:
./typed-trust-codex-review-8.md:8752:168-Each named entry can be referenced by `tolerances[].output` so a single
./typed-trust-codex-review-8.md:8761:177:  Biopython: "1.83"              # one entry per oracle named in evidence.oracle
./typed-trust-codex-review-8.md:8765:181:The validator requires that every name in `evidence.oracle` appears as a
./typed-trust-codex-review-8.md:8999:42-  oracle:       [...]
./typed-trust-codex-review-8.md:9021:64-| `oracle`           | empty — consumer must define                                       |
./typed-trust-codex-review-8.md:9041:110:- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review-8.md:9042:111-  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review-8.md:9051:120-  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-8.md:9060:129-tolerances:
./typed-trust-codex-review-8.md:9071:292-AND oracle ∩ {OpenMM, BALL} != ∅
./typed-trust-codex-review-8.md:9086:307-1. Lift `evidence.tolerance` into one or more `tolerances:` entries.
./typed-trust-codex-review-8.md:9091:312-   measurements (and they may then drop `subsystem` and `tolerances`).
./evident-agent/tests/test_review.py:454:        "tolerances": [
./evident-agent/tests/test_review.py:463:            "oracle": ["BALL"],
./evident-agent/tests/test_review.py:503:    assert backing["tolerances"][0]["op"] == ">="
./evident-agent/tests/test_review.py:504:    assert backing["tolerances"][0]["value"] == 0.02
./evident-agent/tests/test_review.py:505:    # last_verified.value = violation.observed_value (so backing sustains).
./evident-agent/tests/test_review.py:506:    assert backing["last_verified"]["value"] == 0.025
./evident-agent/tests/test_review.py:507:    assert backing["last_verified"]["date"]
./evident-agent/tests/test_sidecar.py:16:    path = tmp_path / "last_verified.json"
./evident-agent/tests/test_cli_dry_run.py:28:                tolerances:
./evident-agent/tests/test_cli_dry_run.py:34:                  oracle: [Test]
./evident-agent/tests/test_cli_dry_run.py:46:    sidecar = tmp_path / "last_verified.json"
./evident-agent/tests/test_cli_dry_run.py:70:    sidecar = tmp_path / "last_verified.json"
./typed-trust-codex-review.md:238:+enum ClaimKind {
./typed-trust-codex-review.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review.md:368:+                                        // tolerances
./typed-trust-codex-review.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review.md:614:+pub enum ClaimKind {
./typed-trust-codex-review.md:995:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review.md:1064:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review.md:1127:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review.md:1588:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review.md:1611:+///   `last_verified`).
./typed-trust-codex-review.md:1948:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review.md:1949:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review.md:1950:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review.md:1958:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review.md:1988:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review.md:2000:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review.md:2003:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review.md:2031:+    pub oracle: Vec<String>,
./typed-trust-codex-review.md:2052:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review.md:2143:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review.md:2147:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review.md:2148:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review.md:2150:+pub fn translate_tolerances(
./typed-trust-codex-review.md:2153:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review.md:2154:+        if e.oracle.len() == 1 {
./typed-trust-codex-review.md:2155:+            Some(e.oracle[0].clone())
./typed-trust-codex-review.md:2161:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review.md:2169:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review.md:2177:+    single_oracle: &Option<String>,
./typed-trust-codex-review.md:2185:+        against: single_oracle.clone(),
./typed-trust-codex-review.md:2194:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review.md:2195:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review.md:2197:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review.md:2212:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review.md:2214:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review.md:2224:+    let reruns = translate_last_verified(
./typed-trust-codex-review.md:2225:+        mc.last_verified.as_ref(),
./typed-trust-codex-review.md:2262:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review.md:2264:+/// - `last_verified` is absent;
./typed-trust-codex-review.md:2265:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review.md:2266:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review.md:2271:+fn translate_last_verified(
./typed-trust-codex-review.md:2297:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review.md:2364:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review.md:2365:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review.md:2372:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review.md:2568:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review.md:2879:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review.md:2901:+    // populated to the oracle name.
./typed-trust-codex-review.md:2947:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review.md:3055:+               within tier-specific tolerances."
./typed-trust-codex-review.md:3067:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review.md:3114:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review.md:3131:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review.md:3142:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review.md:3226:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review.md:3232:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review.md:3342:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review.md:3348:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review.md:3349:+/// translation working) with last_verified populated.
./typed-trust-codex-review.md:3363:+    tolerances:
./typed-trust-codex-review.md:3371:+      oracle:
./typed-trust-codex-review.md:3376:+    last_verified:
./typed-trust-codex-review.md:3385:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review.md:3399:+    tolerances:
./typed-trust-codex-review.md:3406:+      oracle:
./typed-trust-codex-review.md:3411:+    last_verified:
./typed-trust-codex-review.md:3431:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review.md:3475:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review.md:3477:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review.md:3478:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review.md:3880:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review.md:3885:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review.md:3886:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review.md:3904:+    tolerances:
./typed-trust-codex-review.md:3912:+      oracle:
./typed-trust-codex-review.md:3917:+    last_verified:
./typed-trust-codex-review.md:3925:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review.md:3929:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review.md:3930:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review.md:3931:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review.md:3947:+    tolerances:
./typed-trust-codex-review.md:3955:+      oracle:
./typed-trust-codex-review.md:3960:+    last_verified:
./typed-trust-codex-review.md:3974:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review.md:3975:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review.md:3985:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review.md:3987:+      oracle:
./typed-trust-codex-review.md:3989:+      command: pytest tests/oracle -v
./typed-trust-codex-review.md:4013:+    tolerances:
./typed-trust-codex-review.md:4033:+      oracle:
./typed-trust-codex-review.md:4035:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review.md:4060:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review.md:4081:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review.md:4083:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review.md:4091:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review.md:4092:+    // entry in `evidence.oracle`.
./typed-trust-codex-review.md:4097:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review.md:4112:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review.md:4120:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review.md:4124:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review.md:4142:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review.md:4169:+    tolerances:
./typed-trust-codex-review.md:4175:+      oracle: [SomeOracle]
./typed-trust-codex-review.md:4180:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review.md:4198:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review.md:4201:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review.md:4204:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review.md:4207:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review.md:4214:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review.md:4243:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review.md:4246:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review.md:4263:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review.md:4277:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review.md:4291:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review.md:4307: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review.md:4313: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review.md:4324:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review.md:4346:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review.md:4440://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review.md:4463:///   `last_verified`).
./typed-trust-codex-review.md:4706://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review.md:4707://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review.md:4708://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review.md:4716://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review.md:4746:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review.md:4758:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review.md:4761:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review.md:4789:    pub oracle: Vec<String>,
./typed-trust-codex-review.md:4810:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review.md:4901:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review.md:4905:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review.md:4906:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review.md:4908:pub fn translate_tolerances(
./typed-trust-codex-review.md:4911:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review.md:4912:        if e.oracle.len() == 1 {
./typed-trust-codex-review.md:4913:            Some(e.oracle[0].clone())
./typed-trust-codex-review.md:4919:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review.md:4927:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review.md:4940:    single_oracle: &Option<String>,
./typed-trust-codex-review.md:4948:        against: single_oracle.clone(),
./typed-trust-codex-review.md:4957:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review.md:4958:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review.md:4960:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review.md:4975:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review.md:4977:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review.md:4987:    let reruns = translate_last_verified(
./typed-trust-codex-review.md:4988:        mc.last_verified.as_ref(),
./typed-trust-codex-review.md:5025:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review.md:5027:/// - `last_verified` is absent;
./typed-trust-codex-review.md:5028:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review.md:5029:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review.md:5034:fn translate_last_verified(
./typed-trust-codex-review.md:5060:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review.md:5127:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review.md:5128:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review.md:5135:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review.md:5263:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review.md:5332:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review.md:6137:90:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review.md:6140:93-| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review.md:6146:99-omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review.md:6157:110-- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review.md:6158:111:  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review.md:6167:120-  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review.md:6176:129-tolerances:
./typed-trust-codex-review.md:6200:177:  Biopython: "1.83"              # one entry per oracle named in evidence.oracle
./typed-trust-codex-review.md:6204:181:The validator requires that every name in `evidence.oracle` appears as a
./typed-trust-codex-review.md:6227:    96	    pub oracle: Vec<String>,
./typed-trust-codex-review.md:6248:   117	    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review.md:6266:    65	    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review.md:6269:    68	    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review.md:6296:The new typed-trust implementation rejects schema-valid prose-only tolerances and can compute incorrect render statuses for challenged reports. These issues can affect valid manifests and public library output.
./typed-trust-codex-review.md:6301:  The schema allows tolerance entries with only `prose` when `metric`/`op`/`value` are intentionally omitted, but these fields are required here during YAML deserialization. A valid manifest such as a research-tier deferred-spec claim with `tolerances: [{ prose: ... }]` will fail `parse_manifest_file` and make the CLI reject the entire manifest before translation can mark the criterion as unassessed or prose-only.
./typed-trust-codex-review.md:6308:The new typed-trust implementation rejects schema-valid prose-only tolerances and can compute incorrect render statuses for challenged reports. These issues can affect valid manifests and public library output.
./typed-trust-codex-review.md:6313:  The schema allows tolerance entries with only `prose` when `metric`/`op`/`value` are intentionally omitted, but these fields are required here during YAML deserialization. A valid manifest such as a research-tier deferred-spec claim with `tolerances: [{ prose: ... }]` will fail `parse_manifest_file` and make the CLI reject the entire manifest before translation can mark the criterion as unassessed or prose-only.
./evident-agent/tests/test_violation.py:12:  is the logical inverse of the target's, whose last_verified.value
./evident-agent/tests/test_violation.py:39:        "tolerances": [
./evident-agent/tests/test_violation.py:48:            "oracle": ["BALL"],
./evident-agent/tests/test_violation.py:80:    tol = backing["tolerances"][0]
./evident-agent/tests/test_violation.py:85:    # last_verified.value = observed; satisfies the inverse tolerance.
./evident-agent/tests/test_violation.py:86:    assert backing["last_verified"]["value"] == 0.025
./evident-agent/tests/test_violation.py:87:    assert backing["last_verified"]["value"] >= tol["value"]
./evident-agent/tests/test_violation.py:88:    # Date is present so typed-trust's translate_last_verified binds.
./evident-agent/tests/test_violation.py:89:    assert backing["last_verified"]["date"]
./evident-agent/tests/test_violation.py:127:    with pytest.raises(ViolationRejected, match="not found in target tolerances"):
./evident-agent/tests/test_violation.py:135:    target["tolerances"][0]["op"] = None
./evident-agent/tests/test_violation.py:136:    target["tolerances"][0]["value"] = None
./evident-agent/tests/test_violation.py:163:    target["tolerances"][0]["op"] = "="
./evident-agent/tests/test_violation.py:243:    target["tolerances"][0]["op"] = "<="
./evident-agent/tests/test_violation.py:272:    """A claim with two tolerances on the same metric, distinguished by
./evident-agent/tests/test_violation.py:282:        "tolerances": [
./evident-agent/tests/test_violation.py:298:        "evidence": {"oracle": ["BALL"], "command": "x", "artifact": "x"},
./evident-agent/tests/test_violation.py:373:    """Two tolerances share metric=relative_error; the violation against
./evident-agent/tests/test_violation.py:434:    target["tolerances"][0]["op"] = target_op
./evident-agent/tests/test_violation.py:435:    target["tolerances"][0]["value"] = bound
./evident-agent/tests/test_violation.py:444:    assert backing["tolerances"][0]["op"] == inverse_op
./evident-agent/tests/test_violation.py:445:    assert backing["last_verified"]["value"] == observed
./EVIDENT_AGENT_PHASE2D.code-review.md:2439://! observed values against tolerances, applies the §8 rule for
./EVIDENT_AGENT_PHASE2D.code-review.md:2462:///   `last_verified`).
./evident-agent/tests/test_extract_metadata.py:11:  provenance + metadata block (no tolerances/evidence.command)
./evident-agent/tests/test_extract_metadata.py:179:        assert c["kind"] == "metadata_compatibility"
./evident-agent/tests/test_extract_metadata.py:182:        # Critical: no tolerances, no evidence.command — those
./evident-agent/tests/test_extract_metadata.py:185:        assert "tolerances" not in c
./evident-agent/tests/test_extract_metadata.py:282:        c["kind"] == "metadata_compatibility" for c in manifest["claims"]
./evident-agent/tests/test_extract_validator.py:5:metric, comparator, bound, AND subject all co-occur in the same local
./evident-agent/tests/test_extract_validator.py:10:claimed for the claimed subject.
./evident-agent/tests/test_extract_validator.py:33:    metric, and subject all in the same sentence."""
./evident-agent/tests/test_extract_validator.py:43:    subject_aliases = ["we", "our", "ours", "the proposed method"]
./evident-agent/tests/test_extract_validator.py:44:    validate_tolerance(tolerance, subject_aliases=subject_aliases)
./evident-agent/tests/test_extract_validator.py:54:    validate_tolerance(tolerance, subject_aliases=["our", "we", "ours"])
./evident-agent/tests/test_extract_validator.py:67:    validate_tolerance(tolerance, subject_aliases=["we", "our"])
./evident-agent/tests/test_extract_validator.py:75:def test_wrong_subject_binding_is_rejected():
./evident-agent/tests/test_extract_validator.py:80:    *baseline* subject, not the claimed subject.
./evident-agent/tests/test_extract_validator.py:92:            tolerance, subject_aliases=["our method", "we", "ours"]
./evident-agent/tests/test_extract_validator.py:94:    assert exc.value.kind == "comparator_bound_to_wrong_subject"
./evident-agent/tests/test_extract_validator.py:119:            tolerance, subject_aliases=["our", "we", "ours"]
./evident-agent/tests/test_extract_validator.py:122:        "comparator_bound_to_wrong_subject",
./evident-agent/tests/test_extract_validator.py:129:# Per-element rejection: missing metric, comparator, value, or subject
./evident-agent/tests/test_extract_validator.py:141:        validate_tolerance(tolerance, subject_aliases=["we", "our"])
./evident-agent/tests/test_extract_validator.py:155:            tolerance, subject_aliases=["our method", "we"]
./evident-agent/tests/test_extract_validator.py:159:        "comparator_bound_to_wrong_subject",
./evident-agent/tests/test_extract_validator.py:172:            tolerance, subject_aliases=["our", "we"]
./evident-agent/tests/test_extract_validator.py:176:        "comparator_bound_to_wrong_subject",
./evident-agent/tests/test_extract_validator.py:180:def test_missing_subject_is_rejected():
./evident-agent/tests/test_extract_validator.py:181:    """If the source span never mentions the claimed subject, the
./evident-agent/tests/test_extract_validator.py:193:            subject_aliases=["our method", "our system", "we propose"],
./evident-agent/tests/test_extract_validator.py:195:    assert exc.value.kind == "missing_subject"
./evident-agent/tests/test_extract_validator.py:246:    # Should NOT raise. The bound, comparator, and subject are all in
./evident-agent/tests/test_extract_validator.py:248:    validate_tolerance(tolerance, subject_aliases=["our method", "we"])
./evident-agent/tests/test_extract_validator.py:283:        tolerance, subject_aliases=["our system", "we", "our"]
./evident-agent/tests/test_extract_validator.py:315:            tolerance, subject_aliases=["our method", "we"]
./evident-agent/tests/test_extract_validator.py:335:            tolerance, subject_aliases=["our", "we"]
./evident-agent/tests/test_extract_validator.py:341:# if the bound + subject co-occur in ONE of them
./evident-agent/tests/test_extract_validator.py:361:            tolerance, subject_aliases=["our method"]
./evident-agent/tests/test_extract_validator.py:376:        validate_tolerance(tolerance, subject_aliases=["our"])
./evident-agent/tests/test_extract_validator.py:390:        tolerance, subject_aliases=["our method"]
./evident-agent/tests/test_extract_validator.py:404:        tolerance, subject_aliases=["our method"]
./evident-agent/tests/test_extract_validator.py:416:        # present, the subject is present, the metric is present —
./evident-agent/tests/test_extract_validator.py:425:            tolerance, subject_aliases=["our method"]
./evident-agent/tests/test_extract_validator.py:429:        "comparator_bound_to_wrong_subject",
./evident-agent/tests/test_extract_validator.py:445:            tolerance, subject_aliases=["our"]
./evident-agent/tests/test_extract_validator.py:449:        "comparator_bound_to_wrong_subject",
./evident-agent/tests/test_extract_validator.py:465:            tolerance, subject_aliases=["our"]
./evident-agent/tests/test_extract_validator.py:469:        "comparator_bound_to_wrong_subject",
./evident-agent/tests/test_extract_validator.py:491:        validate_tolerance(tolerance, subject_aliases=["ours"])
./evident-agent/tests/test_extract_validator.py:492:    assert exc.value.kind == "comparator_bound_to_wrong_subject"
./evident-agent/tests/test_extract_validator.py:496:    """Counterpart: when the comparator, bound, metric, and subject
./evident-agent/tests/test_extract_validator.py:505:        tolerance, subject_aliases=["our method"]
./evident-agent/tests/test_extract_validator.py:509:def test_subject_alias_with_trailing_punctuation_anchors_correctly():
./evident-agent/tests/test_extract_validator.py:510:    """Codex F-PR4-CR2b: subject aliases ending in non-word chars
./evident-agent/tests/test_extract_validator.py:524:        tolerance, subject_aliases=["ABRA-2.0", "ours"]
./evident-agent/tests/test_extract_validator.py:530:    has all four (metric, comparator, value, subject), the tolerance
./evident-agent/tests/test_extract_validator.py:543:        tolerance, subject_aliases=["our method", "we", "ours"]
./evident-agent/examples/proteon_sasa_release.sh:19:#  - Writes a sidecar entry in workflow/evident.py's last_verified.json
./evident-agent/examples/proteon_sasa_release.sh:28:SIDECAR="/scratch/TMAlign/proteon/evident/last_verified.json"
./evident-agent/tests/test_extract_paper_cli.py:109:        "subject_aliases": ["our method", "we", "ours"],
./evident-agent/tests/test_extract_paper_cli.py:110:        "tolerances": [
./evident-agent/tests/test_extract_paper_cli.py:145:# Validator catches wrong-subject-binding
./evident-agent/tests/test_extract_paper_cli.py:149:def test_end_to_end_wrong_subject_binding_paper_drops_to_zero(tmp_path: Path):
./evident-agent/tests/test_extract_paper_cli.py:161:        "subject_aliases": ["our method", "we"],
./evident-agent/tests/test_extract_paper_cli.py:162:        "tolerances": [
./evident-agent/tests/test_extract_paper_cli.py:185:    fixture_text = (FIXTURES / "wrong_subject_binding.md").read_text()
./evident-agent/tests/test_extract_paper_cli.py:192:        paper_path=FIXTURES / "wrong_subject_binding.md",
./evident-agent/tests/test_extract_paper_cli.py:198:    # The rejection MUST be a comparator_bound_to_wrong_subject —
./evident-agent/tests/test_extract_paper_cli.py:206:    assert matching[0].reason == "comparator_bound_to_wrong_subject"
./evident-agent/tests/test_extract_paper_cli.py:385:    candidate claim with tolerances, source spans, prose, and any
./evident-agent/tests/test_extract_paper_cli.py:399:                "subject_aliases": ["our method", "we"],
./evident-agent/tests/test_extract_paper_cli.py:400:                "tolerances": [
./evident-agent/tests/test_extract_paper_cli.py:409:                        "prose": "subject-conflated bound",
./evident-agent/tests/test_extract_paper_cli.py:443:        tool_input["claims"][0]["tolerances"][0]["source_span"]
./evident-agent/tests/test_rephrase.py:45:                "tolerances": [
./evident-agent/tests/test_rephrase.py:54:                    "oracle": ["Paper-Authority"],
./evident-agent/tests/test_rephrase.py:110:        parsed["tolerances"][0]["prose"] = "rephrased tolerance prose"
./evident-agent/tests/test_rephrase.py:118:    assert sorted(result.fields_changed) == ["claim", "tolerances"]
./evident-agent/tests/test_rephrase.py:262:    """Codex F-REPHRASE-CR1 P1: a curator adding ``last_verified:
./evident-agent/tests/test_rephrase.py:271:        assert "last_verified" not in parsed
./evident-agent/tests/test_rephrase.py:272:        parsed["last_verified"] = None  # add null
./evident-agent/tests/test_rephrase.py:281:    assert "last_verified" in str(exc.value)
./evident-agent/tests/test_extract_cli.py:10:'model emits clean output' path and the 'model emits bad tolerances
./evident-agent/tests/test_extract_cli.py:157:        "subject_aliases": ["our system", "we"],
./evident-agent/tests/test_extract_cli.py:158:        "tolerances": [
./evident-agent/tests/test_extract_cli.py:181:        "subject_aliases": ["our system"],
./evident-agent/tests/test_extract_cli.py:182:        "tolerances": [
./evident-agent/tests/test_extract_cli.py:215:def test_process_tool_response_drops_claim_with_zero_valid_tolerances():
./evident-agent/tests/test_extract_cli.py:216:    """Codex v3: if all tolerances on a claim fail validation, the
./evident-agent/tests/test_extract_cli.py:310:    """Path (b): model emits two marketing-language tolerances the
./evident-agent/tests/test_extract_cli.py:322:        "subject_aliases": ["our system", "blazestack"],
./evident-agent/tests/test_extract_cli.py:323:        "tolerances": [
./evident-agent/tests/test_extract_cli.py:338:        "subject_aliases": ["our system", "blazestack"],
./evident-agent/tests/test_extract_cli.py:339:        "tolerances": [
./evident-agent/tests/test_extract_cli.py:360:    # Both bad tolerances appear in the rejections list.
./evident-agent/tests/test_extract_cli.py:377:        "subject_aliases": ["our system"],
./evident-agent/tests/test_extract_cli.py:378:        "tolerances": [
./evident-agent/tests/test_extract_cli.py:395:        "subject_aliases": ["our system"],
./evident-agent/tests/test_extract_cli.py:396:        "tolerances": [
./evident-agent/tests/test_extract_cli.py:423:    spans = {c.tolerances[0]["source_span"] for c in result.claims}
./evident-agent/tests/test_extract_cli.py:445:        "subject_aliases": ["we", "our system"],
./evident-agent/tests/test_extract_cli.py:446:        "tolerances": [
./evident-agent/tests/test_curator.py:41:                "tolerances": [
./evident-agent/tests/test_curator.py:50:                    "oracle": ["Paper-Authority"],
./evident-agent/tests/test_curator.py:74:                "tolerances": [
./evident-agent/tests/test_curator.py:83:                    "oracle": ["Paper-Authority"],
./evident-agent/tests/test_extract_typed_trust_roundtrip.py:68:    tolerances:
./evident-agent/tests/test_extract_typed_trust_roundtrip.py:74:      oracle: [Manual]
./evident-agent/tests/test_extract_typed_trust_roundtrip.py:114:                subject_aliases=["our method", "we"],
./evident-agent/tests/test_extract_typed_trust_roundtrip.py:115:                tolerances=[
./evident-agent/tests/test_extract_render.py:45:                subject_aliases=["our method", "we", "ours"],
./evident-agent/tests/test_extract_render.py:46:                tolerances=[
./evident-agent/tests/test_extract_render.py:130:    t = manifest["claims"][0]["tolerances"][0]
./evident-agent/tests/test_extract_render.py:141:    v3: default-deny means honest emptiness > invented tolerances)."""
./evident-agent/tests/test_review_walkthrough.py:46:                "tolerances": [
./evident-agent/tests/test_review_walkthrough.py:55:                    "oracle": ["Paper-Authority"],
./evident-agent/tests/test_review_walkthrough.py:79:                "tolerances": [
./evident-agent/tests/test_review_walkthrough.py:88:                    "oracle": ["Paper-Authority"],
./evident-agent/tests/test_review_walkthrough.py:112:                "tolerances": [
./evident-agent/tests/test_review_walkthrough.py:121:                    "oracle": ["Paper-Authority"],
./evident-agent/tests/test_required_fixtures.py:112:                tolerances:
./evident-agent/tests/test_required_fixtures.py:118:                  oracle: [Test]
./evident-agent/tests/test_required_fixtures.py:173:        "tolerances": [
./evident-agent/tests/test_required_fixtures.py:188:            "oracle": ["Test"],
./evident-agent/tests/test_required_fixtures.py:501:        "tolerances": [
./evident-agent/tests/test_required_fixtures.py:510:            "oracle": ["BALL"],
./evident-agent/tests/test_required_fixtures.py:585:    target["last_verified"] = {
./evident-agent/tests/test_required_fixtures.py:600:        "    tolerances:\n"
./evident-agent/tests/test_required_fixtures.py:606:        "      oracle: [BALL]\n"
./evident-agent/tests/test_required_fixtures.py:609:        "    last_verified:\n"
./evident-agent/tests/test_required_fixtures.py:690:        "    tolerances:\n"
./evident-agent/tests/test_required_fixtures.py:696:        "      oracle: [BALL]\n"
./evident-agent/tests/test_required_fixtures.py:762:        "    tolerances:\n"
./evident-agent/tests/test_required_fixtures.py:768:        "      oracle: [BALL]\n"
./evident-agent/tests/test_required_fixtures.py:807:                                "tolerances": [
./evident-agent/tests/test_required_fixtures.py:816:                                    "oracle": ["BALL"],
./evident-agent/tests/test_required_fixtures.py:820:                                "last_verified": {
./evident-agent/tests/test_review_sidecar.py:197:                "tolerances": [
./evident-agent/tests/test_review_sidecar.py:200:                "evidence": {"oracle": ["BALL"], "command": "pytest", "artifact": "x"},
./evident-agent/tests/test_review_sidecar.py:201:                "last_verified": {"date": "2026-06-02", "value": 0.025},
./evident-agent/tests/test_review_sidecar.py:238:    assert e["challenge"]["backing_claim"]["tolerances"][0]["op"] == ">="
./evident-agent/tests/test_review_sidecar.py:265:        "    tolerances:\n"
./evident-agent/tests/test_review_sidecar.py:271:        "      oracle: [BALL]\n"
./evident-agent/tests/test_review_sidecar.py:326:        "    tolerances:\n"
./evident-agent/tests/test_review_sidecar.py:332:        "      oracle: [Test]\n"
./evident-agent/tests/test_cli_review.py:39:                tolerances:
./evident-agent/tests/test_cli_review.py:45:                  oracle: [Test]
./evident-agent/tests/test_cli_review.py:103:def test_last_verified_commit_reaches_the_digest_header(tmp_path: Path) -> None:
./evident-agent/tests/test_cli_review.py:105:    from the last_verified sidecar so the reproducible_chain check
./evident-agent/tests/test_cli_review.py:117:    last_verified = tmp_path / "last_verified.json"
./evident-agent/tests/test_cli_review.py:118:    last_verified.write_text(
./evident-agent/tests/test_cli_review.py:137:    entries = sidecar_mod.read(last_verified)
./evident-agent/tests/test_cli_review.py:140:        {"last_verified": None}, entries.get("claim-A")
./evident-agent/tests/test_cli_review.py:323:                tolerances:
./EVIDENT_AGENT_PHASE2.codex-review.md:24:`last_verified.json` by running the cited verification command in
./EVIDENT_AGENT_PHASE2.codex-review.md:58:1. Agent reads claim YAML + rendered evidence + last_verified.
./EVIDENT_AGENT_PHASE2.codex-review.md:91:evidence and last_verified, and emits either:
./EVIDENT_AGENT_PHASE2.codex-review.md:207:- The full claim YAML (tier, tolerance, evidence pointer, last_verified).
./EVIDENT_AGENT_PHASE2.codex-review.md:298:   b. Read claim's last_verified (from sidecar if present, else
./evident-agent/evident_agent/review_walkthrough.py:169:    for i, t in enumerate(claim.get("tolerances") or []):
./evident-agent/evident_agent/review_sidecar.py:3:Mirrors Phase 1's ``sidecar.py`` (last_verified) but with two extra
./evident-agent/evident_agent/violation.py:21:  is the target's bound; `last_verified.value` is the violation's
./evident-agent/evident_agent/violation.py:73:      target's ``tolerances`` list (no inventing criteria).
./evident-agent/evident_agent/violation.py:97:    tolerances = target_claim.get("tolerances") or []
./evident-agent/evident_agent/violation.py:100:        for t in tolerances
./evident-agent/evident_agent/violation.py:105:            f"target_criterion_id {target_criterion_id!r} not found in target tolerances"
./evident-agent/evident_agent/violation.py:112:        # Multiple tolerances share this metric; disambiguate by the
./evident-agent/evident_agent/violation.py:128:                f"target has {len(metric_matches)} tolerances on metric "
./evident-agent/evident_agent/violation.py:134:                f"target has {len(disambiguated)} tolerances on metric "
./evident-agent/evident_agent/violation.py:223:      observed_value as ``last_verified.value``, the backing's
./evident-agent/evident_agent/violation.py:227:      command, oracle. The Challenge cites a row of the same
./evident-agent/evident_agent/violation.py:229:    - Includes ``last_verified.date`` (today's date in UTC) AND
./evident-agent/evident_agent/violation.py:230:      ``last_verified.value``. Both fields are required for
./evident-agent/evident_agent/violation.py:231:      typed-trust's ``translate_last_verified`` to bind the
./evident-agent/evident_agent/violation.py:269:        "tolerances": [
./evident-agent/evident_agent/violation.py:282:        "last_verified": {
./evident-agent/evident_agent/violation.py:381:    artifact, command, oracle. The Challenge cites a row of that same
./evident-agent/evident_agent/violation.py:385:        "oracle": list(target_evidence.get("oracle") or []),
./evident-agent/evident_agent/scoring.py:40:        if not score.tolerances:
./evident-agent/evident_agent/scoring.py:43:        first = score.tolerances[0]
./evident-agent/evident_agent/curator.py:138:    "tolerances",
./evident-agent/evident_agent/curator.py:154:    "last_verified",
./evident-agent/evident_agent/curator.py:540:      ``last_verified`` must not change. Those fields require typed
./evident-agent/evident_agent/curator.py:641:# add `last_verified: null` to bypass the lock. Comparing via
./evident-agent/evident_agent/curator.py:653:    Absent-vs-null distinction: a curator inserting ``last_verified:
./evident-agent/tests/fixtures/adversarial/sasa_endorse/evident.yaml:15:    tolerances:
./evident-agent/tests/fixtures/adversarial/sasa_endorse/evident.yaml:23:      oracle: [Biopython, FreeSASA]
./evident-agent/tests/fixtures/adversarial/sasa_endorse/evident.yaml:26:    last_verified:
./evident-agent/evident_agent/docker.py:6:oracle binaries available.
./evident-agent/evident_agent/last_concorded.py:1:"""Sidecar ``last_concorded.json`` read/write.
./evident-agent/evident_agent/last_concorded.py:3:The concordance counterpart to ``last_verified.json``. Per the v4
./evident-agent/evident_agent/last_concorded.py:6:``last_verified``; concordance claims consult ``last_concorded``.
./evident-agent/evident_agent/prompt.py:48:Escalate from Dissent to **Challenge** only when the digest contains a specific observed value that violates one of the target claim's stated tolerances. Report the violation as `{ target_criterion_id, metric, observed_value, bound, comparator, citation }` using the target's own metric name, bound, and comparator. The agent — not you — constructs the backing claim from your report. Do NOT propose new metrics, looser bounds, or trivial predicates like `observed > 0`, `row exists`, or `value is numeric` as a way to escalate. If you cannot cite a specific row/field/value that contradicts the target tolerance with its own bound, stay with Dissent.
./evident-agent/tests/fixtures/adversarial/ball_challenge/evident.yaml:13:    tolerances:
./evident-agent/tests/fixtures/adversarial/ball_challenge/evident.yaml:21:      oracle: [BALL, Tinker]
./evident-agent/tests/fixtures/adversarial/ball_challenge/evident.yaml:24:    last_verified:
./EVIDENT_PHASE5_PR4.codex-review.md:14:Code-review this Python PR as an independent engineer. Context: EVIDENT's evident-agent Python package adds the input-side claim extraction (extract harness + validator). This PR is the NON-walker parts (framing + validator + render); paper/repo walkers come in PR5/PR6. The validator is the load-bearing piece — it enforces a local-binding rule (metric+comparator+value+subject must co-occur in same sentence/cell/row) to kill 'silent threshold invention.' Code is in evident-agent/evident_agent/extract/{validator,framing,render}.py with tests in evident-agent/tests/test_extract_*.py.
./EVIDENT_PHASE5_PR4.codex-review.md:23:4) SUBJECT MATCHING — _subject_appears_in uses \b boundaries. Edge cases: aliases like 'we' could match in 'wear' or 'weight' — does \b prevent that? Aliases like 'X.Y' (the system name 'ABRA-2.0') — does \b break on the punctuation? What if subject_aliases is empty (the model didn't supply any)?
./EVIDENT_PHASE5_PR4.codex-review.md:27:6) DEFAULT-DENY FRAMING — the system prompt is ~80 lines. Is it too verbose (model might skim)? Are the 4 'specifically forbidden' clauses concrete enough to pattern-match? Specifically, the clause about 'wrong subject binding' uses the exact example codex v2 flagged — good. But: does the prompt say anything about WHAT to do if the source's subject is unclear (e.g. 'the method' without specifying whose)?
./EVIDENT_PHASE5_PR4.codex-review.md:88:+prompt reduces the *rate* at which the model proposes bad tolerances
./EVIDENT_PHASE5_PR4.codex-review.md:114:+4. The claimed subject of the measurement (the system being claimed \
./EVIDENT_PHASE5_PR4.codex-review.md:130:+- Attributing a bound to the wrong subject. If the source says \
./EVIDENT_PHASE5_PR4.codex-review.md:133:+*baseline*, not to your subject.
./EVIDENT_PHASE5_PR4.codex-review.md:141:+value, and subject. The validator checks this character-by-character.
./EVIDENT_PHASE5_PR4.codex-review.md:144:+``comparator_bound_to_wrong_subject``, ``value_only_in_image_table``, \
./EVIDENT_PHASE5_PR4.codex-review.md:184:+                                "the source-id and the claim subject."
./EVIDENT_PHASE5_PR4.codex-review.md:196:+                        "subject_aliases": {
./EVIDENT_PHASE5_PR4.codex-review.md:201:+                                "to the claimed subject (\"ours\", "
./EVIDENT_PHASE5_PR4.codex-review.md:209:+                        "tolerances": {
./EVIDENT_PHASE5_PR4.codex-review.md:226:+                                            "value, and subject in "
./EVIDENT_PHASE5_PR4.codex-review.md:257:+                        "subject_aliases",
./EVIDENT_PHASE5_PR4.codex-review.md:258:+                        "tolerances",
./EVIDENT_PHASE5_PR4.codex-review.md:285:+                                "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR4.codex-review.md:325:+        "subject) is missing or attached to a different subject, "
./EVIDENT_PHASE5_PR4.codex-review.md:407:+    subject_aliases: list[str]
./EVIDENT_PHASE5_PR4.codex-review.md:408:+    tolerances: list[dict]
./EVIDENT_PHASE5_PR4.codex-review.md:463:+        "tolerances": [_tolerance_block(t) for t in claim.tolerances],
./EVIDENT_PHASE5_PR4.codex-review.md:465:+            "oracle": ["Paper-Authority"],
./EVIDENT_PHASE5_PR4.codex-review.md:488:+        "last_verified": {
./EVIDENT_PHASE5_PR4.codex-review.md:499:+    extractor-side fields (``source_span``, ``subject_aliases``)
./EVIDENT_PHASE5_PR4.codex-review.md:536:+        for i, t in enumerate(claim.tolerances):
./EVIDENT_PHASE5_PR4.codex-review.md:564:+                f"  Tolerances: {len(c.tolerances)}; "
./EVIDENT_PHASE5_PR4.codex-review.md:565:+                f"subject aliases: {c.subject_aliases}"
./EVIDENT_PHASE5_PR4.codex-review.md:643:+claimed for the claimed subject.
./EVIDENT_PHASE5_PR4.codex-review.md:650:+   the bound value, AND at least one claimed-subject alias.
./EVIDENT_PHASE5_PR4.codex-review.md:654:+   subject in the same span does NOT satisfy the rule.
./EVIDENT_PHASE5_PR4.codex-review.md:767:+KIND_MISSING_SUBJECT = "missing_subject"
./EVIDENT_PHASE5_PR4.codex-review.md:769:+KIND_WRONG_BINDING = "comparator_bound_to_wrong_subject"
./EVIDENT_PHASE5_PR4.codex-review.md:796:+    bound, and subject must co-occur. For prose this is roughly a
./EVIDENT_PHASE5_PR4.codex-review.md:917:+def _subject_appears_in(text: str, aliases: Iterable[str]) -> bool:
./EVIDENT_PHASE5_PR4.codex-review.md:918:+    """True if any subject alias appears in `text` (case-insensitive,
./EVIDENT_PHASE5_PR4.codex-review.md:955:+    subject_aliases: Iterable[str],
./EVIDENT_PHASE5_PR4.codex-review.md:969:+    ``subject_aliases``: list of strings that refer to the claimed
./EVIDENT_PHASE5_PR4.codex-review.md:970:+    subject (``["our method", "we", "ours"]`` etc.). Provided by the
./EVIDENT_PHASE5_PR4.codex-review.md:971:+    extractor based on the source's own subject-identifying phrasings.
./EVIDENT_PHASE5_PR4.codex-review.md:1000:+    # subject alias. Use the longest-match comparator scanner so
./EVIDENT_PHASE5_PR4.codex-review.md:1009:+    subject_present_somewhere = False
./EVIDENT_PHASE5_PR4.codex-review.md:1011:+    aliases = list(subject_aliases)
./EVIDENT_PHASE5_PR4.codex-review.md:1021:+        has_subject = _subject_appears_in(sentence, aliases)
./EVIDENT_PHASE5_PR4.codex-review.md:1025:+        subject_present_somewhere |= has_subject
./EVIDENT_PHASE5_PR4.codex-review.md:1031:+            and has_subject
./EVIDENT_PHASE5_PR4.codex-review.md:1077:+    if not subject_present_somewhere:
./EVIDENT_PHASE5_PR4.codex-review.md:1081:+                "none of the claimed subject's aliases appear in "
./EVIDENT_PHASE5_PR4.codex-review.md:1092:+            "metric, comparator, value, and subject appear in the "
./EVIDENT_PHASE5_PR4.codex-review.md:1094:+            "the bound may be attached to a different subject in the "
./EVIDENT_PHASE5_PR4.codex-review.md:1129:+def test_tool_schema_claim_requires_subject_aliases_and_source_span():
./EVIDENT_PHASE5_PR4.codex-review.md:1136:+    assert "subject_aliases" in claim_schema["required"]
./EVIDENT_PHASE5_PR4.codex-review.md:1137:+    tolerance_schema = claim_schema["properties"]["tolerances"]["items"]
./EVIDENT_PHASE5_PR4.codex-review.md:1151:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR4.codex-review.md:1176:+def test_system_prompt_calls_out_wrong_subject_binding():
./EVIDENT_PHASE5_PR4.codex-review.md:1182:+    assert "wrong subject" in prompt or "bound to" in prompt
./EVIDENT_PHASE5_PR4.codex-review.md:1255:+                subject_aliases=["our method", "we", "ours"],
./EVIDENT_PHASE5_PR4.codex-review.md:1256:+                tolerances=[
./EVIDENT_PHASE5_PR4.codex-review.md:1340:+    t = manifest["claims"][0]["tolerances"][0]
./EVIDENT_PHASE5_PR4.codex-review.md:1351:+    v3: default-deny means honest emptiness > invented tolerances)."""
./EVIDENT_PHASE5_PR4.codex-review.md:1517:+                subject_aliases=["our method", "we"],
./EVIDENT_PHASE5_PR4.codex-review.md:1518:+                tolerances=[
./EVIDENT_PHASE5_PR4.codex-review.md:1614:+metric, comparator, bound, AND subject all co-occur in the same local
./EVIDENT_PHASE5_PR4.codex-review.md:1619:+claimed for the claimed subject.
./EVIDENT_PHASE5_PR4.codex-review.md:1642:+    metric, and subject all in the same sentence."""
./EVIDENT_PHASE5_PR4.codex-review.md:1652:+    subject_aliases = ["we", "our", "ours", "the proposed method"]
./EVIDENT_PHASE5_PR4.codex-review.md:1653:+    validate_tolerance(tolerance, subject_aliases=subject_aliases)
./EVIDENT_PHASE5_PR4.codex-review.md:1663:+    validate_tolerance(tolerance, subject_aliases=["our", "we", "ours"])
./EVIDENT_PHASE5_PR4.codex-review.md:1676:+    validate_tolerance(tolerance, subject_aliases=["we", "our"])
./EVIDENT_PHASE5_PR4.codex-review.md:1684:+def test_wrong_subject_binding_is_rejected():
./EVIDENT_PHASE5_PR4.codex-review.md:1689:+    *baseline* subject, not the claimed subject.
./EVIDENT_PHASE5_PR4.codex-review.md:1701:+            tolerance, subject_aliases=["our method", "we", "ours"]
./EVIDENT_PHASE5_PR4.codex-review.md:1703:+    assert exc.value.kind == "comparator_bound_to_wrong_subject"
./EVIDENT_PHASE5_PR4.codex-review.md:1728:+            tolerance, subject_aliases=["our", "we", "ours"]
./EVIDENT_PHASE5_PR4.codex-review.md:1731:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR4.codex-review.md:1738:+# Per-element rejection: missing metric, comparator, value, or subject
./EVIDENT_PHASE5_PR4.codex-review.md:1750:+        validate_tolerance(tolerance, subject_aliases=["we", "our"])
./EVIDENT_PHASE5_PR4.codex-review.md:1764:+            tolerance, subject_aliases=["our method", "we"]
./EVIDENT_PHASE5_PR4.codex-review.md:1768:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR4.codex-review.md:1781:+            tolerance, subject_aliases=["our", "we"]
./EVIDENT_PHASE5_PR4.codex-review.md:1785:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR4.codex-review.md:1789:+def test_missing_subject_is_rejected():
./EVIDENT_PHASE5_PR4.codex-review.md:1790:+    """If the source span never mentions the claimed subject, the
./EVIDENT_PHASE5_PR4.codex-review.md:1802:+            subject_aliases=["our method", "our system", "we propose"],
./EVIDENT_PHASE5_PR4.codex-review.md:1804:+    assert exc.value.kind == "missing_subject"
./EVIDENT_PHASE5_PR4.codex-review.md:1855:+    # Should NOT raise. The bound, comparator, and subject are all in
./EVIDENT_PHASE5_PR4.codex-review.md:1857:+    validate_tolerance(tolerance, subject_aliases=["our method", "we"])
./EVIDENT_PHASE5_PR4.codex-review.md:1892:+        tolerance, subject_aliases=["our system", "we", "our"]
./EVIDENT_PHASE5_PR4.codex-review.md:1924:+            tolerance, subject_aliases=["our method", "we"]
./EVIDENT_PHASE5_PR4.codex-review.md:1944:+            tolerance, subject_aliases=["our", "we"]
./EVIDENT_PHASE5_PR4.codex-review.md:1950:+# if the bound + subject co-occur in ONE of them
./EVIDENT_PHASE5_PR4.codex-review.md:1956:+    has all four (metric, comparator, value, subject), the tolerance
./EVIDENT_PHASE5_PR4.codex-review.md:1969:+        tolerance, subject_aliases=["our method", "we", "ours"]
./EVIDENT_PHASE5_PR4.codex-review.md:1994:prompt reduces the *rate* at which the model proposes bad tolerances
./EVIDENT_PHASE5_PR4.codex-review.md:2020:4. The claimed subject of the measurement (the system being claimed \
./EVIDENT_PHASE5_PR4.codex-review.md:2036:- Attributing a bound to the wrong subject. If the source says \
./EVIDENT_PHASE5_PR4.codex-review.md:2039:*baseline*, not to your subject.
./EVIDENT_PHASE5_PR4.codex-review.md:2047:value, and subject. The validator checks this character-by-character.
./EVIDENT_PHASE5_PR4.codex-review.md:2050:``comparator_bound_to_wrong_subject``, ``value_only_in_image_table``, \
./EVIDENT_PHASE5_PR4.codex-review.md:2090:                                "the source-id and the claim subject."
./EVIDENT_PHASE5_PR4.codex-review.md:2102:                        "subject_aliases": {
./EVIDENT_PHASE5_PR4.codex-review.md:2107:                                "to the claimed subject (\"ours\", "
./EVIDENT_PHASE5_PR4.codex-review.md:2115:                        "tolerances": {
./EVIDENT_PHASE5_PR4.codex-review.md:2132:                                            "value, and subject in "
./EVIDENT_PHASE5_PR4.codex-review.md:2163:                        "subject_aliases",
./EVIDENT_PHASE5_PR4.codex-review.md:2164:                        "tolerances",
./EVIDENT_PHASE5_PR4.codex-review.md:2191:                                "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR4.codex-review.md:2231:        "subject) is missing or attached to a different subject, "
./EVIDENT_PHASE5_PR4.codex-review.md:2277:claimed for the claimed subject.
./EVIDENT_PHASE5_PR4.codex-review.md:2284:   the bound value, AND at least one claimed-subject alias.
./EVIDENT_PHASE5_PR4.codex-review.md:2288:   subject in the same span does NOT satisfy the rule.
./EVIDENT_PHASE5_PR4.codex-review.md:2401:KIND_MISSING_SUBJECT = "missing_subject"
./EVIDENT_PHASE5_PR4.codex-review.md:2403:KIND_WRONG_BINDING = "comparator_bound_to_wrong_subject"
./EVIDENT_PHASE5_PR4.codex-review.md:2430:    bound, and subject must co-occur. For prose this is roughly a
./EVIDENT_PHASE5_PR4.codex-review.md:2551:def _subject_appears_in(text: str, aliases: Iterable[str]) -> bool:
./EVIDENT_PHASE5_PR4.codex-review.md:2552:    """True if any subject alias appears in `text` (case-insensitive,
./EVIDENT_PHASE5_PR4.codex-review.md:2589:    subject_aliases: Iterable[str],
./EVIDENT_PHASE5_PR4.codex-review.md:2603:    ``subject_aliases``: list of strings that refer to the claimed
./EVIDENT_PHASE5_PR4.codex-review.md:2604:    subject (``["our method", "we", "ours"]`` etc.). Provided by the
./EVIDENT_PHASE5_PR4.codex-review.md:2605:    extractor based on the source's own subject-identifying phrasings.
./EVIDENT_PHASE5_PR4.codex-review.md:2634:    # subject alias. Use the longest-match comparator scanner so
./EVIDENT_PHASE5_PR4.codex-review.md:2643:    subject_present_somewhere = False
./EVIDENT_PHASE5_PR4.codex-review.md:2645:    aliases = list(subject_aliases)
./EVIDENT_PHASE5_PR4.codex-review.md:2655:        has_subject = _subject_appears_in(sentence, aliases)
./EVIDENT_PHASE5_PR4.codex-review.md:2659:        subject_present_somewhere |= has_subject
./EVIDENT_PHASE5_PR4.codex-review.md:2665:            and has_subject
./EVIDENT_PHASE5_PR4.codex-review.md:2711:    if not subject_present_somewhere:
./EVIDENT_PHASE5_PR4.codex-review.md:2715:                "none of the claimed subject's aliases appear in "
./EVIDENT_PHASE5_PR4.codex-review.md:2726:            "metric, comparator, value, and subject appear in the "
./EVIDENT_PHASE5_PR4.codex-review.md:2728:            "the bound may be attached to a different subject in the "
./EVIDENT_PHASE5_PR4.codex-review.md:2776:    subject_aliases: list[str]
./EVIDENT_PHASE5_PR4.codex-review.md:2777:    tolerances: list[dict]
./EVIDENT_PHASE5_PR4.codex-review.md:2832:        "tolerances": [_tolerance_block(t) for t in claim.tolerances],
./EVIDENT_PHASE5_PR4.codex-review.md:2834:            "oracle": ["Paper-Authority"],
./EVIDENT_PHASE5_PR4.codex-review.md:2857:        "last_verified": {
./EVIDENT_PHASE5_PR4.codex-review.md:2868:    extractor-side fields (``source_span``, ``subject_aliases``)
./EVIDENT_PHASE5_PR4.codex-review.md:2905:        for i, t in enumerate(claim.tolerances):
./EVIDENT_PHASE5_PR4.codex-review.md:2933:                f"  Tolerances: {len(c.tolerances)}; "
./EVIDENT_PHASE5_PR4.codex-review.md:2934:                f"subject aliases: {c.subject_aliases}"
./EVIDENT_PHASE5_PR4.codex-review.md:3058:                subject_aliases=["our method", "we"],
./EVIDENT_PHASE5_PR4.codex-review.md:3059:                tolerances=[
./EVIDENT_PHASE5_PR4.codex-review.md:3162:    97	        "tolerances": [_tolerance_block(t) for t in claim.tolerances],
./EVIDENT_PHASE5_PR4.codex-review.md:3164:    99	            "oracle": ["Paper-Authority"],
./EVIDENT_PHASE5_PR4.codex-review.md:3187:   122	        "last_verified": {
./EVIDENT_PHASE5_PR4.codex-review.md:3198:   133	    extractor-side fields (``source_span``, ``subject_aliases``)
./EVIDENT_PHASE5_PR4.codex-review.md:3207:   131	KIND_MISSING_SUBJECT = "missing_subject"
./EVIDENT_PHASE5_PR4.codex-review.md:3209:   133	KIND_WRONG_BINDING = "comparator_bound_to_wrong_subject"
./EVIDENT_PHASE5_PR4.codex-review.md:3236:   160	    bound, and subject must co-occur. For prose this is roughly a
./EVIDENT_PHASE5_PR4.codex-review.md:3357:   281	def _subject_appears_in(text: str, aliases: Iterable[str]) -> bool:
./EVIDENT_PHASE5_PR4.codex-review.md:3358:   282	    """True if any subject alias appears in `text` (case-insensitive,
./EVIDENT_PHASE5_PR4.codex-review.md:3395:   319	    subject_aliases: Iterable[str],
./EVIDENT_PHASE5_PR4.codex-review.md:3409:   333	    ``subject_aliases``: list of strings that refer to the claimed
./EVIDENT_PHASE5_PR4.codex-review.md:3410:   334	    subject (``["our method", "we", "ours"]`` etc.). Provided by the
./EVIDENT_PHASE5_PR4.codex-review.md:3411:   335	    extractor based on the source's own subject-identifying phrasings.
./EVIDENT_PHASE5_PR4.codex-review.md:3535:    43	4. The claimed subject of the measurement (the system being claimed \
./EVIDENT_PHASE5_PR4.codex-review.md:3551:    59	- Attributing a bound to the wrong subject. If the source says \
./EVIDENT_PHASE5_PR4.codex-review.md:3554:    62	*baseline*, not to your subject.
./EVIDENT_PHASE5_PR4.codex-review.md:3562:    70	value, and subject. The validator checks this character-by-character.
./EVIDENT_PHASE5_PR4.codex-review.md:3565:    73	``comparator_bound_to_wrong_subject``, ``value_only_in_image_table``, \
./EVIDENT_PHASE5_PR4.codex-review.md:3599:P1 [validator.py:145](/scratch/TMAlign/evident/evident-agent/evident_agent/extract/validator.py:145): markdown table rows are not actually parsed as cells, and pipes are not boundaries. A span like `| baseline | rmsd | < 0.5 | ours | rmsd | 0.42 |` passes for `ours < 0.5` because all four tokens co-occur in one unsplit row. This is exactly the wrong-subject binding class the validator is meant to kill. Either split/validate table cells more carefully, or reject ambiguous multi-subject rows unless the comparator/value are in the same cell or adjacent cells tied to the claimed subject.
./EVIDENT_PHASE5_PR4.codex-review.md:3601:P2 [validator.py:200](/scratch/TMAlign/evident/evident-agent/evident_agent/extract/validator.py:200): comparator matching is also unanchored. Phrases like `under`, `above`, `max`, `<`, `>` are found by raw substring search, so `thunder`, `aboveboard`, or `maximal` can satisfy a comparator. The value/metric/subject requirements reduce blast radius, but this can still admit accidental bounds in dense scientific prose. Phrase comparators should use word boundaries or token-aware regex; symbolic operators can remain raw-token matches.
./EVIDENT_PHASE5_PR4.codex-review.md:3605:P2 [validator.py:281](/scratch/TMAlign/evident/evident-agent/evident_agent/extract/validator.py:281): subject aliases with punctuation are fragile at boundaries. `we` is safe against `wear`/`weight` due `\b`. But aliases ending in non-word chars, e.g. `ABRA-2.0`, fail because the trailing `\b` after `0` followed by punctuation can work, but aliases ending in punctuation or symbols can break; aliases starting with punctuation have the inverse problem. Prefer custom `(?<!\w)` / `(?!\w)` boundaries around escaped aliases. Empty `subject_aliases` correctly default-denies as `missing_subject`.
./EVIDENT_PHASE5_PR4.codex-review.md:3609:The phrase ordering is deterministic and safe under hash randomization: `LT_PHRASES` and `GT_PHRASES` are tuples, concatenated lists are sorted by `(-len, phrase)`, and no dict iteration is involved. Longest-first correctly protects `no more than` from `more than`. Case-insensitivity is handled with `.lower()`, but there is no Unicode normalization; decomposed characters could miss. For the current comparator set, that mostly affects non-ASCII operators and subject/metric aliases, so `unicodedata.normalize("NFKC", ...)` would be prudent.
./EVIDENT_PHASE5_PR4.codex-review.md:3613:The framing is directionally good: default-deny, explicit wrong-subject example, and concrete forbidden clauses. I would add one sentence: “If the source says only ‘the method’ or another ambiguous subject and you cannot bind it to the supplied artifact’s named system, emit no tolerance.” That covers unclear subject references directly.
./EVIDENT_PHASE5_PR4.codex-review.md:3619:What’s right: the validator is default-deny on missing source span/value/subject, the comparator longest-match design is solid, and the tests cover the core silent-threshold cases. The main blocker is token matching being too substring-oriented for a load-bearing gate.
./EVIDENT_PHASE5_PR4.codex-review.md:3626:P1 [validator.py:145](/scratch/TMAlign/evident/evident-agent/evident_agent/extract/validator.py:145): markdown table rows are not actually parsed as cells, and pipes are not boundaries. A span like `| baseline | rmsd | < 0.5 | ours | rmsd | 0.42 |` passes for `ours < 0.5` because all four tokens co-occur in one unsplit row. This is exactly the wrong-subject binding class the validator is meant to kill. Either split/validate table cells more carefully, or reject ambiguous multi-subject rows unless the comparator/value are in the same cell or adjacent cells tied to the claimed subject.
./EVIDENT_PHASE5_PR4.codex-review.md:3628:P2 [validator.py:200](/scratch/TMAlign/evident/evident-agent/evident_agent/extract/validator.py:200): comparator matching is also unanchored. Phrases like `under`, `above`, `max`, `<`, `>` are found by raw substring search, so `thunder`, `aboveboard`, or `maximal` can satisfy a comparator. The value/metric/subject requirements reduce blast radius, but this can still admit accidental bounds in dense scientific prose. Phrase comparators should use word boundaries or token-aware regex; symbolic operators can remain raw-token matches.
./EVIDENT_PHASE5_PR4.codex-review.md:3632:P2 [validator.py:281](/scratch/TMAlign/evident/evident-agent/evident_agent/extract/validator.py:281): subject aliases with punctuation are fragile at boundaries. `we` is safe against `wear`/`weight` due `\b`. But aliases ending in non-word chars, e.g. `ABRA-2.0`, fail because the trailing `\b` after `0` followed by punctuation can work, but aliases ending in punctuation or symbols can break; aliases starting with punctuation have the inverse problem. Prefer custom `(?<!\w)` / `(?!\w)` boundaries around escaped aliases. Empty `subject_aliases` correctly default-denies as `missing_subject`.
./EVIDENT_PHASE5_PR4.codex-review.md:3636:The phrase ordering is deterministic and safe under hash randomization: `LT_PHRASES` and `GT_PHRASES` are tuples, concatenated lists are sorted by `(-len, phrase)`, and no dict iteration is involved. Longest-first correctly protects `no more than` from `more than`. Case-insensitivity is handled with `.lower()`, but there is no Unicode normalization; decomposed characters could miss. For the current comparator set, that mostly affects non-ASCII operators and subject/metric aliases, so `unicodedata.normalize("NFKC", ...)` would be prudent.
./EVIDENT_PHASE5_PR4.codex-review.md:3640:The framing is directionally good: default-deny, explicit wrong-subject example, and concrete forbidden clauses. I would add one sentence: “If the source says only ‘the method’ or another ambiguous subject and you cannot bind it to the supplied artifact’s named system, emit no tolerance.” That covers unclear subject references directly.
./EVIDENT_PHASE5_PR4.codex-review.md:3646:What’s right: the validator is default-deny on missing source span/value/subject, the comparator longest-match design is solid, and the tests cover the core silent-threshold cases. The main blocker is token matching being too substring-oriented for a load-bearing gate.
./evident-agent/evident_agent/concordance.py:39:    Mirrors the ``last_concorded.json`` sidecar shape that this
./evident-agent/evident_agent/sidecar.py:1:"""Sidecar ``last_verified.json`` read/write.
./evident-agent/evident_agent/cli.py:84:    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./evident-agent/evident_agent/cli.py:122:        sidecar_path = manifest_path.parent / "last_verified.json"
./evident-agent/evident_agent/cli.py:274:    "last_verified_sidecar_path",
./evident-agent/evident_agent/cli.py:278:        "Path to a last_verified.json sidecar to overlay onto each claim "
./evident-agent/evident_agent/cli.py:279:        "before producing the digest. Default: manifest.parent / 'last_verified.json' if it exists."
./evident-agent/evident_agent/cli.py:317:    last_verified_sidecar_path: Optional[Path],
./evident-agent/evident_agent/cli.py:346:    if last_verified_sidecar_path is None:
./evident-agent/evident_agent/cli.py:347:        candidate = manifest_path.parent / "last_verified.json"
./evident-agent/evident_agent/cli.py:349:            last_verified_sidecar_path = candidate
./evident-agent/evident_agent/cli.py:365:    # Load the last_verified sidecar (if any) so per-claim verification
./evident-agent/evident_agent/cli.py:369:    last_verified_by_claim: dict[str, sidecar.LastVerifiedEntry] = {}
./evident-agent/evident_agent/cli.py:370:    if last_verified_sidecar_path is not None and last_verified_sidecar_path.is_file():
./evident-agent/evident_agent/cli.py:371:        last_verified_by_claim = sidecar.read(last_verified_sidecar_path)
./evident-agent/evident_agent/cli.py:390:        # Per-claim commit comes from (in order): the last_verified
./evident-agent/evident_agent/cli.py:391:        # sidecar entry > the manifest's inline last_verified.commit
./evident-agent/evident_agent/cli.py:394:        commit = _resolve_commit_for_claim(claim.raw, last_verified_by_claim.get(claim.id))
./evident-agent/evident_agent/cli.py:586:            sidecar_path=last_verified_sidecar_path,
./evident-agent/evident_agent/cli.py:604:    Precedence: sidecar entry > inline manifest last_verified.commit
./evident-agent/evident_agent/cli.py:611:    inline = (claim_raw.get("last_verified") or {}).get("commit")
./evident-agent/evident_agent/cli.py:618:    missing tolerances."""
./evident-agent/evident_agent/cli.py:619:    tols = claim_raw.get("tolerances") or []
./evident-agent/evident_agent/cli.py:631:    tols = claim_raw.get("tolerances") or []
./evident-agent/evident_agent/cli.py:641:    The model sees the full structured claim (tier, all tolerances,
./evident-agent/evident_agent/cli.py:642:    evidence pointer, last_verified) — multi-criterion claims must
./evident-agent/evident_agent/cli.py:837:    extract structured tolerances. Each tolerance is validated by
./evident-agent/evident_agent/cli.py:1034:    Editable fields: title, claim, tolerances, case, source,
./evident-agent/evident_agent/cli.py:1036:    evidence, provenance, last_verified) must NOT be changed —
./evident-agent/evident_agent/cli.py:1096:    root and emits `kind: metadata_compatibility` claims. No model
./evident-agent/tests/fixtures/adversarial/ball_dissent/evident.yaml:13:    tolerances:
./evident-agent/tests/fixtures/adversarial/ball_dissent/evident.yaml:21:      oracle: [BALL, Tinker]
./evident-agent/tests/fixtures/adversarial/ball_dissent/evident.yaml:24:    last_verified:
./evident-agent/evident_agent/extract/render.py:41:    subject_aliases: list[str]
./evident-agent/evident_agent/extract/render.py:42:    tolerances: list[dict]
./evident-agent/evident_agent/extract/render.py:97:        "tolerances": [_tolerance_block(t) for t in claim.tolerances],
./evident-agent/evident_agent/extract/render.py:99:            "oracle": ["Paper-Authority"],
./evident-agent/evident_agent/extract/render.py:122:        "last_verified": {
./evident-agent/evident_agent/extract/render.py:133:    extractor-side fields (``source_span``, ``subject_aliases``)
./evident-agent/evident_agent/extract/render.py:170:        for i, t in enumerate(claim.tolerances):
./evident-agent/evident_agent/extract/render.py:198:                f"  Tolerances: {len(c.tolerances)}; "
./evident-agent/evident_agent/extract/render.py:199:                f"subject aliases: {c.subject_aliases}"
./evident-agent/evident_agent/__init__.py:7:``last_verified.json`` (framework convention) which typed-trust then
./evident-agent/evident_agent/extract/validator.py:7:claimed for the claimed subject.
./evident-agent/evident_agent/extract/validator.py:14:   the bound value, AND at least one claimed-subject alias.
./evident-agent/evident_agent/extract/validator.py:18:   subject in the same span does NOT satisfy the rule.
./evident-agent/evident_agent/extract/validator.py:131:KIND_MISSING_SUBJECT = "missing_subject"
./evident-agent/evident_agent/extract/validator.py:133:KIND_WRONG_BINDING = "comparator_bound_to_wrong_subject"
./evident-agent/evident_agent/extract/validator.py:143:# wrong-subject case (F-PR4-CR1b): a markdown table row like
./evident-agent/evident_agent/extract/validator.py:147:# comparator and bound to be in the same cell as the subject.
./evident-agent/evident_agent/extract/validator.py:163:    bound, and subject must co-occur. For prose this is roughly a
./evident-agent/evident_agent/extract/validator.py:311:def _subject_appears_in(text: str, aliases: Iterable[str]) -> bool:
./evident-agent/evident_agent/extract/validator.py:312:    """True if any subject alias appears in `text` (case-insensitive,
./evident-agent/evident_agent/extract/validator.py:370:    subject_aliases: Iterable[str],
./evident-agent/evident_agent/extract/validator.py:384:    ``subject_aliases``: list of strings that refer to the claimed
./evident-agent/evident_agent/extract/validator.py:385:    subject (``["our method", "we", "ours"]`` etc.). Provided by the
./evident-agent/evident_agent/extract/validator.py:386:    extractor based on the source's own subject-identifying phrasings.
./evident-agent/evident_agent/extract/validator.py:415:    # subject alias. Use the longest-match comparator scanner so
./evident-agent/evident_agent/extract/validator.py:424:    subject_present_somewhere = False
./evident-agent/evident_agent/extract/validator.py:426:    aliases = list(subject_aliases)
./evident-agent/evident_agent/extract/validator.py:436:        has_subject = _subject_appears_in(sentence, aliases)
./evident-agent/evident_agent/extract/validator.py:440:        subject_present_somewhere |= has_subject
./evident-agent/evident_agent/extract/validator.py:446:            and has_subject
./evident-agent/evident_agent/extract/validator.py:492:    if not subject_present_somewhere:
./evident-agent/evident_agent/extract/validator.py:496:                "none of the claimed subject's aliases appear in "
./evident-agent/evident_agent/extract/validator.py:507:            "metric, comparator, value, and subject appear in the "
./evident-agent/evident_agent/extract/validator.py:509:            "the bound may be attached to a different subject in the "
./evident-agent/evident_agent/extract/cli.py:16:validator-approved tolerances reach ``evident.yaml``.
./evident-agent/evident_agent/extract/cli.py:139:    tolerances, drop claims with zero remaining tolerances, and
./evident-agent/evident_agent/extract/cli.py:150:        accepted_tolerances: list[dict] = []
./evident-agent/evident_agent/extract/cli.py:151:        for raw_tol in raw_claim.get("tolerances", []):
./evident-agent/evident_agent/extract/cli.py:155:                    subject_aliases=raw_claim.get(
./evident-agent/evident_agent/extract/cli.py:156:                        "subject_aliases", []
./evident-agent/evident_agent/extract/cli.py:174:            accepted_tolerances.append(raw_tol)
./evident-agent/evident_agent/extract/cli.py:176:        if not accepted_tolerances:
./evident-agent/evident_agent/extract/cli.py:177:            # Codex v3 contract: claim with zero valid tolerances is
./evident-agent/evident_agent/extract/cli.py:187:                subject_aliases=list(
./evident-agent/evident_agent/extract/cli.py:188:                    raw_claim.get("subject_aliases", [])
./evident-agent/evident_agent/extract/cli.py:190:                tolerances=accepted_tolerances,
./evident-agent/evident_agent/extract/cli.py:227:    "missing_subject": "validator_missing_subject",
./evident-agent/evident_agent/extract/cli.py:229:    "comparator_bound_to_wrong_subject": "comparator_bound_to_wrong_subject",
./evident-agent/evident_agent/extract/cli.py:238:    and `comparator_bound_to_wrong_subject`) so the curator can
./evident-agent/evident_agent/extract/cli.py:298:    # claim it proposed, with tolerances + source spans + prose)
./evident-agent/evident_agent/extract/metadata.py:4:package.json) and emits ``kind: metadata_compatibility`` claims.
./evident-agent/evident_agent/extract/metadata.py:43:    """One emitted metadata_compatibility claim ready for manifest
./evident-agent/evident_agent/extract/metadata.py:584:    Output shape: one ``kind: metadata_compatibility`` claim per
./evident-agent/evident_agent/extract/metadata.py:587:    No ``tolerances`` or ``evidence.command`` — the declaration IS
./evident-agent/evident_agent/extract/metadata.py:606:                "kind": "metadata_compatibility",
./EVIDENT_AGENT_PHASE2B.code-review.md:126:+Escalate from Dissent to **Challenge** only when the digest contains a specific observed value that violates one of the target claim's stated tolerances. Report the violation as `{ target_criterion_id, metric, observed_value, bound, comparator, citation }` using the target's own metric name, bound, and comparator. The agent — not you — constructs the backing claim from your report. Do NOT propose new metrics, looser bounds, or trivial predicates like `observed > 0`, `row exists`, or `value is numeric` as a way to escalate. If you cannot cite a specific row/field/value that contradicts the target tolerance with its own bound, stay with Dissent.
./EVIDENT_AGENT_PHASE2B.code-review.md:512:+  is the target's bound; `last_verified.value` is the violation's
./EVIDENT_AGENT_PHASE2B.code-review.md:564:+      target's ``tolerances`` list (no inventing criteria).
./EVIDENT_AGENT_PHASE2B.code-review.md:582:+    tolerances = target_claim.get("tolerances") or []
./EVIDENT_AGENT_PHASE2B.code-review.md:584:+    for t in tolerances:
./EVIDENT_AGENT_PHASE2B.code-review.md:590:+            f"target_criterion_id {target_criterion_id!r} not found in target tolerances"
./EVIDENT_AGENT_PHASE2B.code-review.md:676:+      observed_value as ``last_verified.value``, the backing's
./EVIDENT_AGENT_PHASE2B.code-review.md:680:+      command, oracle. The Challenge cites a row of the same
./EVIDENT_AGENT_PHASE2B.code-review.md:682:+    - Includes ``last_verified.date`` (today's date in UTC) AND
./EVIDENT_AGENT_PHASE2B.code-review.md:683:+      ``last_verified.value``. Both fields are required for
./EVIDENT_AGENT_PHASE2B.code-review.md:684:+      typed-trust's ``translate_last_verified`` to bind the
./EVIDENT_AGENT_PHASE2B.code-review.md:712:+        "tolerances": [
./EVIDENT_AGENT_PHASE2B.code-review.md:725:+        "last_verified": {
./EVIDENT_AGENT_PHASE2B.code-review.md:793:+    artifact, command, oracle. The Challenge cites a row of that same
./EVIDENT_AGENT_PHASE2B.code-review.md:797:+        "oracle": list(target_evidence.get("oracle") or []),
./EVIDENT_AGENT_PHASE2B.code-review.md:891:+        "tolerances": [
./EVIDENT_AGENT_PHASE2B.code-review.md:900:+            "oracle": ["BALL"],
./EVIDENT_AGENT_PHASE2B.code-review.md:975:+    target["last_verified"] = {
./EVIDENT_AGENT_PHASE2B.code-review.md:990:+        "    tolerances:\n"
./EVIDENT_AGENT_PHASE2B.code-review.md:996:+        "      oracle: [BALL]\n"
./EVIDENT_AGENT_PHASE2B.code-review.md:999:+        "    last_verified:\n"
./EVIDENT_AGENT_PHASE2B.code-review.md:1080:+        "    tolerances:\n"
./EVIDENT_AGENT_PHASE2B.code-review.md:1086:+        "      oracle: [BALL]\n"
./EVIDENT_AGENT_PHASE2B.code-review.md:1152:+        "    tolerances:\n"
./EVIDENT_AGENT_PHASE2B.code-review.md:1158:+        "      oracle: [BALL]\n"
./EVIDENT_AGENT_PHASE2B.code-review.md:1197:+                                "tolerances": [
./EVIDENT_AGENT_PHASE2B.code-review.md:1206:+                                    "oracle": ["BALL"],
./EVIDENT_AGENT_PHASE2B.code-review.md:1210:+                                "last_verified": {
./EVIDENT_AGENT_PHASE2B.code-review.md:1378:+        "tolerances": [
./EVIDENT_AGENT_PHASE2B.code-review.md:1387:+            "oracle": ["BALL"],
./EVIDENT_AGENT_PHASE2B.code-review.md:1427:+    assert backing["tolerances"][0]["op"] == ">="
./EVIDENT_AGENT_PHASE2B.code-review.md:1428:+    assert backing["tolerances"][0]["value"] == 0.02
./EVIDENT_AGENT_PHASE2B.code-review.md:1429:+    # last_verified.value = violation.observed_value (so backing sustains).
./EVIDENT_AGENT_PHASE2B.code-review.md:1430:+    assert backing["last_verified"]["value"] == 0.025
./EVIDENT_AGENT_PHASE2B.code-review.md:1431:+    assert backing["last_verified"]["date"]
./EVIDENT_AGENT_PHASE2B.code-review.md:1536:+                "tolerances": [
./EVIDENT_AGENT_PHASE2B.code-review.md:1539:+                "evidence": {"oracle": ["BALL"], "command": "pytest", "artifact": "x"},
./EVIDENT_AGENT_PHASE2B.code-review.md:1540:+                "last_verified": {"date": "2026-06-02", "value": 0.025},
./EVIDENT_AGENT_PHASE2B.code-review.md:1577:+    assert e["challenge"]["backing_claim"]["tolerances"][0]["op"] == ">="
./EVIDENT_AGENT_PHASE2B.code-review.md:1604:+        "    tolerances:\n"
./EVIDENT_AGENT_PHASE2B.code-review.md:1610:+        "      oracle: [BALL]\n"
./EVIDENT_AGENT_PHASE2B.code-review.md:1658:+  is the logical inverse of the target's, whose last_verified.value
./EVIDENT_AGENT_PHASE2B.code-review.md:1685:+        "tolerances": [
./EVIDENT_AGENT_PHASE2B.code-review.md:1694:+            "oracle": ["BALL"],
./EVIDENT_AGENT_PHASE2B.code-review.md:1726:+    tol = backing["tolerances"][0]
./EVIDENT_AGENT_PHASE2B.code-review.md:1731:+    # last_verified.value = observed; satisfies the inverse tolerance.
./EVIDENT_AGENT_PHASE2B.code-review.md:1732:+    assert backing["last_verified"]["value"] == 0.025
./EVIDENT_AGENT_PHASE2B.code-review.md:1733:+    assert backing["last_verified"]["value"] >= tol["value"]
./EVIDENT_AGENT_PHASE2B.code-review.md:1734:+    # Date is present so typed-trust's translate_last_verified binds.
./EVIDENT_AGENT_PHASE2B.code-review.md:1735:+    assert backing["last_verified"]["date"]
./EVIDENT_AGENT_PHASE2B.code-review.md:1773:+    with pytest.raises(ViolationRejected, match="not found in target tolerances"):
./EVIDENT_AGENT_PHASE2B.code-review.md:1781:+    target["tolerances"][0]["op"] = None
./EVIDENT_AGENT_PHASE2B.code-review.md:1782:+    target["tolerances"][0]["value"] = None
./EVIDENT_AGENT_PHASE2B.code-review.md:1809:+    target["tolerances"][0]["op"] = "="
./EVIDENT_AGENT_PHASE2B.code-review.md:1889:+    target["tolerances"][0]["op"] = "<="
./EVIDENT_AGENT_PHASE2B.code-review.md:1928:+    target["tolerances"][0]["op"] = target_op
./EVIDENT_AGENT_PHASE2B.code-review.md:1929:+    target["tolerances"][0]["value"] = bound
./EVIDENT_AGENT_PHASE2B.code-review.md:1938:+    assert backing["tolerances"][0]["op"] == inverse_op
./EVIDENT_AGENT_PHASE2B.code-review.md:1939:+    assert backing["last_verified"]["value"] == observed
./EVIDENT_AGENT_PHASE2B.code-review.md:1949:-    translate_tolerances, ManifestClaim, ManifestLastVerified, ReviewEventSidecar,
./EVIDENT_AGENT_PHASE2B.code-review.md:1952:+    translate_review_event, translate_tolerances, ManifestClaim, ManifestLastVerified,
./EVIDENT_AGENT_PHASE2B.code-review.md:2083:+        let bc_criteria = translate_tolerances(bc)
./EVIDENT_AGENT_PHASE2B.code-review.md:2418:+        tolerances: Some(vec![ManifestTolerance {
./EVIDENT_AGENT_PHASE2B.code-review.md:2426:+            oracle: vec!["BALL".into()],
./EVIDENT_AGENT_PHASE2B.code-review.md:2431:+        last_verified: None,
./EVIDENT_AGENT_PHASE2B.code-review.md:2596: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./EVIDENT_AGENT_PHASE2B.code-review.md:2602: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./EVIDENT_AGENT_PHASE2B.code-review.md:2613:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./EVIDENT_AGENT_PHASE2B.code-review.md:2635:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./EVIDENT_AGENT_PHASE2B.code-review.md:2757:+  is the target's bound; `last_verified.value` is the violation's
./EVIDENT_AGENT_PHASE2B.code-review.md:2809:+      target's ``tolerances`` list (no inventing criteria).
./EVIDENT_AGENT_PHASE2B.code-review.md:2827:+    tolerances = target_claim.get("tolerances") or []
./EVIDENT_AGENT_PHASE2B.code-review.md:2829:+    for t in tolerances:
./EVIDENT_AGENT_PHASE2B.code-review.md:2835:+            f"target_criterion_id {target_criterion_id!r} not found in target tolerances"
./EVIDENT_AGENT_PHASE2B.code-review.md:2921:+      observed_value as ``last_verified.value``, the backing's
./EVIDENT_AGENT_PHASE2B.code-review.md:2925:+      command, oracle. The Challenge cites a row of the same
./EVIDENT_AGENT_PHASE2B.code-review.md:2927:+    - Includes ``last_verified.date`` (today's date in UTC) AND
./EVIDENT_AGENT_PHASE2B.code-review.md:2928:+      ``last_verified.value``. Both fields are required for
./EVIDENT_AGENT_PHASE2B.code-review.md:2929:+      typed-trust's ``translate_last_verified`` to bind the
./EVIDENT_AGENT_PHASE2B.code-review.md:2957:+        "tolerances": [
./EVIDENT_AGENT_PHASE2B.code-review.md:2970:+        "last_verified": {
./EVIDENT_AGENT_PHASE2B.code-review.md:3038:+    artifact, command, oracle. The Challenge cites a row of that same
./EVIDENT_AGENT_PHASE2B.code-review.md:3042:+        "oracle": list(target_evidence.get("oracle") or []),
./EVIDENT_AGENT_PHASE2B.code-review.md:3063:-    translate_tolerances, ManifestClaim, ManifestLastVerified, ReviewEventSidecar,
./EVIDENT_AGENT_PHASE2B.code-review.md:3066:+    translate_review_event, translate_tolerances, ManifestClaim, ManifestLastVerified,
./EVIDENT_AGENT_PHASE2B.code-review.md:3197:+        let bc_criteria = translate_tolerances(bc)
./EVIDENT_AGENT_PHASE2B.code-review.md:4053:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./EVIDENT_AGENT_PHASE2B.code-review.md:4157:        let bc_criteria = translate_tolerances(bc)
./EVIDENT_AGENT_PHASE2B.code-review.md:4395://! observed values against tolerances, applies the §8 rule for
./EVIDENT_AGENT_PHASE2B.code-review.md:4418:///   `last_verified`).
./EVIDENT_AGENT_PHASE2B.code-review.md:4915:443:fn translate_last_verified(
./EVIDENT_AGENT_PHASE2B.code-review.md:4921:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./EVIDENT_AGENT_PHASE2B.code-review.md:4922:///   collapsed into a single Evidence; oracle identity per tolerance
./EVIDENT_AGENT_PHASE2B.code-review.md:4924:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./EVIDENT_AGENT_PHASE2B.code-review.md:4939:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./EVIDENT_AGENT_PHASE2B.code-review.md:4941:///   criterion id (shipping convention: `last_verified.value` is the
./EVIDENT_AGENT_PHASE2B.code-review.md:4962:    let reruns = translate_last_verified(
./EVIDENT_AGENT_PHASE2B.code-review.md:4963:        mc.last_verified.as_ref(),
./EVIDENT_AGENT_PHASE2B.code-review.md:5000:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./EVIDENT_AGENT_PHASE2B.code-review.md:5002:/// - `last_verified` is absent;
./EVIDENT_AGENT_PHASE2B.code-review.md:5003:/// - `last_verified.date` is null (replay loop hasn't run);
./EVIDENT_AGENT_PHASE2B.code-review.md:5004:/// - `last_verified.value` is null (no primary observation).
./EVIDENT_AGENT_PHASE2B.code-review.md:5009:fn translate_last_verified(
./EVIDENT_AGENT_PHASE2B.code-review.md:5035:        // Shipping convention: a populated last_verified records a
./EVIDENT_AGENT_PHASE2B.code-review.md:5103:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./EVIDENT_AGENT_PHASE2B.code-review.md:5104:///   (most measurement claims compare against an oracle).
./EVIDENT_AGENT_PHASE2B.code-review.md:5111:        .is_some_and(|e| !e.oracle.is_empty())
./EVIDENT_AGENT_PHASE2B.code-review.md:5166:/bin/bash -lc "grep -n \"fn translate_tolerances\" -n typed-trust/src/translate.rs && sed -n '250,350p' typed-trust/src/translate.rs && grep -R \"tolerances\" -n evident-agent/tests/test_violation.py | head" in /scratch/TMAlign/evident
./EVIDENT_AGENT_PHASE2B.code-review.md:5168:272:pub fn translate_tolerances(
./EVIDENT_AGENT_PHASE2B.code-review.md:5184:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./EVIDENT_AGENT_PHASE2B.code-review.md:5188:/// When the claim's `evidence.oracle` is a single entry, populate
./EVIDENT_AGENT_PHASE2B.code-review.md:5189:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./EVIDENT_AGENT_PHASE2B.code-review.md:5191:pub fn translate_tolerances(
./EVIDENT_AGENT_PHASE2B.code-review.md:5194:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./EVIDENT_AGENT_PHASE2B.code-review.md:5195:        if e.oracle.len() == 1 {
./EVIDENT_AGENT_PHASE2B.code-review.md:5196:            Some(e.oracle[0].clone())
./EVIDENT_AGENT_PHASE2B.code-review.md:5202:    let Some(ts) = mc.tolerances.as_ref() else {
./EVIDENT_AGENT_PHASE2B.code-review.md:5203:        // Measurement claims require non-empty tolerances per
./EVIDENT_AGENT_PHASE2B.code-review.md:5223:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./EVIDENT_AGENT_PHASE2B.code-review.md:5235:    single_oracle: &Option<String>,
./EVIDENT_AGENT_PHASE2B.code-review.md:5244:        // tolerances; allowing them to translate would let
./EVIDENT_AGENT_PHASE2B.code-review.md:5261:            against: single_oracle.clone(),
./EVIDENT_AGENT_PHASE2B.code-review.md:5270:39:        "tolerances": [
./EVIDENT_AGENT_PHASE2B.code-review.md:5271:80:    tol = backing["tolerances"][0]
./EVIDENT_AGENT_PHASE2B.code-review.md:5272:127:    with pytest.raises(ViolationRejected, match="not found in target tolerances"):
./EVIDENT_AGENT_PHASE2B.code-review.md:5273:135:    target["tolerances"][0]["op"] = None
./EVIDENT_AGENT_PHASE2B.code-review.md:5274:136:    target["tolerances"][0]["value"] = None
./EVIDENT_AGENT_PHASE2B.code-review.md:5275:163:    target["tolerances"][0]["op"] = "="
./EVIDENT_AGENT_PHASE2B.code-review.md:5276:243:    target["tolerances"][0]["op"] = "<="
./EVIDENT_AGENT_PHASE2B.code-review.md:5277:282:    target["tolerances"][0]["op"] = target_op
./EVIDENT_AGENT_PHASE2B.code-review.md:5278:283:    target["tolerances"][0]["value"] = bound
./EVIDENT_AGENT_PHASE2B.code-review.md:5279:292:    assert backing["tolerances"][0]["op"] == inverse_op
./EVIDENT_AGENT_PHASE2B.code-review.md:5368: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./EVIDENT_AGENT_PHASE2B.code-review.md:5374: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./EVIDENT_AGENT_PHASE2B.code-review.md:5385:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./EVIDENT_AGENT_PHASE2B.code-review.md:5407:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./EVIDENT_AGENT_PHASE2B.code-review.md:5516:    # Load the last_verified sidecar (if any) so per-claim verification
./EVIDENT_AGENT_PHASE2B.code-review.md:5520:    last_verified_by_claim: dict[str, sidecar.LastVerifiedEntry] = {}
./EVIDENT_AGENT_PHASE2B.code-review.md:5521:    if last_verified_sidecar_path is not None and last_verified_sidecar_path.is_file():
./EVIDENT_AGENT_PHASE2B.code-review.md:5522:        last_verified_by_claim = sidecar.read(last_verified_sidecar_path)
./EVIDENT_AGENT_PHASE2B.code-review.md:5541:        # Per-claim commit comes from (in order): the last_verified
./EVIDENT_AGENT_PHASE2B.code-review.md:5542:        # sidecar entry > the manifest's inline last_verified.commit
./EVIDENT_AGENT_PHASE2B.code-review.md:5545:        commit = _resolve_commit_for_claim(claim.raw, last_verified_by_claim.get(claim.id))
./EVIDENT_AGENT_PHASE2B.code-review.md:5659:    Precedence: sidecar entry > inline manifest last_verified.commit
./EVIDENT_AGENT_PHASE2B.code-review.md:5666:    inline = (claim_raw.get("last_verified") or {}).get("commit")
./EVIDENT_AGENT_PHASE2B.code-review.md:5673:    missing tolerances."""
./EVIDENT_AGENT_PHASE2B.code-review.md:5674:    tols = claim_raw.get("tolerances") or []
./EVIDENT_AGENT_PHASE2B.code-review.md:5686:    tols = claim_raw.get("tolerances") or []
./EVIDENT_AGENT_PHASE2B.code-review.md:5696:    The model sees the full structured claim (tier, all tolerances,
./EVIDENT_AGENT_PHASE2B.code-review.md:5697:    evidence pointer, last_verified) — multi-criterion claims must
./EVIDENT_AGENT_PHASE2B.code-review.md:5718:  is the logical inverse of the target's, whose last_verified.value
./EVIDENT_AGENT_PHASE2B.code-review.md:5745:        "tolerances": [
./EVIDENT_AGENT_PHASE2B.code-review.md:5754:            "oracle": ["BALL"],
./EVIDENT_AGENT_PHASE2B.code-review.md:5786:    tol = backing["tolerances"][0]
./EVIDENT_AGENT_PHASE2B.code-review.md:5880:Mirrors Phase 1's ``sidecar.py`` (last_verified) but with two extra
./EVIDENT_AGENT_PHASE2B.code-review.md:6188:    91	    tolerances = target_claim.get("tolerances") or []
./EVIDENT_AGENT_PHASE2B.code-review.md:6190:    93	    for t in tolerances:
./EVIDENT_AGENT_PHASE2B.code-review.md:6196:    99	            f"target_criterion_id {target_criterion_id!r} not found in target tolerances"
./EVIDENT_AGENT_PHASE2B.code-review.md:6205:/bin/bash -lc 'grep -n "tolerances" -A45 workflow/SCHEMA.md | head -60' in /scratch/TMAlign/evident
./EVIDENT_AGENT_PHASE2B.code-review.md:6207:89:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./EVIDENT_AGENT_PHASE2B.code-review.md:6208:90-| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./EVIDENT_AGENT_PHASE2B.code-review.md:6211:93-| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./EVIDENT_AGENT_PHASE2B.code-review.md:6217:99:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./EVIDENT_AGENT_PHASE2B.code-review.md:6228:110-- **Oracles**: `pinned_versions` must include every oracle named in
./EVIDENT_AGENT_PHASE2B.code-review.md:6229:111-  `evidence.oracle` with the version used to produce the cited result
./EVIDENT_AGENT_PHASE2B.code-review.md:6238:120:  `tolerances[].output` should reference an entry by name.
./EVIDENT_AGENT_PHASE2B.code-review.md:6247:129:tolerances:
./EVIDENT_AGENT_PHASE2B.code-review.md:6300:192:+    assert backing["tolerances"][0]["op"] == ">="
./EVIDENT_AGENT_PHASE2B.code-review.md:6301:193:+    assert backing["tolerances"][0]["value"] == 0.02
./EVIDENT_AGENT_PHASE2B.code-review.md:6302:194:+    # last_verified.value = violation.observed_value (so backing sustains).
./EVIDENT_AGENT_PHASE2B.code-review.md:6303:195:+    assert backing["last_verified"]["value"] == 0.025
./EVIDENT_AGENT_PHASE2B.code-review.md:6304:196:+    assert backing["last_verified"]["date"]
./EVIDENT_AGENT_PHASE2B.code-review.md:6326:342:+    assert e["challenge"]["backing_claim"]["tolerances"][0]["op"] == ">="
./EVIDENT_AGENT_PHASE2B.code-review.md:6360:- [P2] Disambiguate duplicate metric tolerances — /scratch/TMAlign/evident/evident-agent/evident_agent/violation.py:93-95
./EVIDENT_AGENT_PHASE2B.code-review.md:6361:  When a claim has multiple tolerances with the same `metric` but different `output` or bounds, this always selects the first one. A valid challenge against the later tolerance will be rejected as threshold drift, or validated against the wrong criterion, even though the schema allows several tolerance entries. Use a stable criterion identifier or include enough fields to select the intended tolerance.
./EVIDENT_AGENT_PHASE2B.code-review.md:6369:- [P2] Disambiguate duplicate metric tolerances — /scratch/TMAlign/evident/evident-agent/evident_agent/violation.py:93-95
./EVIDENT_AGENT_PHASE2B.code-review.md:6370:  When a claim has multiple tolerances with the same `metric` but different `output` or bounds, this always selects the first one. A valid challenge against the later tolerance will be rejected as threshold drift, or validated against the wrong criterion, even though the schema allows several tolerance entries. Use a stable criterion identifier or include enough fields to select the intended tolerance.
./evident-agent/evident_agent/extract/framing.py:17:prompt reduces the *rate* at which the model proposes bad tolerances
./evident-agent/evident_agent/extract/framing.py:43:4. The claimed subject of the measurement (the system being claimed \
./evident-agent/evident_agent/extract/framing.py:59:- Attributing a bound to the wrong subject. If the source says \
./evident-agent/evident_agent/extract/framing.py:62:*baseline*, not to your subject.
./evident-agent/evident_agent/extract/framing.py:63:- Extracting when the source's subject is ambiguous. If the source \
./evident-agent/evident_agent/extract/framing.py:73:value, and subject. The validator checks this character-by-character.
./evident-agent/evident_agent/extract/framing.py:76:``comparator_bound_to_wrong_subject``, ``value_only_in_image_table``, \
./evident-agent/evident_agent/extract/framing.py:116:                                "the source-id and the claim subject."
./evident-agent/evident_agent/extract/framing.py:128:                        "subject_aliases": {
./evident-agent/evident_agent/extract/framing.py:133:                                "to the claimed subject (\"ours\", "
./evident-agent/evident_agent/extract/framing.py:141:                        "tolerances": {
./evident-agent/evident_agent/extract/framing.py:158:                                            "value, and subject in "
./evident-agent/evident_agent/extract/framing.py:189:                        "subject_aliases",
./evident-agent/evident_agent/extract/framing.py:190:                        "tolerances",
./evident-agent/evident_agent/extract/framing.py:217:                                "comparator_bound_to_wrong_subject",
./evident-agent/evident_agent/extract/framing.py:257:        "subject) is missing or attached to a different subject, "
./EVIDENT_AGENT_PHASE3_DRAFT.md:8:- **Phase 1**: deterministic replay agent populates `last_verified`.
./EVIDENT_AGENT_PHASE3_DRAFT.md:121:             canonical last_verified sidecar path)
./EVIDENT_AGENT_PHASE3_DRAFT.md:167:read_report(manifest_path, claim_id, sidecar?, last_verified_sidecar?)
./EVIDENT_AGENT_PHASE3_DRAFT.md:202:              last_verified_sidecar?, format)
./EVIDENT_AGENT_PHASE3_DRAFT.md:430:    last_verified_sidecar: Option<String>,
./EVIDENT_AGENT_PHASE3_DRAFT.md:586:| sidecar loaders (`review_events`, `last_verified`) | ✅ Yes |
./EVIDENT_AGENT_PHASE3_DRAFT.md:603:  files (e.g., passing a `last_verified.json` shape via
./EVIDENT_AGENT_PHASE3_DRAFT.md:605:- `last_verified.json` sidecars are accepted only via the
./EVIDENT_AGENT_PHASE3_DRAFT.md:606:  `last_verified_sidecar` parameter; same schema-bind discipline.
./EVIDENT_AGENT_PHASE3_DRAFT.md:743:- **Sidecar type mismatch.** Passing a `last_verified.json`
./anti-patterns/README.md:34:trust question, oracle, tolerance, assumption, or decision rule they support.
./anti-patterns/README.md:43:- identify the oracle or reference
./EVIDENT_PHASE5_PR5_DRAFT.md:256:  the claim's `subject_aliases`.
./EVIDENT_PHASE5_PR5_DRAFT.md:259:- If a claim ends up with zero valid tolerances: drops the claim
./EVIDENT_PHASE5_PR5_DRAFT.md:263:emit anything, but only validator-approved tolerances reach the
./EVIDENT_PHASE5_PR5_DRAFT.md:289:  tolerances and the validator rejects them, manifest is empty,
./EVIDENT_PHASE5_PR5_DRAFT.md:353:- model-response processor moves invalid tolerances to rejections
./EVIDENT_PHASE5_PR5_DRAFT.md:355:- claim with zero valid tolerances is dropped
./EVIDENT_PHASE5_PR5_DRAFT.md:368:  (needs a `metadata_compatibility` claim kind that doesn't fit
./experiments/phase5-extraction-rate/curation/_template.yaml:39:  - reason: bound_not_stated | comparator_bound_to_wrong_subject | ...
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:1:# Phase 5 extraction — rustims subject (2026-06-03)
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:3:First subject for the Phase 5 extraction-rate experiment: the
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:25:`comparator_bound_to_wrong_subject` and similar. Across two
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:30:(`hedged_qualitative_only`, `comparator_bound_to_wrong_subject`).
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:56:| `comparator_bound_to_wrong_subject` | 2 | "we observed 3-4% for Spectronaut, up to 5% for DIA-NN v1.8" — FDR observations about third-party tools on simulated data, not bounds on the paper's own system (timsim/rustims). |
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:64:| `comparator_bound_to_wrong_subject` | 4 | "FDR ≤ 0.01 for peptides" inside PEAKS/FragPipe analysis configuration. Bounds describe filter thresholds applied within external tools, not the paper's subject artifact. |
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:72:1. **Benchmark-subject conflation** (`comparator_bound_to_wrong_subject`,
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:79:   to the paper's subject.
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:113:  `kind: behavioral_concordance` — that doesn't require an
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:124:This is **one subject**. The plan called for 8–12 artifacts to
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:127:1. **Add more subjects** with different paper styles (a methods
./experiments/phase5-extraction-rate/RUSTIMS_RUN.md:132:   the next subjects to probe specific hypotheses (e.g., "do
./EVIDENT_REPHRASE.code-review.md:14:Code-review this Python PR as an independent engineer. Context: EVIDENT's evident-agent walkthrough gains a fourth decision: rephrase. Curator picks 'r' in the prompt, the walkthrough spawns $EDITOR on the claim YAML, validates the edits, and writes back atomically. Locked fields (id, tier, evidence, provenance, kind, last_verified) cannot change via rephrase — those require typed paths. Editable fields are a fixed allowlist (title, claim, tolerances, case, source, assumptions, failure_modes). Pre/post sha pair recorded for audit.
./EVIDENT_REPHRASE.code-review.md:66:+    Editable fields: title, claim, tolerances, case, source,
./EVIDENT_REPHRASE.code-review.md:68:+    evidence, provenance, last_verified) must NOT be changed —
./EVIDENT_REPHRASE.code-review.md:154:+    "tolerances",
./EVIDENT_REPHRASE.code-review.md:170:+    "last_verified",
./EVIDENT_REPHRASE.code-review.md:235:+      ``last_verified`` must not change. Those fields require typed
./EVIDENT_REPHRASE.code-review.md:569:+                "tolerances": [
./EVIDENT_REPHRASE.code-review.md:578:+                    "oracle": ["Paper-Authority"],
./EVIDENT_REPHRASE.code-review.md:634:+        parsed["tolerances"][0]["prose"] = "rephrased tolerance prose"
./EVIDENT_REPHRASE.code-review.md:642:+    assert sorted(result.fields_changed) == ["claim", "tolerances"]
./EVIDENT_REPHRASE.code-review.md:1320:evident-agent/tests/test_rephrase.py:110:        parsed["tolerances"][0]["prose"] = "rephrased tolerance prose"
./EVIDENT_REPHRASE.code-review.md:1638:    for i, t in enumerate(claim.get("tolerances") or []):
./EVIDENT_REPHRASE.code-review.md:1869:    "tolerances",
./EVIDENT_REPHRASE.code-review.md:1885:    "last_verified",
./EVIDENT_REPHRASE.code-review.md:2243:      ``last_verified`` must not change. Those fields require typed
./EVIDENT_REPHRASE.code-review.md:2548:  1032	    Editable fields: title, claim, tolerances, case, source,
./EVIDENT_REPHRASE.code-review.md:2550:  1034	    evidence, provenance, last_verified) must NOT be changed —
./EVIDENT_REPHRASE.code-review.md:2630:   512	      ``last_verified`` must not change. Those fields require typed
./EVIDENT_REPHRASE.code-review.md:2750:  [_validate_rephrase_edits](/scratch/TMAlign/evident/evident-agent/evident_agent/curator.py:603) builds `all_keys`, but compares with `original.get(k) != edited.get(k)`. That treats an absent key and an explicit `null` as equal. A curator can add `last_verified: null` or `summary: null`, and it will not be rejected, then [rephrase_claim writes `edited_claim`](/scratch/TMAlign/evident/evident-agent/evident_agent/curator.py:574). This weakens both locked-field enforcement and the closed allowlist. Use a sentinel: `original.get(k, MISSING) != edited.get(k, MISSING)`.
./EVIDENT_REPHRASE.code-review.md:2780:  [_validate_rephrase_edits](/scratch/TMAlign/evident/evident-agent/evident_agent/curator.py:603) builds `all_keys`, but compares with `original.get(k) != edited.get(k)`. That treats an absent key and an explicit `null` as equal. A curator can add `last_verified: null` or `summary: null`, and it will not be rejected, then [rephrase_claim writes `edited_claim`](/scratch/TMAlign/evident/evident-agent/evident_agent/curator.py:574). This weakens both locked-field enforcement and the closed allowlist. Use a sentinel: `original.get(k, MISSING) != edited.get(k, MISSING)`.
./EVIDENT_PHASE5_PR1.codex-review.md:51:+///   `evidence.command`. Sidecar `last_verified` gets populated by
./EVIDENT_PHASE5_PR1.codex-review.md:97:+    /// Replay depends on human raters / subjective evaluation.
./EVIDENT_PHASE5_PR1.codex-review.md:153:     pub oracle: Vec<String>,
./EVIDENT_PHASE5_PR1.codex-review.md:298: /// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./EVIDENT_PHASE5_PR1.codex-review.md:300: /// - `last_verified` is absent;
./EVIDENT_PHASE5_PR1.codex-review.md:329:+    tolerances:
./EVIDENT_PHASE5_PR1.codex-review.md:335:+      oracle: [Paper-Authority]
./EVIDENT_PHASE5_PR1.codex-review.md:346:+    tolerances:
./EVIDENT_PHASE5_PR1.codex-review.md:352:+      oracle: [Biopython]
./EVIDENT_PHASE5_PR1.codex-review.md:408:             oracle: vec!["BALL".into()],
./EVIDENT_PHASE5_PR1.codex-review.md:415:         last_verified: None,
./EVIDENT_PHASE5_PR1.codex-review.md:470:+    tolerances:
./EVIDENT_PHASE5_PR1.codex-review.md:478:+      oracle: [Paper-Authority]
./EVIDENT_PHASE5_PR1.codex-review.md:484:+    last_verified:
./EVIDENT_PHASE5_PR1.codex-review.md:495:+    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PHASE5_PR1.codex-review.md:516:+    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PHASE5_PR1.codex-review.md:541:+    tolerances:
./EVIDENT_PHASE5_PR1.codex-review.md:548:+      oracle: [Manual]
./EVIDENT_PHASE5_PR1.codex-review.md:556:+    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PHASE5_PR1.codex-review.md:583:+    tolerances:
./EVIDENT_PHASE5_PR1.codex-review.md:590:+      oracle: [Manual]
./EVIDENT_PHASE5_PR1.codex-review.md:597:+    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PHASE5_PR1.codex-review.md:622:+    tolerances:
./EVIDENT_PHASE5_PR1.codex-review.md:629:+      oracle: [Manual]
./EVIDENT_PHASE5_PR1.codex-review.md:636:+    let criteria = translate_tolerances(mc).unwrap();
./EVIDENT_PHASE5_PR1.codex-review.md:675:+    tolerances:
./EVIDENT_PHASE5_PR1.codex-review.md:682:+      oracle: [Manual]
./EVIDENT_PHASE5_PR1.codex-review.md:693:+        let criteria = translate_tolerances(mc).unwrap();
./experiments/phase5-extraction-rate/artifacts/repo-rustims/source.yaml:6:git_subject: "test(midia): pytest covering writer duplicate-Id guard"
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:12:> added (comparator + bound + metric + subject must co-occur in
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:175:   (d) the claimed subject ("ours", "the proposed method",
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:179:   different number or a different subject in the same span does
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:190:   bound to the *baseline* subject, not *our method*.
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:216:   - `wrong_subject_binding.md`: explicitly contains
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:219:     `rejected: comparator_bound_to_wrong_subject`.
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:254:  oracle: [Paper-Authority]
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:295:  / subjective evaluation. The framework's replay loop cannot
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:296:  consume this oracle.
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:417:    tolerances:
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:432:      oracle: [Paper-Authority]
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:437:    last_verified:
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:451:  rejects tolerances without one.
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:516:        wrong_subject_binding.md    # (v3) comparator bound to wrong subject
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:576:  `wrong_subject_binding.md` fixture (`"baseline error is below
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:696:   same source-span discipline as tolerances. Assumption strings
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:766:rule that requires comparator + bound + metric + subject to
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:768:(`prose_says_better.md` + `wrong_subject_binding.md`).
./EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md:773:to the claimed subject — the corpus loses its honesty
./EVIDENT_RECORD_FIXTURES.code-review.md:183:     last_verified_sidecar_path: Optional[Path],
./EVIDENT_RECORD_FIXTURES.code-review.md:482:    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./EVIDENT_RECORD_FIXTURES.code-review.md:520:        sidecar_path = manifest_path.parent / "last_verified.json"
./EVIDENT_RECORD_FIXTURES.code-review.md:659:    "last_verified_sidecar_path",
./EVIDENT_RECORD_FIXTURES.code-review.md:663:        "Path to a last_verified.json sidecar to overlay onto each claim "
./EVIDENT_RECORD_FIXTURES.code-review.md:664:        "before producing the digest. Default: manifest.parent / 'last_verified.json' if it exists."
./EVIDENT_RECORD_FIXTURES.code-review.md:702:    last_verified_sidecar_path: Optional[Path],
./EVIDENT_RECORD_FIXTURES.code-review.md:721:    if last_verified_sidecar_path is None:
./EVIDENT_RECORD_FIXTURES.code-review.md:722:        candidate = manifest_path.parent / "last_verified.json"
./EVIDENT_RECORD_FIXTURES.code-review.md:724:            last_verified_sidecar_path = candidate
./EVIDENT_RECORD_FIXTURES.code-review.md:735:    # Load the last_verified sidecar (if any) so per-claim verification
./EVIDENT_RECORD_FIXTURES.code-review.md:739:    last_verified_by_claim: dict[str, sidecar.LastVerifiedEntry] = {}
./EVIDENT_RECORD_FIXTURES.code-review.md:740:    if last_verified_sidecar_path is not None and last_verified_sidecar_path.is_file():
./EVIDENT_RECORD_FIXTURES.code-review.md:741:        last_verified_by_claim = sidecar.read(last_verified_sidecar_path)
./EVIDENT_RECORD_FIXTURES.code-review.md:760:        # Per-claim commit comes from (in order): the last_verified
./EVIDENT_RECORD_FIXTURES.code-review.md:761:        # sidecar entry > the manifest's inline last_verified.commit
./EVIDENT_RECORD_FIXTURES.code-review.md:764:        commit = _resolve_commit_for_claim(claim.raw, last_verified_by_claim.get(claim.id))
./EVIDENT_RECORD_FIXTURES.code-review.md:884:            sidecar_path=last_verified_sidecar_path,
./EVIDENT_RECORD_FIXTURES.code-review.md:902:    Precedence: sidecar entry > inline manifest last_verified.commit
./EVIDENT_RECORD_FIXTURES.code-review.md:909:    inline = (claim_raw.get("last_verified") or {}).get("commit")
./EVIDENT_RECORD_FIXTURES.code-review.md:916:    missing tolerances."""
./EVIDENT_RECORD_FIXTURES.code-review.md:917:    tols = claim_raw.get("tolerances") or []
./EVIDENT_RECORD_FIXTURES.code-review.md:929:    tols = claim_raw.get("tolerances") or []
./EVIDENT_RECORD_FIXTURES.code-review.md:939:    The model sees the full structured claim (tier, all tolerances,
./EVIDENT_RECORD_FIXTURES.code-review.md:940:    evidence pointer, last_verified) — multi-criterion claims must
./EVIDENT_RECORD_FIXTURES.code-review.md:1542:Escalate from Dissent to **Challenge** only when the digest contains a specific observed value that violates one of the target claim's stated tolerances. Report the violation as `{ target_criterion_id, metric, observed_value, bound, comparator, citation }` using the target's own metric name, bound, and comparator. The agent — not you — constructs the backing claim from your report. Do NOT propose new metrics, looser bounds, or trivial predicates like `observed > 0`, `row exists`, or `value is numeric` as a way to escalate. If you cannot cite a specific row/field/value that contradicts the target tolerance with its own bound, stay with Dissent.
./EVIDENT_RECORD_FIXTURES.code-review.md:1853:+    tolerances:
./EVIDENT_RECORD_FIXTURES.code-review.md:1861:+      oracle: [BALL, Tinker]
./EVIDENT_RECORD_FIXTURES.code-review.md:1864:+    last_verified:
./EVIDENT_RECORD_FIXTURES.code-review.md:1922:+    tolerances:
./EVIDENT_RECORD_FIXTURES.code-review.md:1930:+      oracle: [BALL, Tinker]
./EVIDENT_RECORD_FIXTURES.code-review.md:1933:+    last_verified:
./EVIDENT_RECORD_FIXTURES.code-review.md:1975:+    tolerances:
./EVIDENT_RECORD_FIXTURES.code-review.md:1983:+      oracle: [Biopython, FreeSASA]
./EVIDENT_RECORD_FIXTURES.code-review.md:1986:+    last_verified:
./EVIDENT_RECORD_FIXTURES.code-review.md:2193:workflow/validate_manifest.py:206:    value: Any, oracle_names: list[str], project: str, claim_id: str
./EVIDENT_RECORD_FIXTURES.code-review.md:2196:workflow/validate_manifest.py:283:def validate_last_verified(value: Any, claim_id: str) -> None:
./EVIDENT_RECORD_FIXTURES.code-review.md:2243:234:typically held in a sidecar (`last_verified.json`) keyed by claim id so
./EVIDENT_RECORD_FIXTURES.code-review.md:2310:        oracles = validate_evidence(claim["evidence"], claim_id, vocabularies)
./EVIDENT_RECORD_FIXTURES.code-review.md:2850:/bin/bash -lc "grep -n \"def validate_tolerance\\|tolerances\" -n workflow/validate_manifest.py evident-agent/evident_agent/manifest.py | head -80 && sed -n '120,180p' workflow/validate_manifest.py" in /scratch/TMAlign/evident
./EVIDENT_RECORD_FIXTURES.code-review.md:2852:workflow/validate_manifest.py:67:    "tolerances",
./EVIDENT_RECORD_FIXTURES.code-review.md:2853:workflow/validate_manifest.py:131:            f"move tolerance text into the top-level tolerances: list "
./EVIDENT_RECORD_FIXTURES.code-review.md:2854:workflow/validate_manifest.py:144:def validate_tolerances(
./EVIDENT_RECORD_FIXTURES.code-review.md:2855:workflow/validate_manifest.py:148:        fail(f"claim {claim_id}: tolerances must be a non-empty list")
./EVIDENT_RECORD_FIXTURES.code-review.md:2856:workflow/validate_manifest.py:151:            fail(f"claim {claim_id}: tolerances[{i}] must be a mapping")
./EVIDENT_RECORD_FIXTURES.code-review.md:2857:workflow/validate_manifest.py:154:            fail(f"claim {claim_id}: tolerances[{i}].prose is required")
./EVIDENT_RECORD_FIXTURES.code-review.md:2858:workflow/validate_manifest.py:162:                f"claim {claim_id}: tolerances[{i}] metric/op/value are "
./EVIDENT_RECORD_FIXTURES.code-review.md:2859:workflow/validate_manifest.py:170:                f"tolerances[{i}].metric",
./EVIDENT_RECORD_FIXTURES.code-review.md:2860:workflow/validate_manifest.py:177:                f"tolerances[{i}].op",
./EVIDENT_RECORD_FIXTURES.code-review.md:2861:workflow/validate_manifest.py:183:                fail(f"claim {claim_id}: tolerances[{i}].value must be numeric")
./EVIDENT_RECORD_FIXTURES.code-review.md:2862:workflow/validate_manifest.py:452:            validate_tolerances(claim["tolerances"], claim_id, vocabularies)
./EVIDENT_RECORD_FIXTURES.code-review.md:2868:    for field in ("oracle", "command", "artifact"):
./EVIDENT_RECORD_FIXTURES.code-review.md:2874:            f"move tolerance text into the top-level tolerances: list "
./EVIDENT_RECORD_FIXTURES.code-review.md:2877:    oracles = require_string_list(value["oracle"], "evidence.oracle", claim_id)
./EVIDENT_RECORD_FIXTURES.code-review.md:2878:    for oracle_name in oracles:
./EVIDENT_RECORD_FIXTURES.code-review.md:2880:            oracle_name, "oracle", vocabularies, "evidence.oracle[]", claim_id
./EVIDENT_RECORD_FIXTURES.code-review.md:2884:    return oracles
./EVIDENT_RECORD_FIXTURES.code-review.md:2887:def validate_tolerances(
./EVIDENT_RECORD_FIXTURES.code-review.md:2891:        fail(f"claim {claim_id}: tolerances must be a non-empty list")
./EVIDENT_RECORD_FIXTURES.code-review.md:2894:            fail(f"claim {claim_id}: tolerances[{i}] must be a mapping")
./EVIDENT_RECORD_FIXTURES.code-review.md:2897:            fail(f"claim {claim_id}: tolerances[{i}].prose is required")
./EVIDENT_RECORD_FIXTURES.code-review.md:2905:                f"claim {claim_id}: tolerances[{i}] metric/op/value are "
./EVIDENT_RECORD_FIXTURES.code-review.md:2913:                f"tolerances[{i}].metric",
./EVIDENT_RECORD_FIXTURES.code-review.md:2920:                f"tolerances[{i}].op",
./EVIDENT_RECORD_FIXTURES.code-review.md:3101:./viewer/dist/_astro/hoisted.q6C1j76E.js:1:const C={relative_error:"lower",median_relative_error:"lower",absolute_error:"lower",drift:"lower",pass_rate:"higher",recall:"higher",precision:"higher",f1:"higher"};function v(e){return typeof e=="string"&&e.startsWith("PENDING-")}function b(e){return e==="<"||e==="<="}function f(e){return e===">"||e===">="}function q(e,t){if(!e.tolerances)return!1;const n=C[t.metric];if(!n)return!1;for(const s of e.tolerances)if(s.metric===t.metric&&!(s.op==null||s.value==null))if(n==="lower"){if(!b(s.op)||!b(t.op))continue;if(s.value<=t.value)return!0}else{if(!f(s.op)||!f(t.op))continue;if(s.value>=t.value)return!0}return!1}function I(e,t){const n=[];if(t.tiers?.length&&!t.tiers.includes(e.tier)&&n.push(`tier (claim is ${e.tier})`),t.subsystems?.length&&(!e.subsystem||!t.subsystems.includes(e.subsystem))&&n.push(e.subsystem?`subsystem (claim is ${e.subsystem})`:"subsystem (claim has none)"),t.oracles?.length){const s=new Set(e.evidence?.oracle??[]);t.oracles.some(c=>s.has(c))||n.push(`oracle (claim has ${[...s].join(", ")||"none"})`)}if(t.capabilities?.length){const s=new Set(e.capabilities??[]);t.capabilities.some(c=>s.has(c))||n.push("capability (none of the requested ones)")}if(t.inputClasses?.length){const s=e.inputs?.class;(!s||!t.inputClasses.includes(s))&&n.push(s?`input class (claim is ${s})`:"input class (claim has none)")}if(t.tolerance&&(q(e,t.tolerance)||n.push(`tolerance ${t.tolerance.metric} ${t.tolerance.op} ${t.tolerance.value}`)),t.requirePinned){const s=e.pinned_versions??{},i=Object.values(s).some(v),c=v(e.inputs?.corpus_sha);(i||c)&&n.push("pinning (claim has placeholder versions)")}return n}function j(e,t){const n=[],s=t.filter(i=>i.kind==="measurement");for(const i of e.subsystems??[]){const c=e.tiers?.length?e.tiers:["ci","release","research"];for(const a of c)s.some(l=>l.subsystem===i&&l.tier===a)||n.push(`No measurement claim for subsystem \`${i}\` at tier \`${a}\`.`)}for(const i of e.capabilities??[])s.some(a=>a.capabilities?.includes(i))||n.push(`No claim advertises capability \`${i}\`.`);return e.tolerance&&(s.some(c=>c.tolerances?.some(a=>a.metric===e.tolerance.metric))||n.push(`No claim measures \`${e.tolerance.metric}\`.`)),n}function w(e,t){const n=e.filter(a=>a.kind==="measurement"),s=[],i=[],c=[];for(const a of n){const o=I(a,t);o.length===0?s.push(a):o.length===1?i.push({claim:a,missing:o}):c.push({claim:a,missing:o})}return{matched:s,partial:i,rejected:c,gaps:j(t,e)}}function B(e){const t=y=>{const d=e.get(y);return d?d.split(",").filter(Boolean):void 0},n=t("subsystem"),s=t("tier"),i=t("oracle"),c=t("capability"),a=t("class"),o=e.get("metric"),l=e.get("op"),m=e.get("value"),p=o&&l&&m!=null&&!Number.isNaN(parseFloat(m))?{metric:o,op:l,value:parseFloat(m)}:void 0,g=e.get("pinned")==="1";return{subsystems:n,tiers:s,oracles:i,capabilities:c,inputClasses:a,tolerance:p,requirePinned:g||void 0}}function L(e){const t=new URLSearchParams;e.subsystems?.length&&t.set("subsystem",e.subsystems.join(",")),e.tiers?.length&&t.set("tier",e.tiers.join(",")),e.oracles?.length&&t.set("oracle",e.oracles.join(",")),e.capabilities?.length&&t.set("capability",e.capabilities.join(",")),e.inputClasses?.length&&t.set("class",e.inputClasses.join(",")),e.tolerance&&(t.set("metric",e.tolerance.metric),t.set("op",e.tolerance.op),t.set("value",String(e.tolerance.value))),e.requirePinned&&t.set("pinned","1");const n=t.toString();return n?`?${n}`:""}const P=document.getElementById("claims-data"),x=JSON.parse(P?.textContent??"[]"),r=document.getElementById("composer-form");function $(){const e=y=>Array.from(r.querySelectorAll(`[data-field="${y}"] input[type=checkbox]:checked`)).map(d=>d.value),t=e("subsystem"),n=e("tier"),s=e("oracle"),i=e("capability"),c=e("class"),a=r.querySelector("#tol-metric").value,o=r.querySelector("#tol-op").value,l=r.querySelector("#tol-value").value,m=l===""?NaN:parseFloat(l),p=a&&o&&!Number.isNaN(m)?{metric:a,op:o,value:m}:void 0,g=r.querySelector("#require-pinned").checked||void 0;return{subsystems:t.length?t:void 0,tiers:n&&n.length?n:void 0,oracles:s.length?s:void 0,capabilities:i.length?i:void 0,inputClasses:c.length?c:void 0,tolerance:p,requirePinned:g}}function N(e){const t=(n,s)=>{const i=new Set(s??[]);r.querySelectorAll(`[data-field="${n}"] input[type=checkbox]`).forEach(c=>{c.checked=i.has(c.value)})};t("subsystem",e.subsystems),t("tier",e.tiers),t("oracle",e.oracles),t("capability",e.capabilities),t("class",e.inputClasses),r.querySelector("#tol-metric").value=e.tolerance?.metric??"",r.querySelector("#tol-op").value=e.tolerance?.op??"<=",r.querySelector("#tol-value").value=e.tolerance&&!Number.isNaN(e.tolerance.value)?String(e.tolerance.value):"",r.querySelector("#require-pinned").checked=!!e.requirePinned}function u(e){return e.replace(/[&<>"']/g,t=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"})[t])}function S(e,t){const n=`<span class="badge tier-${e.tier}">${e.tier}</span>`,s=e.subsystem?`<code>${u(e.subsystem)}</code>`:'<span class="gap">—</span>',i=t&&t.length?`<div class="missing">missing: ${t.map(u).join("; ")}</div>`:"";return`
./EVIDENT_RECORD_FIXTURES.code-review.md:3143:  oracle:
./EVIDENT_RECORD_FIXTURES.code-review.md:3165:      oracle:
./typed-trust-codex-review-11.md:238:+enum ClaimKind {
./typed-trust-codex-review-11.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-11.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-11.md:368:+                                        // tolerances
./typed-trust-codex-review-11.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-11.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-11.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-11.md:580:+``last_verified.json`` (framework convention) which typed-trust then
./typed-trust-codex-review-11.md:668:+    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./typed-trust-codex-review-11.md:706:+        sidecar_path = manifest_path.parent / "last_verified.json"
./typed-trust-codex-review-11.md:823:+oracle binaries available.
./typed-trust-codex-review-11.md:1103:+        if not score.tolerances:
./typed-trust-codex-review-11.md:1106:+        first = score.tolerances[0]
./typed-trust-codex-review-11.md:1174:+"""Sidecar ``last_verified.json`` read/write.
./typed-trust-codex-review-11.md:1378:+#  - Writes a sidecar entry in workflow/evident.py's last_verified.json
./typed-trust-codex-review-11.md:1387:+SIDECAR="/scratch/TMAlign/proteon/evident/last_verified.json"
./typed-trust-codex-review-11.md:1469:+                tolerances:
./typed-trust-codex-review-11.md:1475:+                  oracle: [Test]
./typed-trust-codex-review-11.md:1487:+    sidecar = tmp_path / "last_verified.json"
./typed-trust-codex-review-11.md:1511:+    sidecar = tmp_path / "last_verified.json"
./typed-trust-codex-review-11.md:1837:+    path = tmp_path / "last_verified.json"
./typed-trust-codex-review-11.md:2019:+pub enum ClaimKind {
./typed-trust-codex-review-11.md:3205:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-11.md:3253:+    // Overlay sidecar entries onto each claim's last_verified field
./typed-trust-codex-review-11.md:3261:+                        cw.claim.last_verified = Some(lv.clone());
./typed-trust-codex-review-11.md:3305:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-11.md:3308:+                // All translate_tolerances errors at this point are
./typed-trust-codex-review-11.md:3451:+    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./typed-trust-codex-review-11.md:3696:+/// The shape matches `workflow/evident.py`'s `last_verified.json`
./typed-trust-codex-review-11.md:3713:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-11.md:4209:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-11.md:4232:+///   `last_verified`).
./typed-trust-codex-review-11.md:4853:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-11.md:4854:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-11.md:4855:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-11.md:4863:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-11.md:4897:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-11.md:4909:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-11.md:4912:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-11.md:4944:+    pub oracle: Vec<String>,
./typed-trust-codex-review-11.md:4965:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-11.md:4974:+    /// release claims must carry structured tolerances.
./typed-trust-codex-review-11.md:4976:+    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-11.md:4978:+    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-11.md:5012:+                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-11.md:5013:+                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-11.md:5105:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-11.md:5109:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-11.md:5110:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-11.md:5112:+pub fn translate_tolerances(
./typed-trust-codex-review-11.md:5115:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-11.md:5116:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-11.md:5117:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-11.md:5123:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-11.md:5124:+        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-11.md:5144:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-11.md:5156:+    single_oracle: &Option<String>,
./typed-trust-codex-review-11.md:5165:+        // tolerances; allowing them to translate would let
./typed-trust-codex-review-11.md:5182:+            against: single_oracle.clone(),
./typed-trust-codex-review-11.md:5195:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-11.md:5196:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-11.md:5198:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-11.md:5213:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-11.md:5215:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-11.md:5236:+    let reruns = translate_last_verified(
./typed-trust-codex-review-11.md:5237:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-11.md:5274:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-11.md:5276:+/// - `last_verified` is absent;
./typed-trust-codex-review-11.md:5277:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-11.md:5278:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-11.md:5283:+fn translate_last_verified(
./typed-trust-codex-review-11.md:5309:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-11.md:5376:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-11.md:5377:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-11.md:5384:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-11.md:5795:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-11.md:5955:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-11.md:5973:+    tolerances:
./typed-trust-codex-review-11.md:5981:+      oracle:
./typed-trust-codex-review-11.md:5986:+    last_verified:
./typed-trust-codex-review-11.md:6007:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-11.md:6162:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-11.md:6180:+    tolerances:
./typed-trust-codex-review-11.md:6188:+      oracle:
./typed-trust-codex-review-11.md:6193:+    last_verified:
./typed-trust-codex-review-11.md:6214:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-11.md:6559:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-11.md:6581:+    // populated to the oracle name.
./typed-trust-codex-review-11.md:6627:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-11.md:6735:+               within tier-specific tolerances."
./typed-trust-codex-review-11.md:6747:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-11.md:6794:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-11.md:6811:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-11.md:6822:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-11.md:6906:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-11.md:6912:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-11.md:7022:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-11.md:7028:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-11.md:7029:+/// translation working) with last_verified populated.
./typed-trust-codex-review-11.md:7043:+    tolerances:
./typed-trust-codex-review-11.md:7051:+      oracle:
./typed-trust-codex-review-11.md:7056:+    last_verified:
./typed-trust-codex-review-11.md:7065:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-11.md:7079:+    tolerances:
./typed-trust-codex-review-11.md:7086:+      oracle:
./typed-trust-codex-review-11.md:7091:+    last_verified:
./typed-trust-codex-review-11.md:7111:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-11.md:7159:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-11.md:7161:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-11.md:7162:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-11.md:8481:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-11.md:8486:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-11.md:8487:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-11.md:8505:+    tolerances:
./typed-trust-codex-review-11.md:8513:+      oracle:
./typed-trust-codex-review-11.md:8518:+    last_verified:
./typed-trust-codex-review-11.md:8526:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-11.md:8530:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-11.md:8531:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-11.md:8532:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-11.md:8548:+    tolerances:
./typed-trust-codex-review-11.md:8556:+      oracle:
./typed-trust-codex-review-11.md:8561:+    last_verified:
./typed-trust-codex-review-11.md:8575:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-11.md:8576:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-11.md:8586:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-11.md:8588:+      oracle:
./typed-trust-codex-review-11.md:8590:+      command: pytest tests/oracle -v
./typed-trust-codex-review-11.md:8614:+    tolerances:
./typed-trust-codex-review-11.md:8634:+      oracle:
./typed-trust-codex-review-11.md:8636:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-11.md:8661:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-11.md:8682:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-11.md:8684:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-11.md:8692:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-11.md:8693:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-11.md:8698:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-11.md:8713:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-11.md:8721:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-11.md:8725:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-11.md:8747:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-11.md:8777:+    tolerances:
./typed-trust-codex-review-11.md:8783:+      oracle: [SomeOracle]
./typed-trust-codex-review-11.md:8788:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-11.md:8814:+    tolerances:
./typed-trust-codex-review-11.md:8820:+      oracle: [internal]
./typed-trust-codex-review-11.md:8821:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-11.md:8825:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-11.md:8867:+    tolerances:
./typed-trust-codex-review-11.md:8871:+      oracle: [Foo]
./typed-trust-codex-review-11.md:8872:+      command: pytest tests/oracle
./typed-trust-codex-review-11.md:8876:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-11.md:8887:+fn rejects_measurement_claim_without_tolerances() {
./typed-trust-codex-review-11.md:8888:+    // Codex round 5: kind: measurement requires non-empty tolerances per
./typed-trust-codex-review-11.md:8893:+  - id: measurement-no-tolerances
./typed-trust-codex-review-11.md:8894:+    title: missing tolerances
./typed-trust-codex-review-11.md:8901:+      oracle: [Foo]
./typed-trust-codex-review-11.md:8906:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-11.md:8909:+            assert_eq!(id, "measurement-no-tolerances");
./typed-trust-codex-review-11.md:8930:+    tolerances:
./typed-trust-codex-review-11.md:8937:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-11.md:8949:+fn rejects_measurement_claim_with_empty_tolerances_list() {
./typed-trust-codex-review-11.md:8952:+  - id: measurement-empty-tolerances
./typed-trust-codex-review-11.md:8953:+    title: empty tolerances
./typed-trust-codex-review-11.md:8959:+    tolerances: []
./typed-trust-codex-review-11.md:8961:+      oracle: [Foo]
./typed-trust-codex-review-11.md:8966:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-11.md:8984:+    tolerances:
./typed-trust-codex-review-11.md:8988:+      oracle: [Foo]
./typed-trust-codex-review-11.md:8989:+      command: pytest tests/oracle
./typed-trust-codex-review-11.md:8993:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-11.md:9014:+    tolerances:
./typed-trust-codex-review-11.md:9019:+      oracle: [Foo]
./typed-trust-codex-review-11.md:9024:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-11.md:9040:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-11.md:9043:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-11.md:9046:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-11.md:9049:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-11.md:9056:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-11.md:9085:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-11.md:9088:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-11.md:9105:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-11.md:9119:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-11.md:9133:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-11.md:9149: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-11.md:9155: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-11.md:9166:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-11.md:9188:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-11.md:9414:        if not score.tolerances:
./typed-trust-codex-review-11.md:9417:        first = score.tolerances[0]
./typed-trust-codex-review-11.md:9549:    help="Sidecar path. Default: manifest.parent / 'last_verified.json'.",
./typed-trust-codex-review-11.md:9587:        sidecar_path = manifest_path.parent / "last_verified.json"
./typed-trust-codex-review-11.md:9702:oracle binaries available.
./typed-trust-codex-review-11.md:9823:"""Sidecar ``last_verified.json`` read/write.
./typed-trust-codex-review-11.md:10013://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-11.md:10014://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-11.md:10015://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-11.md:10023://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-11.md:10057:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-11.md:10069:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-11.md:10072:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-11.md:10104:    pub oracle: Vec<String>,
./typed-trust-codex-review-11.md:10125:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-11.md:10134:    /// release claims must carry structured tolerances.
./typed-trust-codex-review-11.md:10136:    /// A `kind: measurement` claim omitted `tolerances` or provided
./typed-trust-codex-review-11.md:10138:    /// tolerances on measurement claims; without them the
./typed-trust-codex-review-11.md:10172:                "claim {id}: kind=measurement requires non-empty tolerances; \
./typed-trust-codex-review-11.md:10173:                 add tolerances or change to kind: policy / reference"
./typed-trust-codex-review-11.md:10270:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-11.md:10274:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-11.md:10275:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-11.md:10277:pub fn translate_tolerances(
./typed-trust-codex-review-11.md:10280:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-11.md:10281:        if e.oracle.len() == 1 {
./typed-trust-codex-review-11.md:10282:            Some(e.oracle[0].clone())
./typed-trust-codex-review-11.md:10288:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-11.md:10289:        // Measurement claims require non-empty tolerances per
./typed-trust-codex-review-11.md:10309:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id, &mc.tier)?;
./typed-trust-codex-review-11.md:10321:    single_oracle: &Option<String>,
./typed-trust-codex-review-11.md:10330:        // tolerances; allowing them to translate would let
./typed-trust-codex-review-11.md:10347:            against: single_oracle.clone(),
./typed-trust-codex-review-11.md:10360:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-11.md:10361:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-11.md:10363:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-11.md:10378:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-11.md:10380:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-11.md:10401:    let reruns = translate_last_verified(
./typed-trust-codex-review-11.md:10402:        mc.last_verified.as_ref(),
./typed-trust-codex-review-11.md:10439:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-11.md:10441:/// - `last_verified` is absent;
./typed-trust-codex-review-11.md:10442:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-11.md:10443:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-11.md:10448:fn translate_last_verified(
./typed-trust-codex-review-11.md:10474:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-11.md:10541:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-11.md:10542:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-11.md:10549:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-11.md:10596:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-11.md:10644:    // Overlay sidecar entries onto each claim's last_verified field
./typed-trust-codex-review-11.md:10652:                        cw.claim.last_verified = Some(lv.clone());
./typed-trust-codex-review-11.md:10696:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-11.md:10699:                // All translate_tolerances errors at this point are
./typed-trust-codex-review-11.md:10834://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-11.md:10857:///   `last_verified`).
./typed-trust-codex-review-11.md:11520:    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./typed-trust-codex-review-11.md:11765:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./typed-trust-codex-review-11.md:11770:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./typed-trust-codex-review-11.md:12135:                tolerances:
./typed-trust-codex-review-11.md:12141:                  oracle: [Test]
./typed-trust-codex-review-11.md:12153:    sidecar = tmp_path / "last_verified.json"
./typed-trust-codex-review-11.md:12177:    sidecar = tmp_path / "last_verified.json"
./typed-trust-codex-review-11.md:12262:/bin/bash -lc 'ls workflow && rg "allowed|fields|signature|last_verified|tolerances|evidence" workflow -n' in /scratch/TMAlign/evident
./typed-trust-codex-review-11.md:12271:workflow/evident.py:8:  replay      re-execute claims' evidence.command and update last_verified
./typed-trust-codex-review-11.md:12273:workflow/evident.py:46:        "oracles": list(evidence.get("oracle") or []),
./typed-trust-codex-review-11.md:12276:workflow/evident.py:135:    # `--stale DAYS`. Reads `last_verified.json` next to the manifest;
./typed-trust-codex-review-11.md:12279:workflow/evident.py:299:        add("    tolerances:")
./typed-trust-codex-review-11.md:12281:workflow/evident.py:327:    add("    last_verified:")
./typed-trust-codex-review-11.md:12282:workflow/evident.py:393:# `replay` — re-execute claims' evidence.command and update last_verified
./typed-trust-codex-review-11.md:12283:workflow/evident.py:395:# Closes the loop the schema's last_verified.{date, commit, value, corpus_sha}
./typed-trust-codex-review-11.md:12284:workflow/evident.py:400:# Storage decision: a sidecar `last_verified.json` file next to the manifest,
./typed-trust-codex-review-11.md:12285:workflow/evident.py:417:# etc.) and is its own design. Until that lands, last_verified.value stays
./typed-trust-codex-review-11.md:12286:workflow/evident.py:423:SIDECAR_FILENAME = "last_verified.json"
./typed-trust-codex-review-11.md:12287:workflow/evident.py:453:    Used to fill `commit` in last_verified entries. Returning None when
./typed-trust-codex-review-11.md:12290:workflow/evident.py:614:        # last_verified.commit as the SOURCE SHA where the claim
./typed-trust-codex-review-11.md:12294:workflow/evident.py:704:            "Filter to claims whose sidecar last_verified.date is older than "
./typed-trust-codex-review-11.md:12295:workflow/evident.py:705:            "DAYS (or absent). Reads `last_verified.json` next to the "
./typed-trust-codex-review-11.md:12296:workflow/evident.py:713:        help="Re-execute claims' evidence.command and update last_verified",
./typed-trust-codex-review-11.md:12298:workflow/evident.py:717:            "write to a sidecar `last_verified.json`. The sidecar is the "
./typed-trust-codex-review-11.md:12300:workflow/evident.py:796:        help="Add an oracle name to evidence.oracle (repeatable)",
./typed-trust-codex-review-11.md:12306:workflow/GRAMMAR.md:42:of the claim from `subsystem`, `outputs`, `inputs`, `tolerances`, and
./typed-trust-codex-review-11.md:12309:workflow/GRAMMAR.md:94:Research claims may carry prose-only tolerances and unspecified
./typed-trust-codex-review-11.md:12311:workflow/GRAMMAR.md:110:| `last_verified` populated                   | optional   | recommended | required |
./typed-trust-codex-review-11.md:12313:workflow/GRAMMAR.md:134:  is comparable to the recorded `last_verified.value`. If the command
./typed-trust-codex-review-11.md:12315:workflow/GRAMMAR.md:169:  `pinned_versions` and `evidence.oracle`, not free-form in
./typed-trust-codex-review-11.md:12316:workflow/GRAMMAR.md:188:  from `tolerances`+`last_verified`, conditioning from
./typed-trust-codex-review-11.md:12321:workflow/SCHEMA.md:89:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-11.md:12322:workflow/SCHEMA.md:90:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-11.md:12323:workflow/SCHEMA.md:93:| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-11.md:12325:workflow/SCHEMA.md:99:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-11.md:12326:workflow/SCHEMA.md:111:  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review-11.md:12327:workflow/SCHEMA.md:120:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-11.md:12328:workflow/SCHEMA.md:129:tolerances:
./typed-trust-codex-review-11.md:12329:workflow/SCHEMA.md:168:Each named entry can be referenced by `tolerances[].output` so a single
./typed-trust-codex-review-11.md:12330:workflow/SCHEMA.md:177:  Biopython: "1.83"              # one entry per oracle named in evidence.oracle
./typed-trust-codex-review-11.md:12331:workflow/SCHEMA.md:181:The validator requires that every name in `evidence.oracle` appears as a
./typed-trust-codex-review-11.md:12332:workflow/SCHEMA.md:199:underlying numbers are correct (that is what `tolerances` and
./typed-trust-codex-review-11.md:12333:workflow/SCHEMA.md:200:`last_verified` are for). The three levels:
./typed-trust-codex-review-11.md:12336:workflow/SCHEMA.md:219:replace the underlying `tolerances` and `evidence` fields.
./typed-trust-codex-review-11.md:12337:workflow/SCHEMA.md:224:last_verified:
./typed-trust-codex-review-11.md:12339:workflow/SCHEMA.md:234:typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-11.md:12345:workflow/SCHEMA.md:267:- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-11.md:12353:workflow/SCHEMA.md:307:1. Lift `evidence.tolerance` into one or more `tolerances:` entries.
./typed-trust-codex-review-11.md:12354:workflow/SCHEMA.md:312:   measurements (and they may then drop `subsystem` and `tolerances`).
./typed-trust-codex-review-11.md:12360:workflow/validate_manifest.py:67:    "tolerances",
./typed-trust-codex-review-11.md:12366:workflow/validate_manifest.py:131:            f"move tolerance text into the top-level tolerances: list "
./typed-trust-codex-review-11.md:12367:workflow/validate_manifest.py:134:    oracles = require_string_list(value["oracle"], "evidence.oracle", claim_id)
./typed-trust-codex-review-11.md:12368:workflow/validate_manifest.py:137:            oracle_name, "oracle", vocabularies, "evidence.oracle[]", claim_id
./typed-trust-codex-review-11.md:12371:workflow/validate_manifest.py:144:def validate_tolerances(
./typed-trust-codex-review-11.md:12372:workflow/validate_manifest.py:148:        fail(f"claim {claim_id}: tolerances must be a non-empty list")
./typed-trust-codex-review-11.md:12373:workflow/validate_manifest.py:151:            fail(f"claim {claim_id}: tolerances[{i}] must be a mapping")
./typed-trust-codex-review-11.md:12374:workflow/validate_manifest.py:154:            fail(f"claim {claim_id}: tolerances[{i}].prose is required")
./typed-trust-codex-review-11.md:12375:workflow/validate_manifest.py:162:                f"claim {claim_id}: tolerances[{i}] metric/op/value are "
./typed-trust-codex-review-11.md:12376:workflow/validate_manifest.py:170:                f"tolerances[{i}].metric",
./typed-trust-codex-review-11.md:12377:workflow/validate_manifest.py:177:                f"tolerances[{i}].op",
./typed-trust-codex-review-11.md:12378:workflow/validate_manifest.py:183:                fail(f"claim {claim_id}: tolerances[{i}].value must be numeric")
./typed-trust-codex-review-11.md:12379:workflow/validate_manifest.py:227:            f"named in evidence.oracle; missing: {missing_oracles}"
./typed-trust-codex-review-11.md:12381:workflow/validate_manifest.py:283:def validate_last_verified(value: Any, claim_id: str) -> None:
./typed-trust-codex-review-11.md:12382:workflow/validate_manifest.py:285:        fail(f"claim {claim_id}: last_verified must be a mapping")
./typed-trust-codex-review-11.md:12383:workflow/validate_manifest.py:293:                fail(f"claim {claim_id}: last_verified.value must be numeric or null")
./typed-trust-codex-review-11.md:12384:workflow/validate_manifest.py:295:            fail(f"claim {claim_id}: last_verified.{key} must be a string or null")
./typed-trust-codex-review-11.md:12387:workflow/validate_manifest.py:419:        oracles = validate_evidence(claim["evidence"], claim_id, vocabularies)
./typed-trust-codex-review-11.md:12388:workflow/validate_manifest.py:434:        if "last_verified" in claim:
./typed-trust-codex-review-11.md:12389:workflow/validate_manifest.py:435:            validate_last_verified(claim["last_verified"], claim_id)
./typed-trust-codex-review-11.md:12391:workflow/validate_manifest.py:452:            validate_tolerances(claim["tolerances"], claim_id, vocabularies)
./typed-trust-codex-review-11.md:12401:still need their own oracle or benchmark commands.
./typed-trust-codex-review-11.md:12437:    "oracle": set(),
./typed-trust-codex-review-11.md:12462:    "tolerances",
./typed-trust-codex-review-11.md:12565:        oracles = validate_evidence(claim["evidence"], claim_id, vocabularies)
./typed-trust-codex-review-11.md:12580:        if "last_verified" in claim:
./typed-trust-codex-review-11.md:12581:            validate_last_verified(claim["last_verified"], claim_id)
./typed-trust-codex-review-11.md:12598:            validate_tolerances(claim["tolerances"], claim_id, vocabularies)
./typed-trust-codex-review-11.md:12600:                claim["pinned_versions"], oracles, project, claim_id
./typed-trust-codex-review-11.md:12611:SIDECAR_FILENAME = "last_verified.json"
./typed-trust-codex-review-11.md:12641:    Used to fill `commit` in last_verified entries. Returning None when
./typed-trust-codex-review-11.md:12802:        # last_verified.commit as the SOURCE SHA where the claim
./typed-trust-codex-review-11.md:14074:    85	                        cw.claim.last_verified = Some(lv.clone());
./typed-trust-codex-review-11.md:14118:   129	        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-11.md:14121:   132	                // All translate_tolerances errors at this point are
./EVIDENT_DESIGN_v0.6.codex-review.md:182:The shipping `last_verified: {commit, date, value, corpus_sha}` maps to
./EVIDENT_DESIGN_v0.6.codex-review.md:190:This is the change that makes `last_verified` natively expressible.
./EVIDENT_DESIGN_v0.6.codex-review.md:247:enum ClaimKind {
./EVIDENT_DESIGN_v0.6.codex-review.md:369:The relationship to the shipping manifest's `tolerances` list:
./EVIDENT_DESIGN_v0.6.codex-review.md:373:| `tolerances[].metric` | `Criterion.tolerance.metric` |
./EVIDENT_DESIGN_v0.6.codex-review.md:374:| `tolerances[].op` | `Criterion.tolerance.op` |
./EVIDENT_DESIGN_v0.6.codex-review.md:375:| `tolerances[].value` | `Criterion.tolerance.value` |
./EVIDENT_DESIGN_v0.6.codex-review.md:376:| `tolerances[].output` | `Criterion.tolerance.output` |
./EVIDENT_DESIGN_v0.6.codex-review.md:377:| `tolerances[].prose` | `Criterion.tolerance.prose` |
./EVIDENT_DESIGN_v0.6.codex-review.md:431:| `tolerances[].*` | `Criterion.tolerance: Tolerance` |
./EVIDENT_DESIGN_v0.6.codex-review.md:432:| `evidence.{oracle, command, artifact}` | `Evidence { kind, locator, extraction: Verified }` |
./EVIDENT_DESIGN_v0.6.codex-review.md:434:| `last_verified.*` | latest entry of `Verified.reproductions` |
./EVIDENT_DESIGN_v0.6.codex-review.md:726:enum ClaimKind {
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/evident.yaml:13:  tolerances:
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/evident.yaml:23:    oracle:
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/evident.yaml:38:  last_verified:
./EVIDENT_DESIGN.md:17:fact. That discipline is the subject of this document.
./EVIDENT_DESIGN.md:176:enum ClaimKind {
./experiments/phase5-extraction-rate/README.md:140:  - reason: bound_not_stated | comparator_bound_to_wrong_subject | ...
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:14:        "subject_aliases": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:17:        "tolerances": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:31:        "subject_aliases": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:34:        "tolerances": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:55:        "subject_aliases": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:58:        "tolerances": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:79:        "subject_aliases": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:82:        "tolerances": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:96:        "subject_aliases": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:99:        "tolerances": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:113:        "subject_aliases": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:116:        "tolerances": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:130:        "subject_aliases": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:133:        "tolerances": [
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:148:        "reason": "comparator_bound_to_wrong_subject",
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:149:        "rationale": "Subject is generic 'several dia-PASEF workflows', not a specific named tool. The 3-5% range is also not tied to a single subject with a comparator."
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:173:        "rationale": "Conditional on 'higher complexities' (not precisely defined) and no specific measurement value tied to a clear comparator+subject in a way suitable for extraction."
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:196:        "reason": "comparator_bound_to_wrong_subject",
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:197:        "rationale": "'Up to 10% more' is a vague upper bound on a difference metric; not a cleanly tied claim of (metric, comparator, value, subject)."
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:214:        "reason": "comparator_bound_to_wrong_subject",
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json:215:        "rationale": "Subject is 'both tools' generically; not tied to a single named subject for which the validator can match a source_span."
./EVIDENT_PHASE5_PR6.codex-review.md:53:papers, ask the model to extract structured tolerances, validate
./EVIDENT_PHASE5_PR6.codex-review.md:183:  `comparator_bound_to_wrong_subject`.
./EVIDENT_PHASE5_PR6.codex-review.md:184:- `wrong_subject_binding.md` — codex-flagged: the comparator
./EVIDENT_PHASE5_PR6.codex-review.md:220:- `--paper wrong_subject_binding.md` produces 0 claims because
./EVIDENT_PHASE5_PR6.codex-review.md:310:A single 50 KiB section makes “local binding” too permissive unless downstream validation is strictly sentence/window based. PR6 should either split PDFs into page-based sections using `pdftotext` page breaks/form-feed, or make the prompt/tool schema require the model to return the specific evidence sentence/span used for each tolerance. The prompt should be explicit: values mentioned in prose are extractable only when the subject, comparator, and bound are all stated in nearby machine-readable prose; “see Table 3” alone is not enough.
./EVIDENT_PHASE5_PR6.codex-review.md:316:- `wrong_subject_binding.md` is redundant with PR4 unit coverage, but useful as PR6 integration coverage proving paper framing plus validator still compose correctly. Keep it if test runtime is cheap.
./EVIDENT_PHASE5_PR6.codex-review.md:344:A single 50 KiB section makes “local binding” too permissive unless downstream validation is strictly sentence/window based. PR6 should either split PDFs into page-based sections using `pdftotext` page breaks/form-feed, or make the prompt/tool schema require the model to return the specific evidence sentence/span used for each tolerance. The prompt should be explicit: values mentioned in prose are extractable only when the subject, comparator, and bound are all stated in nearby machine-readable prose; “see Table 3” alone is not enough.
./EVIDENT_PHASE5_PR6.codex-review.md:350:- `wrong_subject_binding.md` is redundant with PR4 unit coverage, but useful as PR6 integration coverage proving paper framing plus validator still compose correctly. Keep it if test runtime is cheap.
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/EXTRACTION.md:14:  Tolerances: 1; subject aliases: ['MaxQuant']
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/EXTRACTION.md:34:### `comparator_bound_to_wrong_subject` (3)
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/EXTRACTION.md:37:  _Reason:_ Subject is generic 'several dia-PASEF workflows', not a specific named tool. The 3-5% range is also not tied to a single subject with a comparator.
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/EXTRACTION.md:39:  _Reason:_ 'Up to 10% more' is a vague upper bound on a difference metric; not a cleanly tied claim of (metric, comparator, value, subject).
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/EXTRACTION.md:41:  _Reason:_ Subject is 'both tools' generically; not tied to a single named subject for which the validator can match a source_span.
./experiments/phase5-extraction-rate/extracted/paper-rustims-main/EXTRACTION.md:48:  _Reason:_ Conditional on 'higher complexities' (not precisely defined) and no specific measurement value tied to a clear comparator+subject in a way suitable for extraction.
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/raw_extraction.json:14:        "reason": "comparator_bound_to_wrong_subject",
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/raw_extraction.json:20:        "reason": "comparator_bound_to_wrong_subject",
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/raw_extraction.json:21:        "rationale": "These are filtering settings applied within PEAKS configuration, not performance claims about timsim or any benchmarked subject."
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/raw_extraction.json:26:        "reason": "comparator_bound_to_wrong_subject",
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/raw_extraction.json:32:        "reason": "comparator_bound_to_wrong_subject",
./EVIDENT_AGENT_PHASE3.code-review-2.md:29:    F-CR3-1 — honor last_verified_sidecar in synthesis
./EVIDENT_AGENT_PHASE3.code-review-2.md:31:    synthesize_for accepted last_verified_path but threaded it
./EVIDENT_AGENT_PHASE3.code-review-2.md:32:    through as _last_verified_path and never used it. read_report
./EVIDENT_AGENT_PHASE3.code-review-2.md:34:    using last_verified.json (the Phase 1 sidecar convention) —
./EVIDENT_AGENT_PHASE3.code-review-2.md:36:    saw the original claim.last_verified (typically null) and the
./EVIDENT_AGENT_PHASE3.code-review-2.md:39:    Fix: load the last_verified sidecar, authorize its path via the
./EVIDENT_AGENT_PHASE3.code-review-2.md:40:    allow-list policy, and overlay onto each claim's last_verified
./EVIDENT_AGENT_PHASE3.code-review-2.md:71:    - last_verified_sidecar_is_overlayed_codex_3_cr1: read_report
./EVIDENT_AGENT_PHASE3.code-review-2.md:91:-    translate_tolerances, ManifestClaim, ManifestReviewEvent, ReviewEventSidecar,
./EVIDENT_AGENT_PHASE3.code-review-2.md:93:+    translate_tolerances, ManifestClaim, ManifestLastVerified, ManifestReviewEvent,
./EVIDENT_AGENT_PHASE3.code-review-2.md:116:-    _last_verified_path: Option<&str>,
./EVIDENT_AGENT_PHASE3.code-review-2.md:117:+    last_verified_path: Option<&str>,
./EVIDENT_AGENT_PHASE3.code-review-2.md:123:+    // Codex F-CR3-1: overlay the last_verified sidecar (when given)
./EVIDENT_AGENT_PHASE3.code-review-2.md:124:+    // onto each claim's `last_verified` field BEFORE translation.
./EVIDENT_AGENT_PHASE3.code-review-2.md:126:+    // for any corpus using last_verified.json (the agent's
./EVIDENT_AGENT_PHASE3.code-review-2.md:128:+    if let Some(lv_path) = last_verified_path {
./EVIDENT_AGENT_PHASE3.code-review-2.md:129:+        authorize_sidecar(state, lv_path, "last_verified_sidecar")?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:130:+        let overlay = load_last_verified_sidecar(lv_path)?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:133:+                cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review-2.md:158:-                let Ok(bc_criteria) = translate_tolerances(bc) else { continue };
./EVIDENT_AGENT_PHASE3.code-review-2.md:194:+            let bc_criteria = translate_tolerances(bc)
./EVIDENT_AGENT_PHASE3.code-review-2.md:220:+/// Read a `last_verified.json` sidecar and return the per-claim
./EVIDENT_AGENT_PHASE3.code-review-2.md:224:+fn load_last_verified_sidecar(
./EVIDENT_AGENT_PHASE3.code-review-2.md:228:+        .map_err(|e| ToolError::data(format!("read last_verified sidecar {path}: {e}")))?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:230:+        .map_err(|e| ToolError::data(format!("parse last_verified sidecar {path}: {e}")))
./EVIDENT_AGENT_PHASE3.code-review-2.md:246:+fn last_verified_sidecar_is_overlayed_codex_3_cr1() {
./EVIDENT_AGENT_PHASE3.code-review-2.md:247:+    // Codex F-CR3-1: read_report with last_verified_sidecar must
./EVIDENT_AGENT_PHASE3.code-review-2.md:250:+    // CLI for any corpus using last_verified.json.
./EVIDENT_AGENT_PHASE3.code-review-2.md:264:+    tolerances:
./EVIDENT_AGENT_PHASE3.code-review-2.md:270:+      oracle: [Test]
./EVIDENT_AGENT_PHASE3.code-review-2.md:277:+    let last_verified = tmp.path().join("last_verified.json");
./EVIDENT_AGENT_PHASE3.code-review-2.md:279:+        &last_verified,
./EVIDENT_AGENT_PHASE3.code-review-2.md:294:+    // Without last_verified_sidecar: criterion is NotAssessed
./EVIDENT_AGENT_PHASE3.code-review-2.md:295:+    // because the manifest's inline last_verified is null/empty.
./EVIDENT_AGENT_PHASE3.code-review-2.md:308:+        "without last_verified overlay the criterion must be not_assessed; got {bundle_without}"
./EVIDENT_AGENT_PHASE3.code-review-2.md:311:+    // With last_verified_sidecar: criterion is Pass (0.0017 < 0.005).
./EVIDENT_AGENT_PHASE3.code-review-2.md:317:+            "last_verified_sidecar": last_verified.to_str().unwrap()
./EVIDENT_AGENT_PHASE3.code-review-2.md:325:+        "last_verified overlay should produce Pass; got {bundle_with}"
./EVIDENT_AGENT_PHASE3.code-review-2.md:351:+    tolerances:
./EVIDENT_AGENT_PHASE3.code-review-2.md:357:+      oracle: [Test]
./EVIDENT_AGENT_PHASE3.code-review-2.md:393:+                        "tolerances": [],
./EVIDENT_AGENT_PHASE3.code-review-2.md:394:+                        "evidence": {"oracle": ["Test"], "command": "true", "artifact": "out.json"}
./EVIDENT_AGENT_PHASE3.code-review-2.md:485:    translate_tolerances, ManifestClaim, ManifestLastVerified, ManifestReviewEvent,
./EVIDENT_AGENT_PHASE3.code-review-2.md:680:    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review-2.md:682:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:932:    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review-2.md:933:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:956:    last_verified_path: Option<&str>,
./EVIDENT_AGENT_PHASE3.code-review-2.md:961:    // Codex F-CR3-1: overlay the last_verified sidecar (when given)
./EVIDENT_AGENT_PHASE3.code-review-2.md:962:    // onto each claim's `last_verified` field BEFORE translation.
./EVIDENT_AGENT_PHASE3.code-review-2.md:964:    // for any corpus using last_verified.json (the agent's
./EVIDENT_AGENT_PHASE3.code-review-2.md:966:    if let Some(lv_path) = last_verified_path {
./EVIDENT_AGENT_PHASE3.code-review-2.md:967:        authorize_sidecar(state, lv_path, "last_verified_sidecar")?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:968:        let overlay = load_last_verified_sidecar(lv_path)?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:971:                cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review-2.md:987:    let criteria = translate_tolerances(&target.claim).map_err(|e| ToolError::data(e.to_string()))?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:1042:            let bc_criteria = translate_tolerances(bc)
./EVIDENT_AGENT_PHASE3.code-review-2.md:1089:/// Read a `last_verified.json` sidecar and return the per-claim
./EVIDENT_AGENT_PHASE3.code-review-2.md:1093:fn load_last_verified_sidecar(
./EVIDENT_AGENT_PHASE3.code-review-2.md:1097:        .map_err(|e| ToolError::data(format!("read last_verified sidecar {path}: {e}")))?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:1099:        .map_err(|e| ToolError::data(format!("parse last_verified sidecar {path}: {e}")))
./EVIDENT_AGENT_PHASE3.code-review-2.md:1103:/bin/bash -lc "grep -R \"struct ManifestLastVerified\\|last_verified\" -n typed-trust/src typed-trust/tests | head -80" in /scratch/TMAlign/evident
./EVIDENT_AGENT_PHASE3.code-review-2.md:1105:typed-trust/src/provenance.rs:3://! Cf. shipping schema's `pinned_versions` + `last_verified` +
./EVIDENT_AGENT_PHASE3.code-review-2.md:1106:typed-trust/src/mcp/handlers.rs:219:    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review-2.md:1107:typed-trust/src/mcp/handlers.rs:221:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:1108:typed-trust/src/mcp/handlers.rs:471:    let last_verified_path = arg_str_opt(&args, "last_verified_sidecar");
./EVIDENT_AGENT_PHASE3.code-review-2.md:1109:typed-trust/src/mcp/handlers.rs:472:    let report = synthesize_for(state, &manifest_path, &claim_id, sidecar_path.as_deref(), last_verified_path.as_deref())?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:1110:typed-trust/src/mcp/handlers.rs:495:    last_verified_path: Option<&str>,
./EVIDENT_AGENT_PHASE3.code-review-2.md:1111:typed-trust/src/mcp/handlers.rs:500:    // Codex F-CR3-1: overlay the last_verified sidecar (when given)
./EVIDENT_AGENT_PHASE3.code-review-2.md:1112:typed-trust/src/mcp/handlers.rs:501:    // onto each claim's `last_verified` field BEFORE translation.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1113:typed-trust/src/mcp/handlers.rs:503:    // for any corpus using last_verified.json (the agent's
./EVIDENT_AGENT_PHASE3.code-review-2.md:1114:typed-trust/src/mcp/handlers.rs:505:    if let Some(lv_path) = last_verified_path {
./EVIDENT_AGENT_PHASE3.code-review-2.md:1115:typed-trust/src/mcp/handlers.rs:506:        authorize_sidecar(state, lv_path, "last_verified_sidecar")?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:1116:typed-trust/src/mcp/handlers.rs:507:        let overlay = load_last_verified_sidecar(lv_path)?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:1117:typed-trust/src/mcp/handlers.rs:510:                cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review-2.md:1118:typed-trust/src/mcp/handlers.rs:628:/// Read a `last_verified.json` sidecar and return the per-claim
./EVIDENT_AGENT_PHASE3.code-review-2.md:1119:typed-trust/src/mcp/handlers.rs:632:fn load_last_verified_sidecar(
./EVIDENT_AGENT_PHASE3.code-review-2.md:1120:typed-trust/src/mcp/handlers.rs:636:        .map_err(|e| ToolError::data(format!("read last_verified sidecar {path}: {e}")))?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:1121:typed-trust/src/mcp/handlers.rs:638:        .map_err(|e| ToolError::data(format!("parse last_verified sidecar {path}: {e}")))
./EVIDENT_AGENT_PHASE3.code-review-2.md:1122:typed-trust/src/mcp/tools.rs:43:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_AGENT_PHASE3.code-review-2.md:1123:typed-trust/src/mcp/tools.rs:50:                "last_verified_sidecar": {"type": "string", "description": "Optional last_verified.json sidecar path"}
./EVIDENT_AGENT_PHASE3.code-review-2.md:1124:typed-trust/src/mcp/tools.rs:161:                "last_verified_sidecar": {"type": "string"},
./EVIDENT_AGENT_PHASE3.code-review-2.md:1125:typed-trust/src/translate.rs:23://! - Translating `last_verified` into a [`Rerun`].
./EVIDENT_AGENT_PHASE3.code-review-2.md:1126:typed-trust/src/translate.rs:57:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./EVIDENT_AGENT_PHASE3.code-review-2.md:1127:typed-trust/src/translate.rs:72:    pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_AGENT_PHASE3.code-review-2.md:1129:typed-trust/src/translate.rs:373:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./EVIDENT_AGENT_PHASE3.code-review-2.md:1130:typed-trust/src/translate.rs:375:///   criterion id (shipping convention: `last_verified.value` is the
./EVIDENT_AGENT_PHASE3.code-review-2.md:1131:typed-trust/src/translate.rs:396:    let reruns = translate_last_verified(
./EVIDENT_AGENT_PHASE3.code-review-2.md:1132:typed-trust/src/translate.rs:397:        mc.last_verified.as_ref(),
./EVIDENT_AGENT_PHASE3.code-review-2.md:1133:typed-trust/src/translate.rs:434:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./EVIDENT_AGENT_PHASE3.code-review-2.md:1134:typed-trust/src/translate.rs:436:/// - `last_verified` is absent;
./EVIDENT_AGENT_PHASE3.code-review-2.md:1135:typed-trust/src/translate.rs:437:/// - `last_verified.date` is null (replay loop hasn't run);
./EVIDENT_AGENT_PHASE3.code-review-2.md:1136:typed-trust/src/translate.rs:438:/// - `last_verified.value` is null (no primary observation).
./EVIDENT_AGENT_PHASE3.code-review-2.md:1137:typed-trust/src/translate.rs:443:fn translate_last_verified(
./EVIDENT_AGENT_PHASE3.code-review-2.md:1138:typed-trust/src/translate.rs:469:        // Shipping convention: a populated last_verified records a
./EVIDENT_AGENT_PHASE3.code-review-2.md:1139:typed-trust/src/synthesize.rs:30:///   `last_verified`).
./EVIDENT_AGENT_PHASE3.code-review-2.md:1140:typed-trust/src/main.rs:80:    // Overlay sidecar entries onto each claim's last_verified field
./EVIDENT_AGENT_PHASE3.code-review-2.md:1141:typed-trust/src/main.rs:88:                        cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review-2.md:1142:typed-trust/src/main.rs:363:    eprintln!("    overlay sidecar JSON entries onto each claim's last_verified field");
./EVIDENT_AGENT_PHASE3.code-review-2.md:1143:typed-trust/src/main.rs:578:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./EVIDENT_AGENT_PHASE3.code-review-2.md:1144:typed-trust/tests/mcp_tools.rs:681:fn last_verified_sidecar_is_overlayed_codex_3_cr1() {
./EVIDENT_AGENT_PHASE3.code-review-2.md:1145:typed-trust/tests/mcp_tools.rs:682:    // Codex F-CR3-1: read_report with last_verified_sidecar must
./EVIDENT_AGENT_PHASE3.code-review-2.md:1146:typed-trust/tests/mcp_tools.rs:685:    // CLI for any corpus using last_verified.json.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1147:typed-trust/tests/mcp_tools.rs:712:    let last_verified = tmp.path().join("last_verified.json");
./EVIDENT_AGENT_PHASE3.code-review-2.md:1148:typed-trust/tests/mcp_tools.rs:714:        &last_verified,
./EVIDENT_AGENT_PHASE3.code-review-2.md:1149:typed-trust/tests/mcp_tools.rs:729:    // Without last_verified_sidecar: criterion is NotAssessed
./EVIDENT_AGENT_PHASE3.code-review-2.md:1150:typed-trust/tests/mcp_tools.rs:730:    // because the manifest's inline last_verified is null/empty.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1151:typed-trust/tests/mcp_tools.rs:743:        "without last_verified overlay the criterion must be not_assessed; got {bundle_without}"
./EVIDENT_AGENT_PHASE3.code-review-2.md:1152:typed-trust/tests/mcp_tools.rs:746:    // With last_verified_sidecar: criterion is Pass (0.0017 < 0.005).
./EVIDENT_AGENT_PHASE3.code-review-2.md:1153:typed-trust/tests/mcp_tools.rs:752:            "last_verified_sidecar": last_verified.to_str().unwrap()
./EVIDENT_AGENT_PHASE3.code-review-2.md:1154:typed-trust/tests/mcp_tools.rs:760:        "last_verified overlay should produce Pass; got {bundle_with}"
./EVIDENT_AGENT_PHASE3.code-review-2.md:1155:typed-trust/tests/translator.rs:16:/// `last_verified` block has all null values (the CI replay loop is
./EVIDENT_AGENT_PHASE3.code-review-2.md:1156:typed-trust/tests/translator.rs:47:    last_verified:
./EVIDENT_AGENT_PHASE3.code-review-2.md:1157:typed-trust/tests/translator.rs:59:/// claim with a populated last_verified block. Verbatim values for the
./EVIDENT_AGENT_PHASE3.code-review-2.md:1158:typed-trust/tests/translator.rs:90:    last_verified:
./EVIDENT_AGENT_PHASE3.code-review-2.md:1159:typed-trust/tests/translator.rs:227:    // last_verified Reruns bind to this stable id.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1160:typed-trust/tests/translator.rs:572:// --- Evidence + last_verified translation ---
./EVIDENT_AGENT_PHASE3.code-review-2.md:1161:typed-trust/tests/translator.rs:575:fn ci_claim_with_null_last_verified_has_empty_reruns() {
./EVIDENT_AGENT_PHASE3.code-review-2.md:1162:typed-trust/tests/translator.rs:585:    // CI tier without populated last_verified → empty reruns.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1163:typed-trust/tests/translator.rs:614:fn release_claim_with_populated_last_verified_emits_rerun() {
./EVIDENT_AGENT_PHASE3.code-review-2.md:1164:typed-trust/tests/translator.rs:634:    // last_verified is fully populated → one Rerun.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1165:typed-trust/tests/translator.rs:648:    // last_verified.value is the primary scalar metric).
./EVIDENT_AGENT_PHASE3.code-review-2.md:1166:typed-trust/tests/review_events_sidecar.rs:106:        last_verified: None,
./EVIDENT_AGENT_PHASE3.code-review-2.md:1167:typed-trust/tests/html_render.rs:38:    last_verified:
./EVIDENT_AGENT_PHASE3.code-review-2.md:1168:typed-trust/tests/synthesize_and_render.rs:20:/// translation working) with last_verified populated.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1169:typed-trust/tests/synthesize_and_render.rs:47:    last_verified:
./EVIDENT_AGENT_PHASE3.code-review-2.md:1170:typed-trust/tests/synthesize_and_render.rs:56:/// Same claim but with last_verified value set to 0.01 (over the
./EVIDENT_AGENT_PHASE3.code-review-2.md:1171:typed-trust/tests/synthesize_and_render.rs:82:    last_verified:
./EVIDENT_AGENT_PHASE3.code-review-2.md:1172:typed-trust/tests/synthesize_and_render.rs:150:    // Strip last_verified so the rerun has no observations.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1173:typed-trust/tests/synthesize_and_render.rs:152:        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./EVIDENT_AGENT_PHASE3.code-review-2.md:1174:typed-trust/tests/synthesize_and_render.rs:153:        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./EVIDENT_AGENT_PHASE3.code-review-2.md:1175:typed-trust/tests/human_render.rs:40:    last_verified:
./EVIDENT_AGENT_PHASE3.code-review-2.md:1189:    pub tolerances: Option<Vec<ManifestTolerance>>,
./EVIDENT_AGENT_PHASE3.code-review-2.md:1192:    pub last_verified: Option<ManifestLastVerified>,
./EVIDENT_AGENT_PHASE3.code-review-2.md:1226:    // Overlay sidecar entries onto each claim's last_verified field
./EVIDENT_AGENT_PHASE3.code-review-2.md:1234:                        cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review-2.md:1265:/// The shape matches `workflow/evident.py`'s `last_verified.json`
./EVIDENT_AGENT_PHASE3.code-review-2.md:1349:    // Overlay sidecar entries onto each claim's last_verified field
./EVIDENT_AGENT_PHASE3.code-review-2.md:1357:                        cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review-2.md:1426:        let criteria = match translate_tolerances(mc) {
./EVIDENT_AGENT_PHASE3.code-review-2.md:1429:                // All translate_tolerances errors at this point are
./EVIDENT_AGENT_PHASE3.code-review-2.md:1570:    pub oracle: Vec<String>,
./EVIDENT_AGENT_PHASE3.code-review-2.md:1591:    /// An unknown comparison operator in `tolerances[].op`.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1600:    /// release claims must carry structured tolerances.
./EVIDENT_AGENT_PHASE3.code-review-2.md:1602:    /// A `kind: measurement` claim omitted `tolerances` or provided
./EVIDENT_AGENT_PHASE3.code-review-2.md:1604:    /// tolerances on measurement claims; without them the
./EVIDENT_AGENT_PHASE3.code-review-2.md:1885:        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
./EVIDENT_AGENT_PHASE3.code-review-2.md:1892:                "last_verified_sidecar": {"type": "string", "description": "Optional last_verified.json sidecar path"}
./EVIDENT_AGENT_PHASE3.code-review-2.md:2003:                "last_verified_sidecar": {"type": "string"},
./EVIDENT_AGENT_PHASE3.code-review-2.md:2045:   500	    // Codex F-CR3-1: overlay the last_verified sidecar (when given)
./EVIDENT_AGENT_PHASE3.code-review-2.md:2046:   501	    // onto each claim's `last_verified` field BEFORE translation.
./EVIDENT_AGENT_PHASE3.code-review-2.md:2048:   503	    // for any corpus using last_verified.json (the agent's
./EVIDENT_AGENT_PHASE3.code-review-2.md:2050:   505	    if let Some(lv_path) = last_verified_path {
./EVIDENT_AGENT_PHASE3.code-review-2.md:2051:   506	        authorize_sidecar(state, lv_path, "last_verified_sidecar")?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:2052:   507	        let overlay = load_last_verified_sidecar(lv_path)?;
./EVIDENT_AGENT_PHASE3.code-review-2.md:2055:   510	                cw.claim.last_verified = Some(lv.clone());
./EVIDENT_AGENT_PHASE3.code-review-2.md:2086:   581	            let bc_criteria = translate_tolerances(bc)
./EVIDENT_AGENT_PHASE3.code-review-2.md:2110:697-        let bc_criteria = translate_tolerances(bc)
./EVIDENT_AGENT_PHASE3.code-review-2.md:2181://! observed values against tolerances, applies the §8 rule for
./EVIDENT_AGENT_PHASE3.code-review-2.md:2204:///   `last_verified`).
./EVIDENT_AGENT_PHASE3.code-review-2.md:2357:The changes address the documented MCP parity issues without introducing an evident correctness regression. The new tests cover the fixed last_verified overlay, backing-claim error surfacing, and out-of-range query cursor behavior.
./EVIDENT_AGENT_PHASE3.code-review-2.md:2358:The changes address the documented MCP parity issues without introducing an evident correctness regression. The new tests cover the fixed last_verified overlay, backing-claim error surfacing, and out-of-range query cursor behavior.
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/EXTRACTION.md:25:### `comparator_bound_to_wrong_subject` (4)
./experiments/phase5-extraction-rate/extracted/paper-rustims-supplement/EXTRACTION.md:30:  _Reason:_ These are filtering settings applied within PEAKS configuration, not performance claims about timsim or any benchmarked subject.
./experiments/phase5-extraction-rate/extracted/repo-rustims-rustms/evident.yaml:6:  kind: metadata_compatibility
./experiments/phase5-extraction-rate/extracted/repo-rustims-rustms/evident.yaml:27:  kind: metadata_compatibility
./experiments/phase5-extraction-rate/extracted/repo-rustims-rustms/evident.yaml:48:  kind: metadata_compatibility
./typed-trust-codex-review-4.md:238:+enum ClaimKind {
./typed-trust-codex-review-4.md:364:+    against: Option<String>,            // names the oracle from manifest
./typed-trust-codex-review-4.md:367:+                                        // multiple oracles with different
./typed-trust-codex-review-4.md:368:+                                        // tolerances
./typed-trust-codex-review-4.md:496:+- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
./typed-trust-codex-review-4.md:520:+- `Tolerance.against` for multi-oracle binding (SASA vs Biopython AND
./typed-trust-codex-review-4.md:521:+  FreeSASA at different tolerances on the same output).
./typed-trust-codex-review-4.md:614:+pub enum ClaimKind {
./typed-trust-codex-review-4.md:1224:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-4.md:1291:+        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-4.md:1530:+//! Cf. shipping schema's `pinned_versions` + `last_verified` +
./typed-trust-codex-review-4.md:2001:+//! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-4.md:2024:+///   `last_verified`).
./typed-trust-codex-review-4.md:2591:+//! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-4.md:2592:+//!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-4.md:2593:+//!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-4.md:2601:+//! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-4.md:2635:+/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-4.md:2647:+    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-4.md:2650:+    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-4.md:2682:+    pub oracle: Vec<String>,
./typed-trust-codex-review-4.md:2703:+    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-4.md:2812:+/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-4.md:2816:+/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-4.md:2817:+/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-4.md:2819:+pub fn translate_tolerances(
./typed-trust-codex-review-4.md:2822:+    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-4.md:2823:+        if e.oracle.len() == 1 {
./typed-trust-codex-review-4.md:2824:+            Some(e.oracle[0].clone())
./typed-trust-codex-review-4.md:2830:+    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-4.md:2838:+            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review-4.md:2850:+    single_oracle: &Option<String>,
./typed-trust-codex-review-4.md:2862:+            against: single_oracle.clone(),
./typed-trust-codex-review-4.md:2875:+/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-4.md:2876:+///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-4.md:2878:+///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-4.md:2893:+/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-4.md:2895:+///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-4.md:2905:+    let reruns = translate_last_verified(
./typed-trust-codex-review-4.md:2906:+        mc.last_verified.as_ref(),
./typed-trust-codex-review-4.md:2943:+/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-4.md:2945:+/// - `last_verified` is absent;
./typed-trust-codex-review-4.md:2946:+/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-4.md:2947:+/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-4.md:2952:+fn translate_last_verified(
./typed-trust-codex-review-4.md:2978:+        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-4.md:3045:+/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-4.md:3046:+///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-4.md:3053:+        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-4.md:3320:+        "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances"
./typed-trust-codex-review-4.md:3482:+    parse_manifest_file, translate_evidence, translate_tolerances, TranslationContext,
./typed-trust-codex-review-4.md:3500:+    tolerances:
./typed-trust-codex-review-4.md:3508:+      oracle:
./typed-trust-codex-review-4.md:3513:+    last_verified:
./typed-trust-codex-review-4.md:3534:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-4.md:3799:+                "split into vs-OpenMM (tight) + vs-BALL (wide) tolerances".into(),
./typed-trust-codex-review-4.md:3821:+    // populated to the oracle name.
./typed-trust-codex-review-4.md:3867:+    // Tolerance.against populated with BALL for single-oracle case.
./typed-trust-codex-review-4.md:3975:+               within tier-specific tolerances."
./typed-trust-codex-review-4.md:3987:+    // §7 — Three Criteria from the YAML's three tolerances
./typed-trust-codex-review-4.md:4034:+                against: Some("FreeSASA".into()), // same metric, different oracle
./typed-trust-codex-review-4.md:4051:+                against: None, // pass rate isn't oracle-specific
./typed-trust-codex-review-4.md:4062:+    // §2/§6 — Evidence (one per oracle, sharing the artifact)
./typed-trust-codex-review-4.md:4146:+    // The F-PR3 win: each tolerance binds to its oracle.
./typed-trust-codex-review-4.md:4152:+    assert_ne!(biopy_tol.against, freesasa_tol.against); // …different oracle
./typed-trust-codex-review-4.md:4262:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-4.md:4268:+/// real proteon SASA release shape (one oracle to keep `against`
./typed-trust-codex-review-4.md:4269:+/// translation working) with last_verified populated.
./typed-trust-codex-review-4.md:4283:+    tolerances:
./typed-trust-codex-review-4.md:4291:+      oracle:
./typed-trust-codex-review-4.md:4296:+    last_verified:
./typed-trust-codex-review-4.md:4305:+/// Same claim but with last_verified value set to 0.01 (over the
./typed-trust-codex-review-4.md:4319:+    tolerances:
./typed-trust-codex-review-4.md:4326:+      oracle:
./typed-trust-codex-review-4.md:4331:+    last_verified:
./typed-trust-codex-review-4.md:4351:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-4.md:4397:+    // Strip last_verified so the rerun has no observations.
./typed-trust-codex-review-4.md:4399:+        "    last_verified:\n      commit: \"4d6ddbec\"\n      date: \"2026-05-11\"\n      value: 0.0017\n      corpus_sha: \"b319c47c\"",
./typed-trust-codex-review-4.md:4400:+        "    last_verified:\n      commit: null\n      date: null\n      value: null\n      corpus_sha: null",
./typed-trust-codex-review-4.md:5226:+    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances,
./typed-trust-codex-review-4.md:5231:+/// proteon-sasa-vs-biopython-ci — single-output single-oracle CI claim.
./typed-trust-codex-review-4.md:5232:+/// `last_verified` block has all null values (the CI replay loop is
./typed-trust-codex-review-4.md:5250:+    tolerances:
./typed-trust-codex-review-4.md:5258:+      oracle:
./typed-trust-codex-review-4.md:5263:+    last_verified:
./typed-trust-codex-review-4.md:5271:+      - Single-oracle agreement can mask a shared convention choice.
./typed-trust-codex-review-4.md:5275:+/// claim with a populated last_verified block. Verbatim values for the
./typed-trust-codex-review-4.md:5276:+/// fields that matter; oracle list trimmed to one for the
./typed-trust-codex-review-4.md:5277:+/// single-oracle path (the real claim has two — Biopython AND
./typed-trust-codex-review-4.md:5293:+    tolerances:
./typed-trust-codex-review-4.md:5301:+      oracle:
./typed-trust-codex-review-4.md:5306:+    last_verified:
./typed-trust-codex-review-4.md:5320:+  - id: proteon-oracle-backed-release-gate
./typed-trust-codex-review-4.md:5321:+    title: Proteon core numerical claims are release-gated by external oracles
./typed-trust-codex-review-4.md:5331:+      accepted for release only when they point to an independent oracle.
./typed-trust-codex-review-4.md:5333:+      oracle:
./typed-trust-codex-review-4.md:5335:+      command: pytest tests/oracle -v
./typed-trust-codex-review-4.md:5359:+    tolerances:
./typed-trust-codex-review-4.md:5379:+      oracle:
./typed-trust-codex-review-4.md:5381:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-4.md:5406:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // oracle present
./typed-trust-codex-review-4.md:5427:+fn translates_single_oracle_tolerance_populates_against() {
./typed-trust-codex-review-4.md:5429:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-4.md:5437:+    // F-PR3 single-oracle case: `against` is populated from the single
./typed-trust-codex-review-4.md:5438:+    // entry in `evidence.oracle`.
./typed-trust-codex-review-4.md:5443:+    // last_verified Reruns bind to this stable id.
./typed-trust-codex-review-4.md:5458:+            assert_eq!(id, "proteon-oracle-backed-release-gate");
./typed-trust-codex-review-4.md:5466:+fn translates_dssp_tolerances_including_eq_operator() {
./typed-trust-codex-review-4.md:5470:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-4.md:5492:+    // Single-oracle case (pydssp) → all three get against=Some("pydssp").
./typed-trust-codex-review-4.md:5522:+    tolerances:
./typed-trust-codex-review-4.md:5528:+      oracle: [SomeOracle]
./typed-trust-codex-review-4.md:5533:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-4.md:5559:+    tolerances:
./typed-trust-codex-review-4.md:5565:+      oracle: [internal]
./typed-trust-codex-review-4.md:5566:+      command: pytest tests/oracle/test_dssp_oracle.py -v
./typed-trust-codex-review-4.md:5570:+    let criteria = translate_tolerances(&manifest.claims[0]).unwrap();
./typed-trust-codex-review-4.md:5608:+    tolerances:
./typed-trust-codex-review-4.md:5613:+      oracle: [Foo]
./typed-trust-codex-review-4.md:5618:+    let result = translate_tolerances(&manifest.claims[0]);
./typed-trust-codex-review-4.md:5634:+    assert_eq!(attested.value.kind, ClaimKind::Comparison); // pydssp oracle
./typed-trust-codex-review-4.md:5637:+// --- Evidence + last_verified translation ---
./typed-trust-codex-review-4.md:5640:+fn ci_claim_with_null_last_verified_has_empty_reruns() {
./typed-trust-codex-review-4.md:5643:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-4.md:5650:+    // CI tier without populated last_verified → empty reruns.
./typed-trust-codex-review-4.md:5679:+fn release_claim_with_populated_last_verified_emits_rerun() {
./typed-trust-codex-review-4.md:5682:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-4.md:5699:+    // last_verified is fully populated → one Rerun.
./typed-trust-codex-review-4.md:5713:+    // last_verified.value is the primary scalar metric).
./typed-trust-codex-review-4.md:5727:+    let criteria = translate_tolerances(mc).unwrap();
./typed-trust-codex-review-4.md:5743: | `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-4.md:5749: typically held in a sidecar (`last_verified.json`) keyed by claim id so
./typed-trust-codex-review-4.md:5760:+  by: maintainer-2026-05         # signer identity (key id, sigstore subject, ...)
./typed-trust-codex-review-4.md:5782:+- **`evidence.artifact` + sha256 in `last_verified.value`** still pin
./typed-trust-codex-review-4.md:5876://! observed values against tolerances, applies the §8 rule for
./typed-trust-codex-review-4.md:5899:///   `last_verified`).
./typed-trust-codex-review-4.md:6309:    parse_manifest_file, translate_claim, translate_evidence, translate_tolerances, ManifestClaim,
./typed-trust-codex-review-4.md:6376:        let criteria = match translate_tolerances(mc) {
./typed-trust-codex-review-4.md:6925://! - Translates the per-claim `tolerances` block into [`Tolerance`]
./typed-trust-codex-review-4.md:6926://!   values, populating `against` from a single-oracle heuristic
./typed-trust-codex-review-4.md:6927://!   (multi-oracle disambiguation needs schema work or convention).
./typed-trust-codex-review-4.md:6935://! - Translating `last_verified` into a [`Rerun`].
./typed-trust-codex-review-4.md:6969:/// capabilities, inputs, outputs, pinned_versions, last_verified)
./typed-trust-codex-review-4.md:6981:    pub tolerances: Option<Vec<ManifestTolerance>>,
./typed-trust-codex-review-4.md:6984:    pub last_verified: Option<ManifestLastVerified>,
./typed-trust-codex-review-4.md:7016:    pub oracle: Vec<String>,
./typed-trust-codex-review-4.md:7037:    /// An unknown comparison operator in `tolerances[].op`.
./typed-trust-codex-review-4.md:7146:/// Translate all `tolerances` entries into [`TranslatedCriterion`]
./typed-trust-codex-review-4.md:7150:/// When the claim's `evidence.oracle` is a single entry, populate
./typed-trust-codex-review-4.md:7151:/// `Tolerance.against` from it (the F-PR3 single-oracle case);
./typed-trust-codex-review-4.md:7153:pub fn translate_tolerances(
./typed-trust-codex-review-4.md:7156:    let single_oracle: Option<String> = mc.evidence.as_ref().and_then(|e| {
./typed-trust-codex-review-4.md:7157:        if e.oracle.len() == 1 {
./typed-trust-codex-review-4.md:7158:            Some(e.oracle[0].clone())
./typed-trust-codex-review-4.md:7164:    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-4.md:7172:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review-4.md:7177:            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review-4.md:7189:    single_oracle: &Option<String>,
./typed-trust-codex-review-4.md:7201:            against: single_oracle.clone(),
./typed-trust-codex-review-4.md:7214:/// - One Evidence per claim (the YAML's `evidence.oracle` list is
./typed-trust-codex-review-4.md:7215:///   collapsed into a single Evidence; oracle identity per tolerance
./typed-trust-codex-review-4.md:7217:///   Evidence per oracle; the translator's 1:1 mapping is simpler and
./typed-trust-codex-review-4.md:7232:/// - `last_verified` populates one [`Rerun`] in the Verified extraction
./typed-trust-codex-review-4.md:7234:///   criterion id (shipping convention: `last_verified.value` is the
./typed-trust-codex-review-4.md:7244:    let reruns = translate_last_verified(
./typed-trust-codex-review-4.md:7245:        mc.last_verified.as_ref(),
./typed-trust-codex-review-4.md:7282:/// Translate the `last_verified` block into a `Vec<Rerun>`. Returns an
./typed-trust-codex-review-4.md:7284:/// - `last_verified` is absent;
./typed-trust-codex-review-4.md:7285:/// - `last_verified.date` is null (replay loop hasn't run);
./typed-trust-codex-review-4.md:7286:/// - `last_verified.value` is null (no primary observation).
./typed-trust-codex-review-4.md:7291:fn translate_last_verified(
./typed-trust-codex-review-4.md:7317:        // Shipping convention: a populated last_verified records a
./typed-trust-codex-review-4.md:7384:/// - Claim has at least one oracle in `evidence.oracle` → `Comparison`
./typed-trust-codex-review-4.md:7385:///   (most measurement claims compare against an oracle).
./typed-trust-codex-review-4.md:7392:        .is_some_and(|e| !e.oracle.is_empty())
./typed-trust-codex-review-4.md:8219:  oracle:
./typed-trust-codex-review-4.md:8241:      oracle:
./typed-trust-codex-review-4.md:8278:   `subsystem`, `oracle`, and `capability` are declared per-manifest;
./typed-trust-codex-review-4.md:8297:  oracle:       [...]
./typed-trust-codex-review-4.md:8319:| `oracle`           | empty — consumer must define                                       |
./typed-trust-codex-review-4.md:8342:| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
./typed-trust-codex-review-4.md:8344:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-4.md:8345:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-4.md:8348:| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
./typed-trust-codex-review-4.md:8354:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-4.md:8365:- **Oracles**: `pinned_versions` must include every oracle named in
./typed-trust-codex-review-4.md:8366:  `evidence.oracle` with the version used to produce the cited result
./typed-trust-codex-review-4.md:8375:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-4.md:8384:tolerances:
./typed-trust-codex-review-4.md:8763:still need their own oracle or benchmark commands.
./typed-trust-codex-review-4.md:8799:    "oracle": set(),
./typed-trust-codex-review-4.md:8824:    "tolerances",
./typed-trust-codex-review-4.md:8882:    for field in ("oracle", "command", "artifact"):
./typed-trust-codex-review-4.md:8888:            f"move tolerance text into the top-level tolerances: list "
./typed-trust-codex-review-4.md:8891:    oracles = require_string_list(value["oracle"], "evidence.oracle", claim_id)
./typed-trust-codex-review-4.md:8892:    for oracle_name in oracles:
./typed-trust-codex-review-4.md:8894:            oracle_name, "oracle", vocabularies, "evidence.oracle[]", claim_id
./typed-trust-codex-review-4.md:8898:    return oracles
./typed-trust-codex-review-4.md:8901:def validate_tolerances(
./typed-trust-codex-review-4.md:8905:        fail(f"claim {claim_id}: tolerances must be a non-empty list")
./typed-trust-codex-review-4.md:8908:            fail(f"claim {claim_id}: tolerances[{i}] must be a mapping")
./typed-trust-codex-review-4.md:8911:            fail(f"claim {claim_id}: tolerances[{i}].prose is required")
./typed-trust-codex-review-4.md:8919:                f"claim {claim_id}: tolerances[{i}] metric/op/value are "
./typed-trust-codex-review-4.md:8927:                f"tolerances[{i}].metric",
./typed-trust-codex-review-4.md:8934:                f"tolerances[{i}].op",
./typed-trust-codex-review-4.md:8940:                fail(f"claim {claim_id}: tolerances[{i}].value must be numeric")
./typed-trust-codex-review-4.md:8963:    value: Any, oracle_names: list[str], project: str, claim_id: str
./typed-trust-codex-review-4.md:8980:    missing_oracles = [o for o in oracle_names if o not in value]
./typed-trust-codex-review-4.md:8981:    if missing_oracles:
./typed-trust-codex-review-4.md:8983:            f"claim {claim_id}: pinned_versions must include every oracle "
./typed-trust-codex-review-4.md:8984:            f"named in evidence.oracle; missing: {missing_oracles}"
./typed-trust-codex-review-4.md:9045:def validate_last_verified(value: Any, claim_id: str) -> None:
./typed-trust-codex-review-4.md:9047:        fail(f"claim {claim_id}: last_verified must be a mapping")
./typed-trust-codex-review-4.md:9055:                fail(f"claim {claim_id}: last_verified.value must be numeric or null")
./typed-trust-codex-review-4.md:9057:            fail(f"claim {claim_id}: last_verified.{key} must be a string or null")
./typed-trust-codex-review-4.md:9181:        oracles = validate_evidence(claim["evidence"], claim_id, vocabularies)
./typed-trust-codex-review-4.md:9196:        if "last_verified" in claim:
./typed-trust-codex-review-4.md:9197:            validate_last_verified(claim["last_verified"], claim_id)
./typed-trust-codex-review-4.md:9214:            validate_tolerances(claim["tolerances"], claim_id, vocabularies)
./typed-trust-codex-review-4.md:9216:                claim["pinned_versions"], oracles, project, claim_id
./typed-trust-codex-review-4.md:9245:89:| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
./typed-trust-codex-review-4.md:9246:90:| `evidence`        | yes      | object | `{oracle, command, artifact}` (no `tolerance` field — it moved up) |
./typed-trust-codex-review-4.md:9247:99:omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.
./typed-trust-codex-review-4.md:9248:120:  `tolerances[].output` should reference an entry by name.
./typed-trust-codex-review-4.md:9249:129:tolerances:
./typed-trust-codex-review-4.md:9255:168:Each named entry can be referenced by `tolerances[].output` so a single
./typed-trust-codex-review-4.md:9256:199:underlying numbers are correct (that is what `tolerances` and
./typed-trust-codex-review-4.md:9257:219:replace the underlying `tolerances` and `evidence` fields.
./typed-trust-codex-review-4.md:9260:307:1. Lift `evidence.tolerance` into one or more `tolerances:` entries.
./typed-trust-codex-review-4.md:9261:312:   measurements (and they may then drop `subsystem` and `tolerances`).
./typed-trust-codex-review-4.md:9318:   245	        if e.oracle.len() == 1 {
./typed-trust-codex-review-4.md:9319:   246	            Some(e.oracle[0].clone())
./typed-trust-codex-review-4.md:9325:   252	    let Some(ts) = mc.tolerances.as_ref() else {
./typed-trust-codex-review-4.md:9333:   260	            let tolerance = translate_tolerance(t, &single_oracle, &mc.id)?;
./typed-trust-codex-review-4.md:9345:   272	    single_oracle: &Option<String>,
./typed-trust-codex-review-4.md:9383:   284	            against: single_oracle.clone(),
./typed-trust-codex-review-4.md:9394:The new typed-trust implementation has correctness gaps in status synthesis for supported review targets and accepts under-specified non-research tolerances. These can produce misleading current reports for contested or insufficiently specified claims.
./typed-trust-codex-review-4.md:9401:- [P2] Reject prose-only tolerances outside research tier — /scratch/TMAlign/evident/typed-trust/src/translate.rs:277-278
./typed-trust-codex-review-4.md:9402:  This accepts `metric`/`op`/`value` all absent for every claim tier, even though the schema frames prose-only tolerances as the research-tier deferred-spec escape hatch. A CI or release claim with only prose will translate successfully, synthesize to `NotAssessed`, and still produce a `current` report instead of failing translation, which can let under-specified release claims pass through this CLI.
./typed-trust-codex-review-4.md:9403:The new typed-trust implementation has correctness gaps in status synthesis for supported review targets and accepts under-specified non-research tolerances. These can produce misleading current reports for contested or insufficiently specified claims.
./typed-trust-codex-review-4.md:9410:- [P2] Reject prose-only tolerances outside research tier — /scratch/TMAlign/evident/typed-trust/src/translate.rs:277-278
./typed-trust-codex-review-4.md:9411:  This accepts `metric`/`op`/`value` all absent for every claim tier, even though the schema frames prose-only tolerances as the research-tier deferred-spec escape hatch. A CI or release claim with only prose will translate successfully, synthesize to `NotAssessed`, and still produce a `current` report instead of failing translation, which can let under-specified release claims pass through this CLI.
./experiments/phase5-extraction-rate/extracted/repo-rustims-imspy-core/evident.yaml:6:  kind: metadata_compatibility
./experiments/phase5-extraction-rate/extracted/repo-rustims-imspy-core/evident.yaml:27:  kind: metadata_compatibility
./experiments/phase5-extraction-rate/extracted/repo-rustims-imspy-core/evident.yaml:48:  kind: metadata_compatibility
./EVIDENT_AGENT_PHASE2B.codex-review.md:27:3. VALIDATION HOLES — the agent's local check 'last_verified.value satisfies its own tolerance' — what edge cases bypass it? Multi-criterion backing? Tolerances with op '=' on floats? Backing claims that don't add information (the model essentially rephrases the target claim's inverse)?
./EVIDENT_AGENT_PHASE2B.codex-review.md:81:`last_verified.value = V` that satisfies the inverted comparator. The
./EVIDENT_AGENT_PHASE2B.codex-review.md:141:          "tolerances": [
./EVIDENT_AGENT_PHASE2B.codex-review.md:150:            "oracle": ["BALL"],
./EVIDENT_AGENT_PHASE2B.codex-review.md:154:          "last_verified": { "value": 0.025 }
./EVIDENT_AGENT_PHASE2B.codex-review.md:197:    `translate_claim` / `translate_tolerances` / `translate_evidence`
./EVIDENT_AGENT_PHASE2B.codex-review.md:237:      `tier`, `tolerances` (≥ 1 structured), `evidence`, and a
./EVIDENT_AGENT_PHASE2B.codex-review.md:238:      `last_verified.value` numeric.
./EVIDENT_AGENT_PHASE2B.codex-review.md:239:    - The backing claim's `last_verified.value` must satisfy the
./EVIDENT_AGENT_PHASE2B.codex-review.md:263:  backing claim; rejects backing claim where last_verified.value
./EVIDENT_AGENT_PHASE2B.codex-review.md:281:      last_verified.value satisfies the tolerance comparator.
./EVIDENT_AGENT_PHASE2B.codex-review.md:310:   from the same artifact (last_verified.value satisfies the
./EVIDENT_AGENT_PHASE2B.codex-review.md:324:> 3. is locally verifiable — its last_verified.value satisfies its
./EVIDENT_AGENT_PHASE2B.codex-review.md:332:The validator enforces (3) directly: if `last_verified.value`
./EVIDENT_AGENT_PHASE2B.codex-review.md:349:   `last_verified.value` doesn't satisfy the structured tolerance.
./EVIDENT_AGENT_PHASE2B.codex-review.md:355:   own tolerance fails (or whose last_verified is null) — target
./EVIDENT_AGENT_PHASE2B.codex-review.md:456:"last_verified": { "value": 0.025 }
./EVIDENT_AGENT_PHASE2B.codex-review.md:461:That is acceptable only if typed-trust treats backing claims as “claims about counter-evidence,” not as normal measurement claims with expected-good tolerances. The plan should say this explicitly. Otherwise you are teaching the model to create a claim whose pass condition is “the defect exists,” which is a valid logical witness but a surprising use of a measurement tolerance.
./EVIDENT_AGENT_PHASE2B.codex-review.md:463:This also opens the trivial-pass problem the user called out: `observed > 0`, `observed != null`, or `row exists` can pass while not contradicting the target claim. The local check “last_verified satisfies its own tolerance” does not protect against that at all.
./EVIDENT_AGENT_PHASE2B.codex-review.md:485:> Challenge only when the digest contains a concrete observed value that violates one of the target claim’s stated tolerances. The backing claim must encode that exact violation using the same metric, bound, and observed value. Do not create new weaker criteria, proxy metrics, or broad predicates merely to make the backing claim pass.
./EVIDENT_AGENT_PHASE2B.codex-review.md:506:- **Multi-tolerance ambiguity:** if the target has several tolerances, the model must name which target criterion it contradicts.
./EVIDENT_AGENT_PHASE2B.codex-review.md:629:"last_verified": { "value": 0.025 }
./EVIDENT_AGENT_PHASE2B.codex-review.md:634:That is acceptable only if typed-trust treats backing claims as “claims about counter-evidence,” not as normal measurement claims with expected-good tolerances. The plan should say this explicitly. Otherwise you are teaching the model to create a claim whose pass condition is “the defect exists,” which is a valid logical witness but a surprising use of a measurement tolerance.
./EVIDENT_AGENT_PHASE2B.codex-review.md:636:This also opens the trivial-pass problem the user called out: `observed > 0`, `observed != null`, or `row exists` can pass while not contradicting the target claim. The local check “last_verified satisfies its own tolerance” does not protect against that at all.
./EVIDENT_AGENT_PHASE2B.codex-review.md:658:> Challenge only when the digest contains a concrete observed value that violates one of the target claim’s stated tolerances. The backing claim must encode that exact violation using the same metric, bound, and observed value. Do not create new weaker criteria, proxy metrics, or broad predicates merely to make the backing claim pass.
./EVIDENT_AGENT_PHASE2B.codex-review.md:679:- **Multi-tolerance ambiguity:** if the target has several tolerances, the model must name which target criterion it contradicts.
./EVIDENT_PHASE5_PR5.code-review.md:101:+    sections), and asks the model to extract structured tolerances.
./EVIDENT_PHASE5_PR5.code-review.md:364:+validator-approved tolerances reach ``evident.yaml``.
./EVIDENT_PHASE5_PR5.code-review.md:440:+    tolerances, drop claims with zero remaining tolerances, and
./EVIDENT_PHASE5_PR5.code-review.md:451:+        accepted_tolerances: list[dict] = []
./EVIDENT_PHASE5_PR5.code-review.md:452:+        for raw_tol in raw_claim.get("tolerances", []):
./EVIDENT_PHASE5_PR5.code-review.md:456:+                    subject_aliases=raw_claim.get(
./EVIDENT_PHASE5_PR5.code-review.md:457:+                        "subject_aliases", []
./EVIDENT_PHASE5_PR5.code-review.md:475:+            accepted_tolerances.append(raw_tol)
./EVIDENT_PHASE5_PR5.code-review.md:477:+        if not accepted_tolerances:
./EVIDENT_PHASE5_PR5.code-review.md:478:+            # Codex v3 contract: claim with zero valid tolerances is
./EVIDENT_PHASE5_PR5.code-review.md:488:+                subject_aliases=list(
./EVIDENT_PHASE5_PR5.code-review.md:489:+                    raw_claim.get("subject_aliases", [])
./EVIDENT_PHASE5_PR5.code-review.md:491:+                tolerances=accepted_tolerances,
./EVIDENT_PHASE5_PR5.code-review.md:524:+    "missing_subject": "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:526:+    "comparator_bound_to_wrong_subject": "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:632:+prompt reduces the *rate* at which the model proposes bad tolerances
./EVIDENT_PHASE5_PR5.code-review.md:658:+4. The claimed subject of the measurement (the system being claimed \
./EVIDENT_PHASE5_PR5.code-review.md:674:+- Attributing a bound to the wrong subject. If the source says \
./EVIDENT_PHASE5_PR5.code-review.md:677:+*baseline*, not to your subject.
./EVIDENT_PHASE5_PR5.code-review.md:678:+- Extracting when the source's subject is ambiguous. If the source \
./EVIDENT_PHASE5_PR5.code-review.md:688:+value, and subject. The validator checks this character-by-character.
./EVIDENT_PHASE5_PR5.code-review.md:691:+``comparator_bound_to_wrong_subject``, ``value_only_in_image_table``, \
./EVIDENT_PHASE5_PR5.code-review.md:731:+                                "the source-id and the claim subject."
./EVIDENT_PHASE5_PR5.code-review.md:743:+                        "subject_aliases": {
./EVIDENT_PHASE5_PR5.code-review.md:748:+                                "to the claimed subject (\"ours\", "
./EVIDENT_PHASE5_PR5.code-review.md:756:+                        "tolerances": {
./EVIDENT_PHASE5_PR5.code-review.md:773:+                                            "value, and subject in "
./EVIDENT_PHASE5_PR5.code-review.md:804:+                        "subject_aliases",
./EVIDENT_PHASE5_PR5.code-review.md:805:+                        "tolerances",
./EVIDENT_PHASE5_PR5.code-review.md:832:+                                "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:872:+        "subject) is missing or attached to a different subject, "
./EVIDENT_PHASE5_PR5.code-review.md:954:+    subject_aliases: list[str]
./EVIDENT_PHASE5_PR5.code-review.md:955:+    tolerances: list[dict]
./EVIDENT_PHASE5_PR5.code-review.md:1010:+        "tolerances": [_tolerance_block(t) for t in claim.tolerances],
./EVIDENT_PHASE5_PR5.code-review.md:1012:+            "oracle": ["Paper-Authority"],
./EVIDENT_PHASE5_PR5.code-review.md:1035:+        "last_verified": {
./EVIDENT_PHASE5_PR5.code-review.md:1046:+    extractor-side fields (``source_span``, ``subject_aliases``)
./EVIDENT_PHASE5_PR5.code-review.md:1083:+        for i, t in enumerate(claim.tolerances):
./EVIDENT_PHASE5_PR5.code-review.md:1111:+                f"  Tolerances: {len(c.tolerances)}; "
./EVIDENT_PHASE5_PR5.code-review.md:1112:+                f"subject aliases: {c.subject_aliases}"
./EVIDENT_PHASE5_PR5.code-review.md:1747:+claimed for the claimed subject.
./EVIDENT_PHASE5_PR5.code-review.md:1754:+   the bound value, AND at least one claimed-subject alias.
./EVIDENT_PHASE5_PR5.code-review.md:1758:+   subject in the same span does NOT satisfy the rule.
./EVIDENT_PHASE5_PR5.code-review.md:1871:+KIND_MISSING_SUBJECT = "missing_subject"
./EVIDENT_PHASE5_PR5.code-review.md:1873:+KIND_WRONG_BINDING = "comparator_bound_to_wrong_subject"
./EVIDENT_PHASE5_PR5.code-review.md:1883:+# wrong-subject case (F-PR4-CR1b): a markdown table row like
./EVIDENT_PHASE5_PR5.code-review.md:1887:+# comparator and bound to be in the same cell as the subject.
./EVIDENT_PHASE5_PR5.code-review.md:1903:+    bound, and subject must co-occur. For prose this is roughly a
./EVIDENT_PHASE5_PR5.code-review.md:2051:+def _subject_appears_in(text: str, aliases: Iterable[str]) -> bool:
./EVIDENT_PHASE5_PR5.code-review.md:2052:+    """True if any subject alias appears in `text` (case-insensitive,
./EVIDENT_PHASE5_PR5.code-review.md:2110:+    subject_aliases: Iterable[str],
./EVIDENT_PHASE5_PR5.code-review.md:2124:+    ``subject_aliases``: list of strings that refer to the claimed
./EVIDENT_PHASE5_PR5.code-review.md:2125:+    subject (``["our method", "we", "ours"]`` etc.). Provided by the
./EVIDENT_PHASE5_PR5.code-review.md:2126:+    extractor based on the source's own subject-identifying phrasings.
./EVIDENT_PHASE5_PR5.code-review.md:2155:+    # subject alias. Use the longest-match comparator scanner so
./EVIDENT_PHASE5_PR5.code-review.md:2164:+    subject_present_somewhere = False
./EVIDENT_PHASE5_PR5.code-review.md:2166:+    aliases = list(subject_aliases)
./EVIDENT_PHASE5_PR5.code-review.md:2176:+        has_subject = _subject_appears_in(sentence, aliases)
./EVIDENT_PHASE5_PR5.code-review.md:2180:+        subject_present_somewhere |= has_subject
./EVIDENT_PHASE5_PR5.code-review.md:2186:+            and has_subject
./EVIDENT_PHASE5_PR5.code-review.md:2232:+    if not subject_present_somewhere:
./EVIDENT_PHASE5_PR5.code-review.md:2236:+                "none of the claimed subject's aliases appear in "
./EVIDENT_PHASE5_PR5.code-review.md:2247:+            "metric, comparator, value, and subject appear in the "
./EVIDENT_PHASE5_PR5.code-review.md:2249:+            "the bound may be attached to a different subject in the "
./EVIDENT_PHASE5_PR5.code-review.md:2408:+'model emits clean output' path and the 'model emits bad tolerances
./EVIDENT_PHASE5_PR5.code-review.md:2530:+        "subject_aliases": ["our system", "we"],
./EVIDENT_PHASE5_PR5.code-review.md:2531:+        "tolerances": [
./EVIDENT_PHASE5_PR5.code-review.md:2554:+        "subject_aliases": ["our system"],
./EVIDENT_PHASE5_PR5.code-review.md:2555:+        "tolerances": [
./EVIDENT_PHASE5_PR5.code-review.md:2588:+def test_process_tool_response_drops_claim_with_zero_valid_tolerances():
./EVIDENT_PHASE5_PR5.code-review.md:2589:+    """Codex v3: if all tolerances on a claim fail validation, the
./EVIDENT_PHASE5_PR5.code-review.md:2683:+    """Path (b): model emits two marketing-language tolerances the
./EVIDENT_PHASE5_PR5.code-review.md:2695:+        "subject_aliases": ["our system", "blazestack"],
./EVIDENT_PHASE5_PR5.code-review.md:2696:+        "tolerances": [
./EVIDENT_PHASE5_PR5.code-review.md:2711:+        "subject_aliases": ["our system", "blazestack"],
./EVIDENT_PHASE5_PR5.code-review.md:2712:+        "tolerances": [
./EVIDENT_PHASE5_PR5.code-review.md:2733:+    # Both bad tolerances appear in the rejections list.
./EVIDENT_PHASE5_PR5.code-review.md:2750:+        "subject_aliases": ["our system"],
./EVIDENT_PHASE5_PR5.code-review.md:2751:+        "tolerances": [
./EVIDENT_PHASE5_PR5.code-review.md:2768:+        "subject_aliases": ["our system"],
./EVIDENT_PHASE5_PR5.code-review.md:2769:+        "tolerances": [
./EVIDENT_PHASE5_PR5.code-review.md:2796:+    spans = {c.tolerances[0]["source_span"] for c in result.claims}
./EVIDENT_PHASE5_PR5.code-review.md:2855:+def test_tool_schema_claim_requires_subject_aliases_and_source_span():
./EVIDENT_PHASE5_PR5.code-review.md:2862:+    assert "subject_aliases" in claim_schema["required"]
./EVIDENT_PHASE5_PR5.code-review.md:2863:+    tolerance_schema = claim_schema["properties"]["tolerances"]["items"]
./EVIDENT_PHASE5_PR5.code-review.md:2877:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:2902:+def test_system_prompt_calls_out_wrong_subject_binding():
./EVIDENT_PHASE5_PR5.code-review.md:2908:+    assert "wrong subject" in prompt or "bound to" in prompt
./EVIDENT_PHASE5_PR5.code-review.md:2981:+                subject_aliases=["our method", "we", "ours"],
./EVIDENT_PHASE5_PR5.code-review.md:2982:+                tolerances=[
./EVIDENT_PHASE5_PR5.code-review.md:3066:+    t = manifest["claims"][0]["tolerances"][0]
./EVIDENT_PHASE5_PR5.code-review.md:3077:+    v3: default-deny means honest emptiness > invented tolerances)."""
./EVIDENT_PHASE5_PR5.code-review.md:3585:+    tolerances:
./EVIDENT_PHASE5_PR5.code-review.md:3591:+      oracle: [Manual]
./EVIDENT_PHASE5_PR5.code-review.md:3631:+                subject_aliases=["our method", "we"],
./EVIDENT_PHASE5_PR5.code-review.md:3632:+                tolerances=[
./EVIDENT_PHASE5_PR5.code-review.md:3732:+metric, comparator, bound, AND subject all co-occur in the same local
./EVIDENT_PHASE5_PR5.code-review.md:3737:+claimed for the claimed subject.
./EVIDENT_PHASE5_PR5.code-review.md:3760:+    metric, and subject all in the same sentence."""
./EVIDENT_PHASE5_PR5.code-review.md:3770:+    subject_aliases = ["we", "our", "ours", "the proposed method"]
./EVIDENT_PHASE5_PR5.code-review.md:3771:+    validate_tolerance(tolerance, subject_aliases=subject_aliases)
./EVIDENT_PHASE5_PR5.code-review.md:3781:+    validate_tolerance(tolerance, subject_aliases=["our", "we", "ours"])
./EVIDENT_PHASE5_PR5.code-review.md:3794:+    validate_tolerance(tolerance, subject_aliases=["we", "our"])
./EVIDENT_PHASE5_PR5.code-review.md:3802:+def test_wrong_subject_binding_is_rejected():
./EVIDENT_PHASE5_PR5.code-review.md:3807:+    *baseline* subject, not the claimed subject.
./EVIDENT_PHASE5_PR5.code-review.md:3819:+            tolerance, subject_aliases=["our method", "we", "ours"]
./EVIDENT_PHASE5_PR5.code-review.md:3821:+    assert exc.value.kind == "comparator_bound_to_wrong_subject"
./EVIDENT_PHASE5_PR5.code-review.md:3846:+            tolerance, subject_aliases=["our", "we", "ours"]
./EVIDENT_PHASE5_PR5.code-review.md:3849:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:3856:+# Per-element rejection: missing metric, comparator, value, or subject
./EVIDENT_PHASE5_PR5.code-review.md:3868:+        validate_tolerance(tolerance, subject_aliases=["we", "our"])
./EVIDENT_PHASE5_PR5.code-review.md:3882:+            tolerance, subject_aliases=["our method", "we"]
./EVIDENT_PHASE5_PR5.code-review.md:3886:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:3899:+            tolerance, subject_aliases=["our", "we"]
./EVIDENT_PHASE5_PR5.code-review.md:3903:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:3907:+def test_missing_subject_is_rejected():
./EVIDENT_PHASE5_PR5.code-review.md:3908:+    """If the source span never mentions the claimed subject, the
./EVIDENT_PHASE5_PR5.code-review.md:3920:+            subject_aliases=["our method", "our system", "we propose"],
./EVIDENT_PHASE5_PR5.code-review.md:3922:+    assert exc.value.kind == "missing_subject"
./EVIDENT_PHASE5_PR5.code-review.md:3973:+    # Should NOT raise. The bound, comparator, and subject are all in
./EVIDENT_PHASE5_PR5.code-review.md:3975:+    validate_tolerance(tolerance, subject_aliases=["our method", "we"])
./EVIDENT_PHASE5_PR5.code-review.md:4010:+        tolerance, subject_aliases=["our system", "we", "our"]
./EVIDENT_PHASE5_PR5.code-review.md:4042:+            tolerance, subject_aliases=["our method", "we"]
./EVIDENT_PHASE5_PR5.code-review.md:4062:+            tolerance, subject_aliases=["our", "we"]
./EVIDENT_PHASE5_PR5.code-review.md:4068:+# if the bound + subject co-occur in ONE of them
./EVIDENT_PHASE5_PR5.code-review.md:4088:+            tolerance, subject_aliases=["our method"]
./EVIDENT_PHASE5_PR5.code-review.md:4103:+        validate_tolerance(tolerance, subject_aliases=["our"])
./EVIDENT_PHASE5_PR5.code-review.md:4117:+        tolerance, subject_aliases=["our method"]
./EVIDENT_PHASE5_PR5.code-review.md:4131:+        tolerance, subject_aliases=["our method"]
./EVIDENT_PHASE5_PR5.code-review.md:4143:+        # present, the subject is present, the metric is present —
./EVIDENT_PHASE5_PR5.code-review.md:4152:+            tolerance, subject_aliases=["our method"]
./EVIDENT_PHASE5_PR5.code-review.md:4156:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:4172:+            tolerance, subject_aliases=["our"]
./EVIDENT_PHASE5_PR5.code-review.md:4176:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:4192:+            tolerance, subject_aliases=["our"]
./EVIDENT_PHASE5_PR5.code-review.md:4196:+        "comparator_bound_to_wrong_subject",
./EVIDENT_PHASE5_PR5.code-review.md:4218:+        validate_tolerance(tolerance, subject_aliases=["ours"])
./EVIDENT_PHASE5_PR5.code-review.md:4219:+    assert exc.value.kind == "comparator_bound_to_wrong_subject"
./EVIDENT_PHASE5_PR5.code-review.md:4223:+    """Counterpart: when the comparator, bound, metric, and subject
./EVIDENT_PHASE5_PR5.code-review.md:4232:+        tolerance, subject_aliases=["our method"]
./EVIDENT_PHASE5_PR5.code-review.md:4236:+def test_subject_alias_with_trailing_punctuation_anchors_correctly():
./EVIDENT_PHASE5_PR5.code-review.md:4237:+    """Codex F-PR4-CR2b: subject aliases ending in non-word chars
./EVIDENT_PHASE5_PR5.code-review.md:4251:+        tolerance, subject_aliases=["ABRA-2.0", "ours"]
./EVIDENT_PHASE5_PR5.code-review.md:4257:+    has all four (metric, comparator, value, subject), the tolerance
./EVIDENT_PHASE5_PR5.code-review.md:4270:+        tolerance, subject_aliases=["our method", "we", "ours"]

codex
I found the draft already checked into the workspace and the existing PR5f-style code/tests. I’m narrowing the review to actual schema boundaries: claim declarations, translator rejection rules, MCP sidecar dispatch, and the rustims extracted manifests.
exec
/bin/bash -lc "sed -n '1370,1580p' typed-trust/tests/translator.rs" in /scratch/TMAlign/evident
exec
/bin/bash -lc "sed -n '1,380p' typed-trust/src/translate.rs" in /scratch/TMAlign/evident
 succeeded in 0ms:
    } else {
        panic!("expected Judged derivation, got {:?}", evidence.supports.derivation);
    }
}

// ----------------------------------------------------------------------
// PR5b: metadata_compatibility claim kind
// ----------------------------------------------------------------------

#[test]
fn metadata_claim_translates_with_metadata_block() {
    let yaml = r#"
claims:
  - id: pdbtbx-rust-msrv
    title: pdbtbx requires Rust MSRV 1.67+
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: |
      pdbtbx's Cargo.toml declares rust-version = "1.67"
    metadata:
      field: rust_msrv
      declared_value: "1.67"
      source_file: Cargo.toml
      source_path: package.rust-version
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    assert_eq!(attested.value.kind, ClaimKind::MetadataCompatibility);
    assert_eq!(attested.value.id.as_str(), "pdbtbx-rust-msrv");
}

// PR5c: the manifest's `metadata:` block must be lifted onto the typed
// Claim so the render layer (which reaches the declaration via
// RenderInput) can surface it. Without this the metadata block parses
// but is dropped on the floor between translate and render.
#[test]
fn metadata_claim_lifts_block_onto_typed_claim_pr5c() {
    let yaml = r#"
claims:
  - id: pdbtbx-rust-msrv
    title: pdbtbx requires Rust MSRV 1.67+
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: pdbtbx declares rust-version = "1.67" in Cargo.toml
    metadata:
      field: rust_msrv
      declared_value: "1.67"
      source_file: Cargo.toml
      source_path: package.rust-version
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    let md = attested
        .value
        .metadata
        .as_ref()
        .expect("metadata declaration present on typed Claim");
    assert_eq!(md.field, "rust_msrv");
    assert_eq!(md.declared_value, "1.67");
    assert_eq!(md.source_file, "Cargo.toml");
    assert_eq!(md.source_path, "package.rust-version");
}

// PR5c: measurement claims do NOT carry a metadata declaration even
// though the struct field exists — keeps the typed Claim's two paths
// disjoint at the type level.
#[test]
fn measurement_claim_has_no_metadata_field_on_typed_claim_pr5c() {
    let yaml = r#"
claims:
  - id: m
    title: measurement
    kind: measurement
    tier: research
    source: .
    claim: c
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: ok
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    assert!(attested.value.metadata.is_none());
}

#[test]
fn metadata_claim_without_metadata_block_is_rejected() {
    let yaml = r#"
claims:
  - id: missing-meta
    title: missing metadata block
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: missing metadata block
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    match err {
        TranslateError::MetadataClaimMissingBlock { id } => {
            assert_eq!(id, "missing-meta");
        }
        other => panic!("expected MetadataClaimMissingBlock, got {other:?}"),
    }
}

#[test]
fn metadata_claim_with_tolerances_is_rejected() {
    let yaml = r#"
claims:
  - id: bad-meta
    title: metadata claim with tolerances
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: bad meta
    metadata:
      field: x
      declared_value: "1"
      source_file: pyproject.toml
      source_path: x
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: should not be allowed
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::MetadataClaimCarriesTolerances { .. }),
        "expected MetadataClaimCarriesTolerances, got {err:?}",
    );
}

#[test]
fn measurement_claim_with_metadata_block_is_rejected() {
    let yaml = r#"
claims:
  - id: bad-measurement
    title: measurement claim with metadata block
    kind: measurement
    tier: research
    case: src.md
    source: ..
    claim: should not have metadata
    tolerances:
      - metric: x
        op: "<"
        value: 1.0
        prose: ok
    evidence:
      oracle: [Manual]
      command: echo
      artifact: out.txt
    metadata:
      field: x
      declared_value: "1"
      source_file: pyproject.toml
      source_path: x
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::MeasurementClaimCarriesMetadata { .. }),
        "expected MeasurementClaimCarriesMetadata, got {err:?}",
    );
}

#[test]
fn metadata_claim_emits_no_criteria() {
    let yaml = r#"
claims:
  - id: pkg-python
    title: package requires Python >= 3.10
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: pkg requires Python >= 3.10
    metadata:
      field: python_version_requirement
      declared_value: ">=3.10"
      source_file: pyproject.toml
      source_path: project.requires-python
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let criteria = translate_tolerances(mc).unwrap();
    assert!(
        criteria.is_empty(),
        "metadata claim should have empty criteria, got {criteria:?}",

 succeeded in 0ms:
//! Manifest → Typed Trust translator.
//!
//! Implements the §11 seam from `concepts/typed-trust.md`: a
//! deterministic projection from the shipping `evident.yaml` schema's
//! measurement-class claims into Typed Trust constructors.
//!
//! Scope (MVP):
//! - Parses the top-level manifest YAML and per-claim measurement
//!   fields into [`ManifestFile`] / [`ManifestClaim`].
//! - Translates one [`ManifestClaim`] into an [`Attested<Claim>`] with
//!   a Verified extraction (per §4 footnote: structured manifest input
//!   yields a Verified, not Judged, extraction).
//! - Translates the per-claim `tolerances` block into [`Tolerance`]
//!   values, populating `against` from a single-oracle heuristic
//!   (multi-oracle disambiguation needs schema work or convention).
//! - Rejects `kind: policy | reference` as [`TranslateError::OutOfScope`]
//!   per §0.
//!
//! Out of scope for this MVP (follow-up work):
//! - Translating `inputs` / `outputs` / `pinned_versions` into
//!   `ProvenanceRecord` + `ToolInvocation.env`.
//! - Translating `evidence.command / artifact` into [`Evidence`].
//! - Translating `last_verified` into a [`Rerun`].
//! - Translating `provenance: peer-reviewed` reviewers into
//!   [`ReviewEvent`]s.
//! - Translating assumptions into `Attested<Assumption>` (currently
//!   dropped on the floor).

use serde::Deserialize;

use crate::claim::{Claim, ClaimKind, MetadataDeclaration, SourceSpan};
use crate::derivation::{
    Attested, Derivation, Locator, Rerun, ReproductionOutcome, ToolInvocation,
};
use crate::evidence::{
    Evidence, EvidenceKind, ReplayReason, ReplayStatus, Strength, SupportRelation,
};
use crate::identity::{Identity, IdentityDetail, IdentityKind};
use crate::ids::{ClaimId, CriterionId, EventId, EvidenceId, Timestamp};
use crate::report::{ComparisonOp, MetricObservation, Tolerance};
use crate::derivation::Confidence;

// ---------- Manifest shape ----------

/// The top-level shape of an `evident.yaml` or included claim file.
/// `claims` is optional: a top-level manifest may carry only an
/// `include:` list and no claims of its own (proteon's `evident.yaml`
/// follows this pattern). `version`, `project`, `vocabularies`,
/// `include` are not parsed at this layer — the CLI handles include
/// resolution.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestFile {
    #[serde(default)]
    pub claims: Vec<ManifestClaim>,
}

/// Subset of the shipping `claim` schema sufficient for the MVP
/// translator. Many manifest fields (subsystem, trust_strategy,
/// capabilities, inputs, outputs, pinned_versions, last_verified)
/// are NOT consumed yet — see module-level scope.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestClaim {
    pub id: String,
    pub title: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub case: Option<String>,
    pub source: Option<String>,
    pub tier: String,
    pub claim: String,
    pub tolerances: Option<Vec<ManifestTolerance>>,
    pub evidence: Option<ManifestEvidence>,
    pub provenance: Option<ManifestProvenance>,
    pub last_verified: Option<ManifestLastVerified>,
    pub assumptions: Option<Vec<String>>,
    pub failure_modes: Option<Vec<String>>,
    /// PR5b: required when ``kind == "metadata_compatibility"``.
    /// Carries the declarative configuration claim — what field is
    /// being asserted, what value the source declares, and which
    /// config file the value came from. Absent for empirical
    /// (measurement) claims.
    #[serde(default)]
    pub metadata: Option<ManifestMetadataBlock>,
    /// PR5f: required when ``kind == "behavioral_concordance"``.
    /// Carries the pattern (numeric_band, relative_band, etc.) +
    /// paper_locator + prior_binding. Absent for any other kind;
    /// the translator rejects mixing.
    #[serde(default)]
    pub concordance: Option<ManifestConcordanceBlock>,
}

/// PR5b: structured block for ``kind: metadata_compatibility``
/// claims. The declaration IS the evidence: the source's
/// pyproject.toml / Cargo.toml / package.json stated this value,
/// no synthesis or measurement required.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestMetadataBlock {
    /// Semantic name of the field being declared (e.g.
    /// ``python_version_requirement``, ``rust_msrv``,
    /// ``node_version_requirement``).
    pub field: String,
    /// The literal value the source declares (e.g. ``">=3.10"``,
    /// ``"1.67"``).
    pub declared_value: String,
    /// The config file the declaration came from
    /// (e.g. ``"pyproject.toml"``).
    pub source_file: String,
    /// The path within the config file where the value lives
    /// (e.g. ``"project.requires-python"`` for TOML, ``"engines.node"``
    /// for package.json).
    pub source_path: String,
}

/// PR5f: structured block for ``kind: behavioral_concordance``
/// claims.
///
/// The shape is a discriminated union on `pattern_kind`. Five
/// variants — `numeric_band`, `relative_band`,
/// `same_order_of_magnitude`, `ordinal_match`, `monotone_with`.
/// Codex v3 review insisted on discriminator dispatch (NOT a
/// Serde untagged union) so each pattern's required fields get
/// specific error messages at parse time, not vague "could not
/// match any variant" failures.
///
/// `deny_unknown_fields` at the variant level catches typos like
/// `prior_valu:` (instead of `prior_value:`) which would otherwise
/// silently drop the prior. Codex's same-PR pattern from PR5b's
/// `metadata` block.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestConcordanceBlock {
    /// The pattern variant + its typed fields. YAML:
    /// ``concordance.pattern.pattern_kind: numeric_band`` is the
    /// discriminator; the rest of the pattern's typed fields
    /// (`metric_path`, `epsilon`, `prior_value`, etc.) live under
    /// the same `pattern` block. Nesting (vs. `serde(flatten)`)
    /// is required because Serde's flatten doesn't compose with
    /// internally-tagged enums.
    pub pattern: ManifestConcordancePattern,
    /// Where in *this* paper the concordance claim is made. v4
    /// design: concordance claims do NOT carry the
    /// measurement-flavored top-level `source` field — they use
    /// `paper_locator` instead so a manifest never has to
    /// disambiguate "is this the paper-side or the prior-side
    /// citation."
    pub paper_locator: String,
    pub prior_binding: ManifestPriorBindingBlock,
}

/// PR5f: discriminator-dispatched pattern variants.
///
/// `pattern_kind` is the discriminator; each variant carries its
/// own typed parameters AND its own typed `prior_value` shape
/// (scalar for the three scalar primitives, per-entity map for
/// `ordinal_match`, absent for `monotone_with` whose prior is the
/// series shape not a value).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "pattern_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManifestConcordancePattern {
    NumericBand {
        metric_path: String,
        epsilon: f64,
        prior_value: f64,
    },
    RelativeBand {
        metric_path: String,
        ratio: f64,
        prior_value: f64,
    },
    SameOrderOfMagnitude {
        metric_path: String,
        #[serde(default = "default_zero_policy")]
        zero_policy: String,
        prior_value: f64,
    },
    OrdinalMatch {
        entity_to_path: std::collections::BTreeMap<String, String>,
        direction: String,
        #[serde(default = "default_tie_policy")]
        tie_policy: String,
        prior_value: std::collections::BTreeMap<String, f64>,
    },
    MonotoneWith {
        metric_path: String,
        parameter_path: String,
        direction: String,
    },
}

fn default_zero_policy() -> String {
    "not_assessed".into()
}

fn default_tie_policy() -> String {
    "strict".into()
}

/// PR5f: the curator-authored prior binding block. v4 design
/// makes the five fields required; they're not optional.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPriorBindingBlock {
    pub prior_unit: String,
    pub prior_metric_definition: String,
    pub locator: String,
    pub prior_extraction_note: String,
    pub source_id: String,
}

/// Phase 5 PR2: the manifest's `provenance` field accepts either the
/// legacy string form (`provenance: automatic`) or a structured object
/// (`provenance: { kind, source_id, ... }`). The structured form is
/// what `evident-extract` writes; the legacy form is what every
/// pre-Phase-5 manifest has and must keep working unchanged.
///
/// Use `effective_kind()` to get the kind string for the existing
/// callers that branch on `automatic` / `human` / `peer-reviewed`
/// without caring about the new sub-fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestProvenance {
    /// Pre-Phase-5 form: `provenance: automatic | human | peer-reviewed`
    /// or any other free-form string. Only `kind` is carried.
    Legacy(String),
    /// Phase 5 form: structured provenance with extractor metadata.
    Structured(ProvenanceBlock),
}

/// Phase 5 PR2: the structured `provenance:` block.
///
/// `kind` is the only required field. Everything else is optional so
/// a manifest can declare `extracted-from-paper` without committing to
/// a particular extractor or source_id at authoring time.
///
/// `deny_unknown_fields` (codex F-PR2-CR2) catches typos like
/// `source_contxt:` at parse time instead of silently dropping them.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceBlock {
    /// The provenance discriminator. Phase 5 introduces
    /// `extracted-from-paper` and `extracted-from-repo`; legacy
    /// values (`automatic`, `human`, `peer-reviewed`) are also
    /// accepted here, so a manifest author who prefers the
    /// structured form can use it for legacy provenance too.
    pub kind: String,
    /// Opaque source identifier — for papers: `arxiv:2501.12345v2`,
    /// `doi:10.1234/xyz`. For repos: `github:org/repo@<sha>`.
    pub source_id: Option<String>,
    /// SHA-256 of the source artifact (e.g. the PDF or the repo
    /// snapshot) so re-extraction is reproducible against the same
    /// source bytes.
    pub source_sha: Option<String>,
    /// Provenance of the text the claim was extracted FROM. Distinct
    /// from `kind`, which is the provenance of the CLAIM. Parses as
    /// a typed enum so an unknown value (`source_context:
    /// completely_made_up`) is rejected at parse time, not at
    /// translate time (codex F-PR2-CR1). This closes the
    /// `list_claims` bypass — every value MCP surfaces is one of
    /// the three legal strings.
    pub source_context: Option<SourceContext>,
    /// Extractor metadata. Optional so manifests can pre-declare
    /// structured provenance before the extractor runs.
    pub extractor: Option<ExtractorBlock>,
    /// Curator identity (set after a human review, null at
    /// extraction time). Free-form here so PR2 doesn't commit to a
    /// curator-identity schema; PR3 will refine.
    pub curator: Option<serde_yaml_ng::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractorBlock {
    pub model: Option<String>,
    pub model_version: Option<String>,
    pub extracted_at: Option<String>,
}

/// Phase 5 PR2: provenance of the source text a claim was extracted
/// FROM. Distinct from `provenance.kind`, which is the provenance of
/// the claim itself.
///
/// Parses as `#[serde(rename_all = "snake_case")]` so the YAML strings
/// are `repo_authored`, `copied_external_text`, `unknown`. Anything
/// else fails at deserialization time with a serde error naming the
/// unknown variant — the validator-at-translate-time pattern was
/// replaced by parse-time enum validation per codex F-PR2-CR1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceContext {
    /// Text was written for the artifact it lives in (e.g. the repo's
    /// own README, the paper's own body).
    RepoAuthored,
    /// Text was copied verbatim from a separate authoritative source
    /// (vendored README, corporate marketing copy, etc.).
    CopiedExternalText,
    /// Extractor could not determine.
    Unknown,
}

impl SourceContext {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceContext::RepoAuthored => "repo_authored",
            SourceContext::CopiedExternalText => "copied_external_text",
            SourceContext::Unknown => "unknown",
        }
    }
}

impl ManifestProvenance {
    /// The provenance kind string — `automatic`, `human`,
    /// `peer-reviewed`, `extracted-from-paper`, `extracted-from-repo`,
    /// etc. Callers that historically branched on `mc.provenance` as a
    /// string use this; the structured form's `kind` is returned
    /// unchanged.
    pub fn effective_kind(&self) -> &str {
        match self {
            ManifestProvenance::Legacy(s) => s.as_str(),
            ManifestProvenance::Structured(b) => b.kind.as_str(),
        }
    }
    pub fn source_id(&self) -> Option<&str> {
        match self {
            ManifestProvenance::Legacy(_) => None,
            ManifestProvenance::Structured(b) => b.source_id.as_deref(),
        }
    }
    pub fn source_sha(&self) -> Option<&str> {
        match self {
            ManifestProvenance::Legacy(_) => None,
            ManifestProvenance::Structured(b) => b.source_sha.as_deref(),
        }
    }
    pub fn source_context(&self) -> Option<&'static str> {
        match self {
            ManifestProvenance::Legacy(_) => None,
            ManifestProvenance::Structured(b) => b.source_context.map(|s| s.as_str()),
        }
    }
    pub fn extractor_model(&self) -> Option<&str> {
        match self {
            ManifestProvenance::Legacy(_) => None,
            ManifestProvenance::Structured(b) => {
                b.extractor.as_ref().and_then(|e| e.model.as_deref())
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestLastVerified {
    pub commit: Option<String>,
    pub date: Option<String>,
    pub value: Option<f64>,
    pub corpus_sha: Option<String>,
}

fn default_kind() -> String {
    "measurement".into()
}

/// Mirrors the shipping schema's tolerance entry. Per `workflow/SCHEMA.md`:
/// `metric`, `op`, `value` are all-or-nothing — either supply all three
/// for a structured tolerance, or supply none and use only `prose` (the
/// research-tier deferred-spec case). `prose` is always required.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestTolerance {
    pub metric: Option<String>,
    pub op: Option<String>,
    pub value: Option<f64>,
    pub output: Option<String>,
    pub prose: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEvidence {
    pub oracle: Vec<String>,
    pub command: String,
    pub artifact: String,

exec
/bin/bash -lc "sed -n '1,260p' typed-trust/src/claim.rs" in /scratch/TMAlign/evident
 succeeded in 0ms:
//! Claim — see §5 of `concepts/typed-trust.md`.
//!
//! Propositional content only. Review actions (Endorse, Dissent,
//! Challenge, Supersede) live in `ReviewEvent`, not as Claim variants.

use crate::derivation::Attested;
use crate::ids::ClaimId;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Claim {
    pub id: ClaimId,
    pub text: String,
    pub kind: ClaimKind,
    pub source: SourceSpan,
    /// Stated verbatim in the source vs. inferred.
    pub explicit: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub decomposes_into: Vec<ClaimId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires_assumptions: Vec<Attested<Assumption>>,
    /// PR5c: declarative configuration claim from a config file.
    /// Only `Some` when `kind == MetadataCompatibility`; the typed
    /// declaration carries field name, declared value, and which
    /// config file/path it came from. Renderers surface this in
    /// place of the (missing) criteria section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataDeclaration>,
    /// PR5f: behavioral concordance — paper claims its measured
    /// behavior tracks a prior paper's reported behavior. Only
    /// `Some` when `kind == BehavioralConcordance`. Replaces the
    /// `source: SourceSpan` plumbing for this kind: concordance
    /// claims do NOT carry the measurement-flavored `source`
    /// field at the schema layer (see
    /// `EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md` v4
    /// "paper_locator is a schema exception").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concordance: Option<ConcordanceDeclaration>,
}

/// PR5f: typed lift of the manifest's `concordance:` block.
///
/// A concordance claim asserts: "my measured behavior at
/// `metric_path` (in the docker artifact) tracks the prior value
/// curated into `prior_binding`, under the relationship declared
/// by `pattern`."
///
/// The framework owns the relationship vocabulary (the
/// `ConcordancePattern` enum + its per-variant parameters and
/// prior shapes). The curator owns the prior binding (transcribing
/// the prior paper's value, unit, metric definition, and
/// extraction provenance). The docker artifact owns the measured
/// value. See the draft for the full layer split.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ConcordanceDeclaration {
    /// The framework-owned pattern: which relationship is being
    /// asserted between the measured value and the prior. Carries
    /// the per-variant parameters and the typed `prior_value` so
    /// every variant has a structurally complete shape.
    pub pattern: ConcordancePattern,
    /// Where in *this* paper the concordance claim is made.
    /// Replaces the measurement-flavored top-level `source` field
    /// for this kind (avoids the "is `source` the rustims-side
    /// or Meier-side citation?" ambiguity).
    pub paper_locator: String,
    /// Curator-authored prior binding: the prose + locator + audit
    /// fields that pin down what the prior actually says. Required
    /// for every concordance claim. `prior_value` lives inside
    /// `pattern` (single source of truth, pattern-typed); this
    /// block carries the human-facing context.
    pub prior_binding: PriorBindingContext,
}

/// PR5f: per-pattern typed shape carrying both the pattern's
/// parameters and its typed prior value. Discriminator-dispatched
/// deserialization (parse `pattern_kind` first, then validate the
/// variant's required fields) — NOT a Serde untagged union, to
/// keep error messages specific (per codex v3 review).
///
/// Variant naming follows the schema's snake_case
/// (`numeric_band`, etc.); see `serde` tag below.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "pattern_kind", rename_all = "snake_case")]
pub enum ConcordancePattern {
    /// Measured value must lie within
    /// `[prior_value - epsilon, prior_value + epsilon]`.
    /// Use when the paper cites a single numeric figure with an
    /// absolute tolerance band (e.g. "FDR within ±0.5 pp of
    /// Meier 2024 Table 3").
    NumericBand {
        /// Top-level metric_path (where in the docker artifact
        /// to read the measured value).
        metric_path: String,
        epsilon: f64,
        /// Pattern-typed prior: scalar.
        prior_value: f64,
    },
    /// Measured value must lie within
    /// `[prior_value / ratio, prior_value * ratio]`.
    /// Use for multiplicative bands ("runtime within 2× of
    /// baseline"). `ratio` must be > 1.0.
    RelativeBand {
        metric_path: String,
        ratio: f64,
        prior_value: f64,
    },
    /// `floor(log10(measured)) == floor(log10(prior_value))`.
    /// Use when the magnitude band is what's load-bearing, not the
    /// exact value. Restricted to strictly positive metrics;
    /// `zero_policy` governs what happens when the measured value
    /// is non-positive.
    SameOrderOfMagnitude {
        metric_path: String,
        /// Behavior when the measured value is `<= 0`.
        /// `prior_value <= 0` is a curator authoring error caught
        /// at translate time.
        zero_policy: ZeroPolicy,
        /// Pattern-typed prior: strictly positive scalar
        /// (validated at translate time).
        prior_value: f64,
    },
    /// The ranking of entities (by their measured values under
    /// `direction`) must match the ranking implied by
    /// `prior_value` (a per-entity prior map). Unlike the other
    /// primitives, `ordinal_match` does NOT carry a top-level
    /// `metric_path` — each entity's measured value resolves via
    /// `entity_to_path` to its own artifact location.
    ///
    /// Translate-time validator: `prior_value`'s keyset MUST
    /// equal `entity_to_path`'s keyset.
    OrdinalMatch {
        /// Explicit per-entity artifact paths. Replaces v2's
        /// implicit `{entity}` substitution (codex v3 fix).
        entity_to_path: std::collections::BTreeMap<String, String>,
        direction: RankingDirection,
        tie_policy: TiePolicy,
        /// Pattern-typed prior: per-entity map. Keyset MUST match
        /// `entity_to_path`.
        prior_value: std::collections::BTreeMap<String, f64>,
    },
    /// The measured series at `metric_path`, when sorted by the
    /// paired parameter series at `parameter_path`, is monotone
    /// in `direction`. No prior numeric value — the prior is the
    /// *shape* of the series, captured in
    /// `PriorBindingContext.prior_metric_definition`.
    MonotoneWith {
        metric_path: String,
        parameter_path: String,
        direction: MonotoneDirection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZeroPolicy {
    /// Treat a non-positive measured value as a translate-time
    /// rejection (the artifact is malformed for the declared
    /// claim).
    Reject,
    /// Treat a non-positive measured value as `NotAssessed`
    /// (replay ran but the comparison can't be made; the criterion
    /// stays unassessed rather than failing).
    NotAssessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingDirection {
    LowerIsBetter,
    HigherIsBetter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TiePolicy {
    /// Any ties between adjacent entities fail the comparison.
    Strict,
    /// A single adjacent-pair swap relative to `prior_value`'s
    /// ranking is tolerated (the common "within noise" case).
    AdjacentSwapOk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MonotoneDirection {
    Increasing,
    Decreasing,
}

/// PR5h: typed lift of a `last_concorded.json` entry.
///
/// The agent's comparator (`evident_agent.concordance`) produces
/// a `ConcordanceResult` per concordance claim and writes it to
/// `last_concorded.json`. typed-trust reads it back here. The
/// shape mirrors the Python `LastConcordedEntry` exactly so the
/// two layers round-trip without translation.
///
/// `comparison_status` is the load-bearing discriminator the
/// framework reads for status synthesis. Pattern-specific fields
/// (`observed_ordering` / `observed_series` etc.) round-trip
/// through but the synthesizer doesn't interpret them; they're
/// audit material for render.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConcordanceResult {
    /// `"pass" | "fail" | "not_assessed"` — the comparator's
    /// verdict. Kept as a typed enum so the synthesizer can
    /// dispatch without string-matching.
    pub comparison_status: ComparisonStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_ordering: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_ordering: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_series: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_series: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub produced_at: Option<String>,
    /// Free-form diagnostics block from the comparator (e.g.
    /// `{"delta_from_prior": 0.1, "within_band": true}`).
    /// Preserved verbatim through serde so the rendered output
    /// can surface whatever the comparator chose to record.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub diagnostics: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonStatus {
    Pass,
    Fail,
    NotAssessed,
}

/// PR5f: curator-authored prior binding context.
///
/// The five required fields pin down what the prior paper
/// actually says — without this, a curator who reads "1.5%" from
/// Meier's Table 3 when the actual cell says "1.4%" is an error
/// the framework cannot catch. The audit trail is structural,
/// not prose-only.
///
/// `prior_value` is NOT here (single source of truth: lives on
/// the typed `ConcordancePattern`, pattern-typed for the variant).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PriorBindingContext {
    /// Unit of the prior value (e.g. `percentage_points`,
    /// `seconds`). Unit-mismatch is the #1 silent concordance
    /// failure; required even for `MonotoneWith` (describes the
    /// series' unit).
    pub prior_unit: String,
    /// Multi-sentence prose describing precisely what the prior
    /// metric IS — denominator, preprocessing, what's excluded.
    /// "FDR" means different things in different papers; the
    /// curator pins down which.

exec
/bin/bash -lc "sed -n '1,260p' typed-trust/src/mcp/tools.rs" in /scratch/TMAlign/evident
 succeeded in 0ms:
//! Tool definitions for the MCP server.
//!
//! Tool descriptions follow the 6-point rubric from the Phase 3
//! plan: when to use, when NOT to use, required relationship
//! between manifest_path/sidecar/allow-list, pagination behavior,
//! summary vs. full content, and enum value semantics.

use serde_json::{json, Value};

/// Return the list of tool definitions advertised on `tools/list`.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        list_claims_tool(),
        read_report_tool(),
        list_review_events_tool(),
        query_claims_tool(),
        get_panel_summary_tool(),
        get_superseded_events_tool(),
        walk_backing_chain_tool(),
        render_report_tool(),
        query_metadata_tool(),
        query_concordance_tool(),
    ]
}

fn list_claims_tool() -> Value {
    json!({
        "name": "list_claims",
        "description": "List every claim in a manifest. Use to discover what claims exist before drilling into one with read_report. Returns summary fields only (claim_id, title, tier, kind). For full report content, follow up with read_report. Supports pagination via limit + cursor.\n\n`manifest_path` must lie under an allowed root configured at server startup (--allow-manifest).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string", "description": "Filesystem path to the manifest YAML"},
                "limit": {"type": "integer", "minimum": 1, "description": "Max claims to return"},
                "cursor": {"type": "string", "description": "Opaque continuation token from a prior truncated response"}
            },
            "required": ["manifest_path"]
        }
    })
}

fn read_report_tool() -> Value {
    json!({
        "name": "read_report",
        "description": "Synthesize and return the augmented TrustReport for one claim. Use when you need the full typed-trust report (status, criteria, panel_summary, backing_reports, etc.). Don't use for shallow questions — list_claims or query_claims are cheaper. Returns the augmented JSON the renderers consume.\n\n`manifest_path` must be allow-listed. Optional `sidecar` overlays a review_events.json; optional `last_verified_sidecar` overlays a last_verified.json.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string", "description": "Optional review_events.json sidecar path"},
                "last_verified_sidecar": {"type": "string", "description": "Optional last_verified.json sidecar path"}
            },
            "required": ["manifest_path", "claim_id"]
        }
    })
}

fn list_review_events_tool() -> Value {
    json!({
        "name": "list_review_events",
        "description": "Inspect Endorse, Dissent, Challenge, and Supersede events. Prefer claim_id-scoped calls when you need rationale text: they're small and include_rationale defaults true. For corpus-wide scans, leave claim_id unset, set include_rationale=false, and combine filters with pagination. Returns event summaries; for the full augmented TrustReport use read_report instead.\n\nFilters compose conjunctively: kind in {endorse, dissent, challenge, supersede}; author matches identity name; event_id selects exactly one event.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string", "description": "Restrict to one claim's events"},
                "sidecar": {"type": "string", "description": "review_events.json sidecar path"},
                "author": {"type": "string", "description": "Filter by author name (matches Identity.name)"},
                "kind": {"type": "string", "enum": ["endorse", "dissent", "challenge", "supersede"]},
                "event_id": {"type": "string", "description": "Filter to one specific event id"},
                "include_rationale": {"type": "boolean", "description": "Include rationale text in each event row"},
                "limit": {"type": "integer", "minimum": 1},
                "cursor": {"type": "string"}
            },
            "required": ["manifest_path", "sidecar"]
        }
    })
}

fn query_claims_tool() -> Value {
    json!({
        "name": "query_claims",
        "description": "Return claim ids matching a conjunction of filter predicates. Use to answer corpus-level questions like 'which claims are contested?' or 'which claims has reviewer X participated in?'. Combines naturally with read_report for follow-up drilldowns.\n\nPredicates: `status` in {current, contested, superseded}; `reviewer` matches Identity.name on any event for the claim; `event_kind` in {endorse, dissent, challenge, supersede}; boolean filters has_panel_summary and has_superseded.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "sidecar": {"type": "string"},
                "status": {"type": "string", "enum": ["current", "contested", "superseded"]},
                "reviewer": {"type": "string"},
                "event_kind": {"type": "string", "enum": ["endorse", "dissent", "challenge", "supersede"]},
                "has_panel_summary": {"type": "boolean"},
                "has_superseded": {"type": "boolean"},
                "limit": {"type": "integer", "minimum": 1},
                "cursor": {"type": "string"}
            },
            "required": ["manifest_path"]
        }
    })
}

fn get_panel_summary_tool() -> Value {
    json!({
        "name": "get_panel_summary",
        "description": "Return the panel_summary block for one claim. Use when the user asks about reviewer agreement / divergence on a specific claim. Phase 2c projection reflects ACTIVE verdicts only (Phase 2d Supersede semantics applied).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string"}
            },
            "required": ["manifest_path", "claim_id", "sidecar"]
        }
    })
}

fn get_superseded_events_tool() -> Value {
    json!({
        "name": "get_superseded_events",
        "description": "Return Phase 2d audit material for one claim: the three subsections (valid superseded pairs / unresolved / invalid) used in the rendered Superseded Events section. Use when the user asks why an event was retired, or to inspect re-judgment history.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string"}
            },
            "required": ["manifest_path", "claim_id", "sidecar"]
        }
    })
}

fn walk_backing_chain_tool() -> Value {
    json!({
        "name": "walk_backing_chain",
        "description": "Walk the backing-claim graph rooted at one claim, grouped by originating Challenge event. Use to answer questions like 'why is this claim contested?' or 'what backs this challenge?'. Returns nested {challenges -> backing_claims -> children} with cycle detection and a configurable max_depth (default 4). Optional event_id filters to one branch.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string"},
                "event_id": {"type": "string", "description": "Optional: limit traversal to this Challenge event"},
                "max_depth": {"type": "integer", "minimum": 1, "default": 4}
            },
            "required": ["manifest_path", "claim_id", "sidecar"]
        }
    })
}

fn query_metadata_tool() -> Value {
    json!({
        "name": "query_metadata",
        "description": "Query declarative `metadata_compatibility` claims in a manifest. Use when the user asks about config-file claims (Python version requirements, Rust MSRV, Node engine ranges, etc.). For empirical / measurement claims use list_claims + read_report instead.\n\nFilters compose conjunctively, all exact-match and case-sensitive: `field` (semantic name like `rust_msrv`), `source_file` (e.g. `Cargo.toml`). Each result item carries the four metadata fields plus the same audit context list_claims returns (title, tier, provenance_kind, source_id, source_context) so the response is self-contained.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "field": {"type": "string", "description": "Filter by metadata field name (exact, case-sensitive)"},
                "source_file": {"type": "string", "description": "Filter by source config file (exact, case-sensitive)"}
            },
            "required": ["manifest_path"]
        }
    })
}

fn render_report_tool() -> Value {
    json!({
        "name": "render_report",
        "description": "Render the augmented TrustReport in the requested human-readable format. Use when you need a human-presentable rendering to quote or show. `format` is one of: `markdown` (PR-comment style), `html` (self-contained HTML document), or `mermaid` (just the attestation-graph source).\n\nReturns an envelope {format, content, truncated}. Mermaid output is graph text only (no prose).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "claim_id": {"type": "string"},
                "sidecar": {"type": "string"},
                "last_verified_sidecar": {"type": "string"},
                "format": {"type": "string", "enum": ["markdown", "html", "mermaid"]}
            },
            "required": ["manifest_path", "claim_id", "format"]
        }
    })
}

fn query_concordance_tool() -> Value {
    json!({
        "name": "query_concordance",
        "description": "Query declarative `behavioral_concordance` claims in a manifest. Use when the user asks about claims that compare measured behavior to a prior paper's reported behavior (numeric_band / relative_band / same_order_of_magnitude / ordinal_match / monotone_with). For empirical / measurement claims use list_claims + read_report; for declarative config-file claims use query_metadata.\n\nFilter compose conjunctively (exact, case-sensitive): `pattern_kind` (one of the five primitives). Each result item carries the pattern_kind, paper_locator, prior_binding source_id, and the same audit context list_claims returns (title, tier, provenance_kind, source_id, source_context).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest_path": {"type": "string"},
                "pattern_kind": {"type": "string", "description": "Filter by pattern primitive name (exact, case-sensitive)"}
            },
            "required": ["manifest_path"]
        }
    })
}

codex
The current typed-trust shape confirms a useful constraint: concordance intentionally bypasses the top-level `source`, while metadata claims have their own declaration path and no criteria. That makes the new observation path plausible, but it also exposes a couple of missing rejection cases.
exec
/bin/bash -lc "sed -n '260,620p' typed-trust/src/claim.rs" in /scratch/TMAlign/evident
 succeeded in 0ms:
    /// curator pins down which.
    pub prior_metric_definition: String,
    /// Where in the prior paper the value lives
    /// (e.g. "Meier 2024 Table 3 row 'FragPipe v22 / HLA-I 10k
    /// measured', column 'true_fdr_pct'").
    pub locator: String,
    /// Curator's audit trail: who extracted, when, what version
    /// of the prior they read, what edge-case checks they ran
    /// (caption confirms units, supplementary figure cross-check,
    /// etc.).
    pub prior_extraction_note: String,
    /// The cited artifact. Either DOI, arXiv id, or another
    /// `source_id` token (analogous to manifest provenance).
    pub source_id: String,
}

/// PR5c: typed lift of the manifest's `metadata:` block. The
/// declaration IS the evidence — the source's
/// `pyproject.toml`/`Cargo.toml`/`package.json` stated this value,
/// no synthesis required.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MetadataDeclaration {
    /// Semantic name of the field (e.g. `rust_msrv`,
    /// `python_version_requirement`).
    pub field: String,
    /// Literal value the source declares (e.g. `">=3.10"`,
    /// `"1.67"`).
    pub declared_value: String,
    /// Config file the declaration came from
    /// (e.g. `"Cargo.toml"`).
    pub source_file: String,
    /// Path within the config file
    /// (e.g. `"package.rust-version"`).
    pub source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ClaimKind {
    Performance,
    Comparison,
    Causal,
    Existence,
    Reproducibility,
    Provenance,
    /// PR5b: declarative claim about a configuration field
    /// (e.g. ``requires-python = ">=3.10"`` in pyproject.toml).
    /// Not an empirical measurement — the declaration IS the
    /// evidence. Synthesizer emits a metadata-flavored
    /// TrustReport without empirical Criteria.
    MetadataCompatibility,
    /// PR5f: behavioral concordance — paper claims its measured
    /// behavior tracks a prior paper's reported behavior, under
    /// a framework-owned relationship pattern (numeric_band,
    /// relative_band, same_order_of_magnitude, ordinal_match,
    /// monotone_with). The curator authors `prior_binding`; the
    /// docker artifact carries the measured value; the framework
    /// comparator decides. Synthesizer emits a concordance-
    /// flavored TrustReport whose status reflects the comparator's
    /// pass/fail/not-assessed verdict.
    BehavioralConcordance,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Assumption {
    pub text: String,
    pub load_bearing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceSpan {
    pub path: String,
    pub span: String,
}

exec
/bin/bash -lc "sed -n '1580,1880p' typed-trust/tests/translator.rs" in /scratch/TMAlign/evident
 succeeded in 0ms:
        "metadata claim should have empty criteria, got {criteria:?}",
    );
}

#[test]
fn metadata_claim_emits_no_evidence() {
    let yaml = r#"
claims:
  - id: pkg-rust
    title: package requires Rust MSRV 1.67
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: pkg requires Rust MSRV 1.67
    metadata:
      field: rust_msrv
      declared_value: "1.67"
      source_file: Cargo.toml
      source_path: package.rust-version
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let criteria = translate_tolerances(mc).unwrap();
    let evidence = translate_evidence(&ctx, mc, &criteria).unwrap();
    assert!(
        evidence.is_none(),
        "metadata claim should produce no Evidence, got Some(_)",
    );
}

#[test]
fn metadata_claim_rejects_unknown_field_in_block() {
    let yaml = r#"
claims:
  - id: bad-extra-field
    title: extra field in metadata
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: bad
    metadata:
      field: x
      declared_value: "1"
      source_file: pyproject.toml
      source_path: x
      unknown_field: this should be rejected
"#;
    let err = parse_manifest_file(yaml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown_field") || msg.contains("unknown field"),
        "expected error mentioning the unknown field, got: {msg}",
    );
}

#[test]
fn metadata_claim_with_evidence_block_is_rejected() {
    // Codex F-PR5b-CR1 (P2): the disjointness rule must reject
    // `evidence:` on a metadata claim. The declaration IS the
    // evidence; carrying a command would be misleading.
    let yaml = r#"
claims:
  - id: bad-meta-evidence
    title: metadata claim with evidence
    kind: metadata_compatibility
    tier: research
    source: ..
    claim: bad
    metadata:
      field: x
      declared_value: "1"
      source_file: pyproject.toml
      source_path: x
    evidence:
      oracle: [Manual]
      command: echo no
      artifact: out.txt
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::MetadataClaimCarriesEvidence { .. }),
        "expected MetadataClaimCarriesEvidence, got {err:?}",
    );
}

// ----------------------------------------------------------------------
// PR5f: behavioral_concordance translator + Manifest deserialization
// ----------------------------------------------------------------------

#[test]
fn behavioral_concordance_numeric_band_translates_with_full_block() {
    let yaml = r#"
claims:
  - id: rustims-fragpipe-fdr-10k-concords-meier
    title: FragPipe FDR on rustims-simulated HLA-I 10k tracks Meier 2024
    kind: behavioral_concordance
    tier: research
    claim: |
      FragPipe v22's empirical true FDR on rustims-simulated HLA-I 10k
      lies within 0.5 pp of Meier 2024's measured value.
    concordance:
      pattern:
        pattern_kind: numeric_band
        metric_path: fragpipe.hla_10k.fdr_pct
        epsilon: 0.5
        prior_value: 1.5
      paper_locator: source/cited.md#rustims-fragpipe-fdr-10k
      prior_binding:
        prior_unit: percentage_points
        prior_metric_definition: |
          Empirical true FDR after target-decoy q<=0.01 filter.
        locator: "Meier 2024 Table 3 row 'FragPipe v22 HLA-I 10k measured'"
        prior_extraction_note: "Curator verified Table 3 print version 2026-XX"
        source_id: "doi:10.1038/PLACEHOLDER"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let attested = translate_claim(&ctx, mc, "claims[0]").unwrap();
    assert_eq!(attested.value.kind, ClaimKind::BehavioralConcordance);
    let cd = attested.value.concordance.as_ref().expect("concordance present");
    match &cd.pattern {
        typed_trust::ConcordancePattern::NumericBand {
            metric_path,
            epsilon,
            prior_value,
        } => {
            assert_eq!(metric_path, "fragpipe.hla_10k.fdr_pct");
            assert_eq!(*epsilon, 0.5);
            assert_eq!(*prior_value, 1.5);
        }
        other => panic!("expected NumericBand, got {other:?}"),
    }
    assert_eq!(cd.paper_locator, "source/cited.md#rustims-fragpipe-fdr-10k");
    assert_eq!(cd.prior_binding.prior_unit, "percentage_points");
    assert_eq!(cd.prior_binding.source_id, "doi:10.1038/PLACEHOLDER");
}

#[test]
fn behavioral_concordance_ordinal_match_keyset_alignment_enforced() {
    // entity_to_path has key "FragPipe_v22" but prior_value has
    // "FragPipe_v23" — keyset mismatch must be rejected.
    let yaml = r#"
claims:
  - id: rustims-tools-fdr-ordering-concords-meier
    title: Tool FDR ordering on rustims-simulated HLA-I 10k
    kind: behavioral_concordance
    tier: research
    claim: |
      Tool ordering by FDR on rustims-simulated data matches Meier 2024.
    concordance:
      pattern:
        pattern_kind: ordinal_match
        entity_to_path:
          FragPipe_v22: fragpipe_v22.hla_10k.fdr_pct
          PEAKS_XPro: peaks_xpro.hla_10k.fdr_pct
        direction: lower_is_better
        tie_policy: adjacent_swap_ok
        prior_value:
          FragPipe_v23: 1.5
          PEAKS_XPro: 1.8
      paper_locator: source/cited.md#rustims-fdr-ordering
      prior_binding:
        prior_unit: percentage_points
        prior_metric_definition: "Empirical true FDR per Meier 2024 §Methods."
        locator: "Meier 2024 Table 3 across two tool rows"
        prior_extraction_note: "Curator verified ordering"
        source_id: "doi:10.1038/PLACEHOLDER"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceOrdinalKeyMismatch { .. }),
        "expected ConcordanceOrdinalKeyMismatch, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_same_order_rejects_non_positive_prior() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    claim: c
    concordance:
      pattern:
        pattern_kind: same_order_of_magnitude
        metric_path: foo.bar
        prior_value: 0.0
        zero_policy: not_assessed
      paper_locator: src.md
      prior_binding:
        prior_unit: count
        prior_metric_definition: "x"
        locator: "x"
        prior_extraction_note: "x"
        source_id: "x"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceSameOrderNonPositivePrior { .. }),
        "expected ConcordanceSameOrderNonPositivePrior, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_relative_band_rejects_ratio_at_or_below_one() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    claim: c
    concordance:
      pattern:
        pattern_kind: relative_band
        metric_path: foo.bar
        ratio: 1.0
        prior_value: 10.0
      paper_locator: src.md
      prior_binding:
        prior_unit: ms
        prior_metric_definition: "runtime"
        locator: "x"
        prior_extraction_note: "x"
        source_id: "x"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceRelativeBandRatioTooSmall { .. }),
        "expected ConcordanceRelativeBandRatioTooSmall, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_rejects_top_level_source() {
    // Concordance claims must NOT carry the measurement-flavored
    // `source` field (v4 design's schema-exception commitment).
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    source: src.md
    claim: c
    concordance:
      pattern:
        pattern_kind: numeric_band
        metric_path: foo.bar
        epsilon: 0.1
        prior_value: 1.0
      paper_locator: src.md
      prior_binding:
        prior_unit: x
        prior_metric_definition: "x"
        locator: "x"
        prior_extraction_note: "x"
        source_id: "x"
"#;
    let manifest = parse_manifest_file(yaml).unwrap();
    let mc = &manifest.claims[0];
    let ctx = ctx("any.yaml");
    let err = translate_claim(&ctx, mc, "claims[0]").unwrap_err();
    assert!(
        matches!(err, TranslateError::ConcordanceClaimCarriesSource { .. }),
        "expected ConcordanceClaimCarriesSource, got {err:?}",
    );
}

#[test]
fn behavioral_concordance_rejects_oracle_in_evidence() {
    let yaml = r#"
claims:
  - id: bad
    title: bad
    kind: behavioral_concordance
    tier: research
    claim: c
    concordance:
      pattern:
        pattern_kind: numeric_band
        metric_path: foo.bar
        epsilon: 0.1
        prior_value: 1.0
      paper_locator: src.md

exec
/bin/bash -lc "sed -n '380,860p' typed-trust/src/translate.rs" in /scratch/TMAlign/evident
 succeeded in 0ms:
    pub artifact: String,
    /// Phase 5: optional replay-path status. Defaults to
    /// `not_attempted` when absent, preserving the meaning of
    /// hand-authored manifests pre-Phase-5.
    #[serde(default)]
    pub replay_status: Option<String>,
    /// Phase 5: optional structured reason a replay is
    /// unavailable. Pair-validator in `translate_evidence` enforces
    /// the legal `(replay_status, replay_reason)` combinations.
    #[serde(default)]
    pub replay_reason: Option<String>,
}

// ---------- Translation context, errors ----------

#[derive(Debug, Clone)]
pub struct TranslationContext {
    /// Time at which the translation is performed; goes into
    /// `Attested.at`.
    pub now: Timestamp,
    /// Source manifest path; goes into `Claim.source.path`.
    pub manifest_path: String,
}

#[derive(Debug)]
pub enum TranslateError {
    Yaml(String),
    /// Policy/reference claims are out of typed-trust scope (§0).
    OutOfScope { id: String, kind: String },
    /// An unknown comparison operator in `tolerances[].op`.
    UnknownOp { id: String, op: String },
    /// A tolerance entry has some but not all of `metric`/`op`/`value`.
    /// The shipping schema requires all three together (structured) or
    /// none (prose-only); mixing them is a manifest error.
    PartialTolerance { id: String },
    /// A prose-only tolerance (metric/op/value all absent) appeared at
    /// a non-research tier. The shipping schema frames prose-only as
    /// the research-tier deferred-spec escape hatch only — CI and
    /// release claims must carry structured tolerances.
    ProseOnlyOutsideResearch { id: String, tier: String },
    /// A `kind: measurement` claim omitted `tolerances` or provided
    /// an empty list. The shipping schema requires non-empty
    /// tolerances on measurement claims; without them the
    /// synthesizer would emit a Current report with nothing
    /// assessed.
    MeasurementWithoutTolerances { id: String },
    /// A `kind: measurement` claim omitted the `evidence` block. The
    /// shipping schema requires evidence on measurement claims;
    /// without it the report would render Current with NotAssessed
    /// criteria — an unevidenced measurement looking accepted.
    MeasurementWithoutEvidence { id: String },
    /// Phase 5: the `evidence.replay_status` field was not one of
    /// `available | not_attempted | unavailable_artifacts`.
    InvalidReplayStatus { id: String, value: String },
    /// Phase 5: the `evidence.replay_reason` field was not one of the
    /// ten known reason strings.
    InvalidReplayReason { id: String, value: String },
    /// Phase 5: the `(replay_status, replay_reason)` pair is not in
    /// the legal set. Legal combinations:
    ///   (available, None), (not_attempted, None),
    ///   (unavailable_artifacts, Some(_))
    IllegalReplayPair {
        id: String,
        status: String,
        reason: Option<String>,
    },
    /// PR5b: `kind: metadata_compatibility` claim missing the
    /// required `metadata` block.
    MetadataClaimMissingBlock { id: String },
    /// PR5b: metadata claims must NOT carry tolerances; they're
    /// declarative not empirical.
    MetadataClaimCarriesTolerances { id: String },
    /// PR5b: metadata claims must NOT carry an `evidence` block;
    /// the declaration IS the evidence (codex F-PR5b-CR1 P2).
    MetadataClaimCarriesEvidence { id: String },
    /// PR5b: measurement claims must NOT carry a metadata block.
    MeasurementClaimCarriesMetadata { id: String },
    /// PR5f: `kind: behavioral_concordance` claim missing the
    /// required `concordance` block.
    ConcordanceClaimMissingBlock { id: String },
    /// PR5f: concordance claims must NOT carry tolerances; the
    /// pattern primitive IS the bound.
    ConcordanceClaimCarriesTolerances { id: String },
    /// PR5f: concordance claims must NOT carry the
    /// measurement-flavored top-level `source` field; they carry
    /// `concordance.paper_locator` instead. v4 design's
    /// `paper_locator`-is-a-schema-exception commitment.
    ConcordanceClaimCarriesSource { id: String },
    /// PR5f: concordance evidence must NOT carry an `oracle`
    /// list. The pattern primitive IS the oracle.
    ConcordanceClaimCarriesOracle { id: String },
    /// PR5f: `ordinal_match` pattern requires that `prior_value`'s
    /// per-entity keyset exactly equals `entity_to_path`'s keyset.
    /// Codex v3 follow-up: keep the two structurally aligned at
    /// translate time so the comparator can dispatch unambiguously.
    ConcordanceOrdinalKeyMismatch { id: String },
    /// PR5f: `same_order_of_magnitude` requires a strictly
    /// positive `prior_value`. Non-positive prior is a curator
    /// authoring error caught at translate time, not at replay.
    ConcordanceSameOrderNonPositivePrior { id: String },
    /// PR5f: `relative_band` requires `ratio > 1.0`. A ratio of
    /// `1.0` would make the band a single point; a ratio of `<1.0`
    /// would invert the interpretation.
    ConcordanceRelativeBandRatioTooSmall { id: String },
    /// PR5f: measurement / metadata_compatibility claims must NOT
    /// carry a concordance block. Keeps the kinds disjoint.
    NonConcordanceClaimCarriesConcordance { id: String },
    /// PR5f: `concordance.pattern.{enum_field}` carried an unknown
    /// enum value (e.g. `direction: "sideways"`,
    /// `tie_policy: "everything_goes"`, `zero_policy: "ignore"`).
    ConcordanceInvalidEnumValue {
        id: String,
        field: String,
        value: String,
    },
}

impl std::fmt::Display for TranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranslateError::Yaml(e) => write!(f, "YAML parse error: {e}"),
            TranslateError::OutOfScope { id, kind } => write!(
                f,
                "claim {id} has kind={kind}, out of typed-trust scope (§0)"
            ),
            TranslateError::UnknownOp { id, op } => {
                write!(f, "claim {id}: unknown comparison op {op:?}")
            }
            TranslateError::PartialTolerance { id } => write!(
                f,
                "claim {id}: tolerance has some but not all of metric/op/value; \
                 shipping schema requires all-or-nothing"
            ),
            TranslateError::ProseOnlyOutsideResearch { id, tier } => write!(
                f,
                "claim {id}: prose-only tolerance not allowed at tier {tier:?}; \
                 prose-only is the research-tier deferred-spec escape hatch only"
            ),
            TranslateError::MeasurementWithoutTolerances { id } => write!(
                f,
                "claim {id}: kind=measurement requires non-empty tolerances; \
                 add tolerances or change to kind: policy / reference"
            ),
            TranslateError::MeasurementWithoutEvidence { id } => write!(
                f,
                "claim {id}: kind=measurement requires an evidence block; \
                 add evidence or change to kind: policy / reference"
            ),
            TranslateError::InvalidReplayStatus { id, value } => write!(
                f,
                "claim {id}: evidence.replay_status {value:?} is not one of \
                 available | not_attempted | unavailable_artifacts"
            ),
            TranslateError::InvalidReplayReason { id, value } => write!(
                f,
                "claim {id}: evidence.replay_reason {value:?} is not a known reason"
            ),
            TranslateError::IllegalReplayPair { id, status, reason } => write!(
                f,
                "claim {id}: illegal (replay_status, replay_reason) pair \
                 ({status:?}, {reason:?}); legal pairs are (available, null), \
                 (not_attempted, null), (unavailable_artifacts, <reason>)"
            ),
            TranslateError::MetadataClaimMissingBlock { id } => write!(
                f,
                "claim {id}: kind=metadata_compatibility requires a \
                 `metadata` block with field/declared_value/source_file/\
                 source_path"
            ),
            TranslateError::MetadataClaimCarriesTolerances { id } => write!(
                f,
                "claim {id}: kind=metadata_compatibility must NOT carry \
                 tolerances; metadata is declarative, not empirical"
            ),
            TranslateError::MetadataClaimCarriesEvidence { id } => write!(
                f,
                "claim {id}: kind=metadata_compatibility must NOT carry \
                 an `evidence` block; the declaration IS the evidence"
            ),
            TranslateError::MeasurementClaimCarriesMetadata { id } => write!(
                f,
                "claim {id}: kind=measurement must NOT carry a `metadata` \
                 block; metadata belongs only to metadata_compatibility claims"
            ),
            TranslateError::ConcordanceClaimMissingBlock { id } => write!(
                f,
                "claim {id}: kind=behavioral_concordance requires a \
                 `concordance` block (pattern_kind + paper_locator + \
                 prior_binding)"
            ),
            TranslateError::ConcordanceClaimCarriesTolerances { id } => write!(
                f,
                "claim {id}: kind=behavioral_concordance must NOT carry \
                 `tolerances`; the pattern primitive is the bound"
            ),
            TranslateError::ConcordanceClaimCarriesSource { id } => write!(
                f,
                "claim {id}: kind=behavioral_concordance must NOT carry \
                 the top-level `source` field; use \
                 `concordance.paper_locator` instead (v4 design's \
                 schema-exception commitment)"
            ),
            TranslateError::ConcordanceClaimCarriesOracle { id } => write!(
                f,
                "claim {id}: kind=behavioral_concordance evidence must \
                 NOT carry an `oracle` list; the pattern primitive \
                 (numeric_band, ordinal_match, etc.) IS the oracle"
            ),
            TranslateError::ConcordanceOrdinalKeyMismatch { id } => write!(
                f,
                "claim {id}: ordinal_match concordance requires \
                 `prior_value`'s per-entity keyset to exactly equal \
                 `entity_to_path`'s keyset"
            ),
            TranslateError::ConcordanceSameOrderNonPositivePrior { id } => write!(
                f,
                "claim {id}: same_order_of_magnitude requires a strictly \
                 positive `prior_value` (log10 is undefined at zero \
                 and semantically wrong for signed quantities)"
            ),
            TranslateError::ConcordanceRelativeBandRatioTooSmall { id } => write!(
                f,
                "claim {id}: relative_band requires `ratio > 1.0` \
                 (a ratio of 1.0 collapses the band to a point; \
                 a ratio < 1.0 inverts the bound interpretation)"
            ),
            TranslateError::NonConcordanceClaimCarriesConcordance { id } => write!(
                f,
                "claim {id}: only kind=behavioral_concordance may carry \
                 a `concordance` block"
            ),
            TranslateError::ConcordanceInvalidEnumValue {
                id,
                field,
                value,
            } => write!(
                f,
                "claim {id}: concordance.{field} carries unknown value \
                 {value:?}; see EVIDENT_BEHAVIORAL_CONCORDANCE_DRAFT.md \
                 for the legal set"
            ),
        }
    }
}

impl std::error::Error for TranslateError {}

// ---------- Translation ----------

/// Parse a YAML manifest into its structured form.
pub fn parse_manifest_file(yaml: &str) -> Result<ManifestFile, TranslateError> {
    serde_yaml_ng::from_str(yaml).map_err(|e| TranslateError::Yaml(e.to_string()))
}

/// Translate a single manifest claim into an [`Attested<Claim>`]. The
/// extraction is `Derivation::Verified` because the projection is
/// deterministic (per §4 footnote).
///
/// `span` is the YAML location of this claim within its file (e.g.
/// `"claims[0]"`); goes into [`SourceSpan`].
pub fn translate_claim(
    ctx: &TranslationContext,
    mc: &ManifestClaim,
    span: &str,
) -> Result<Attested<Claim>, TranslateError> {
    // §0 scope: measurement claims (empirical), metadata_compatibility
    // claims (PR5b — declarative configuration claims), or
    // behavioral_concordance claims (PR5f — paper measured-behavior
    // tracks a prior paper's reported behavior).
    if mc.kind != "measurement"
        && mc.kind != "metadata_compatibility"
        && mc.kind != "behavioral_concordance"
    {
        return Err(TranslateError::OutOfScope {
            id: mc.id.clone(),
            kind: mc.kind.clone(),
        });
    }

    // PR5b: metadata_compatibility claims require the `metadata`
    // block (field/declared_value/source_file/source_path) and must
    // NOT carry tolerances/evidence — those belong to the empirical
    // path.
    if mc.kind == "metadata_compatibility" {
        if mc.metadata.is_none() {
            return Err(TranslateError::MetadataClaimMissingBlock {
                id: mc.id.clone(),
            });
        }
        if mc.tolerances.is_some() {
            return Err(TranslateError::MetadataClaimCarriesTolerances {
                id: mc.id.clone(),
            });
        }
        // Codex F-PR5b-CR1 (P2): also reject `evidence:` on a
        // metadata claim. The two paths are disjoint; the
        // declaration IS the evidence.
        if mc.evidence.is_some() {
            return Err(TranslateError::MetadataClaimCarriesEvidence {
                id: mc.id.clone(),
            });
        }
    } else if mc.metadata.is_some() {
        // A measurement OR concordance claim that accidentally
        // carries a metadata block is rejected — keeps the paths
        // disjoint.
        return Err(TranslateError::MeasurementClaimCarriesMetadata {
            id: mc.id.clone(),
        });
    }

    // PR5f: behavioral_concordance claims require the `concordance`
    // block and must NOT carry `tolerances` (the comparator
    // primitive IS the bound) or the `oracle` list inside evidence
    // (the comparator IS the oracle). They DO carry `evidence`
    // for the docker contract (docker_image, command, artifact).
    if mc.kind == "behavioral_concordance" {
        if mc.concordance.is_none() {
            return Err(TranslateError::ConcordanceClaimMissingBlock {
                id: mc.id.clone(),
            });
        }
        if mc.tolerances.is_some() {
            return Err(TranslateError::ConcordanceClaimCarriesTolerances {
                id: mc.id.clone(),
            });
        }
        // Concordance claims don't use the measurement-flavored
        // top-level `source` field — they carry
        // `concordance.paper_locator` instead. v4 design's
        // "paper_locator is a schema exception" commitment.
        if mc.source.is_some() {
            return Err(TranslateError::ConcordanceClaimCarriesSource {
                id: mc.id.clone(),
            });
        }
        // The `oracle` list is a measurement-evidence concept;
        // for concordance the comparator (pattern_kind) IS the
        // oracle. Reject to make the disjointness load-bearing.
        if let Some(ev) = mc.evidence.as_ref() {
            if !ev.oracle.is_empty() {
                return Err(TranslateError::ConcordanceClaimCarriesOracle {
                    id: mc.id.clone(),
                });
            }
        }
        // Validate the OrdinalMatch keyset invariant: the prior's
        // per-entity map keyset MUST equal entity_to_path's
        // keyset. v4 design commitment; codex v3 finding.
        if let Some(cb) = mc.concordance.as_ref() {
            if let ManifestConcordancePattern::OrdinalMatch {
                entity_to_path,
                prior_value,
                ..
            } = &cb.pattern
            {
                let path_keys: std::collections::BTreeSet<&String> =
                    entity_to_path.keys().collect();
                let prior_keys: std::collections::BTreeSet<&String> =
                    prior_value.keys().collect();
                if path_keys != prior_keys {
                    return Err(TranslateError::ConcordanceOrdinalKeyMismatch {
                        id: mc.id.clone(),
                    });
                }
            }
            // SameOrderOfMagnitude: prior_value > 0 is a curator
            // authoring invariant per v4 design.
            if let ManifestConcordancePattern::SameOrderOfMagnitude { prior_value, .. } =
                &cb.pattern
            {
                if *prior_value <= 0.0 {
                    return Err(TranslateError::ConcordanceSameOrderNonPositivePrior {
                        id: mc.id.clone(),
                    });
                }
            }
            // RelativeBand: ratio > 1.0 per v4 design.
            if let ManifestConcordancePattern::RelativeBand { ratio, .. } = &cb.pattern {
                if *ratio <= 1.0 {
                    return Err(TranslateError::ConcordanceRelativeBandRatioTooSmall {
                        id: mc.id.clone(),
                    });
                }
            }
        }
    } else if mc.concordance.is_some() {
        // A measurement or metadata_compatibility claim that
        // accidentally carries a concordance block is rejected.
        return Err(TranslateError::NonConcordanceClaimCarriesConcordance {
            id: mc.id.clone(),
        });
    }

    let kind = if mc.kind == "metadata_compatibility" {
        ClaimKind::MetadataCompatibility
    } else if mc.kind == "behavioral_concordance" {
        ClaimKind::BehavioralConcordance
    } else {
        infer_kind(mc)
    };

    let metadata = mc.metadata.as_ref().map(|m| MetadataDeclaration {
        field: m.field.clone(),
        declared_value: m.declared_value.clone(),
        source_file: m.source_file.clone(),
        source_path: m.source_path.clone(),
    });

    let concordance = mc.concordance.as_ref().map(translate_concordance_block).transpose()?;

    let claim = Claim {
        id: ClaimId::new(&mc.id),
        text: mc.claim.trim().to_string(),
        kind,
        source: SourceSpan {
            path: ctx.manifest_path.clone(),
            span: span.into(),
        },
        explicit: true,
        decomposes_into: vec![],
        // TODO: translate `assumptions` into Vec<Attested<Assumption>>.
        // Each assumption becomes a Judged attestation by the manifest
        // author. Requires either reviewer identity or the degraded
        // `unspecified_human_from_manifest` form.
        requires_assumptions: vec![],
        metadata,
        concordance,
    };

    let derivation = Derivation::Verified {
        method: ToolInvocation {
            command: format!("typed-trust translate {}", ctx.manifest_path),
            tool_version: env!("CARGO_PKG_VERSION").into(),
            env: vec![],
        },
        ran_by: translator_identity(),
        reruns: vec![],
    };

    Ok(Attested {
        value: claim,
        derivation,
        at: ctx.now.clone(),
    })
}

/// A criterion id paired with its tolerance, ready to be lifted into a
/// [`Criterion`] once synthesis decides a result. The id is generated
/// at translate time so [`MetricObservation`] in a [`Rerun`] can bind
/// to it deterministically.
///
/// `tolerance` is `None` when the manifest tolerance is prose-only
/// (research-tier deferred-spec — `metric`/`op`/`value` all absent,
/// only `prose` carried). Synthesis treats such criteria as
/// `NotAssessed { reason: "no structured tolerance ..." }`. The
/// `prose` text is preserved on the Criterion via its name.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslatedCriterion {
    pub id: CriterionId,
    pub tolerance: Option<Tolerance>,
    /// Always present — `prose` is required by the shipping schema
    /// even when the structured triple is absent.
    pub prose: String,
}

/// PR5f: lift the manifest's `concordance` block onto the typed
/// `ConcordanceDeclaration`. Parses the enum-valued fields
/// (`direction`, `tie_policy`, `zero_policy`) into their typed
/// counterparts and emits a structured `TranslateError` if an
/// unknown value is encountered.
fn translate_concordance_block(
    mb: &ManifestConcordanceBlock,
) -> Result<crate::claim::ConcordanceDeclaration, TranslateError> {
    use crate::claim::{
        ConcordanceDeclaration, ConcordancePattern, MonotoneDirection, PriorBindingContext,
        RankingDirection, TiePolicy, ZeroPolicy,
    };

    fn parse_zero_policy(s: &str) -> Option<ZeroPolicy> {
        match s {

exec
/bin/bash -lc "sed -n '1,220p' experiments/phase5-extraction-rate/extracted/paper-rustims-main/evident.yaml && sed -n '1,220p' experiments/phase5-extraction-rate/extracted/paper-rustims-main/raw_extraction.json" in /scratch/TMAlign/evident
 succeeded in 0ms:
version: '0.1'
project: extracted/preprint-rustims-v1-covered-6dadf069
claims:
- id: rustims-maxquant-peak-matching-error-7p5min
  title: MaxQuant peak matching error reaches up to 30% in 7.5 min, 150,000-peptide
    setup
  kind: measurement
  tier: research
  source: source/cited.md
  case: source/cited.md#rustims-maxquant-peak-matching-error-7p5min
  claim: MaxQuant's peak matching error rate rose sharply, reaching up to 30% in the
    7.5 min, 150,000-peptide setup.
  tolerances:
  - metric: peak_matching_error_rate
    op: <=
    value: 30
    prose: 'In the 7.5-minute gradient with 150,000 peptides setup, MaxQuant''s peak
      matching error rate reached up to 30%.

      source_span: "MaxQuant''s peak matching error rate rose sharply, reaching up
      to 30% in the 7.5 min, 150,000-peptide setup"'
  evidence:
    oracle:
    - Paper-Authority
    command: no-replay-path
    artifact: source/cited.md#rustims-maxquant-peak-matching-error-7p5min
    replay_status: unavailable_artifacts
    replay_reason: code_private
  provenance:
    kind: extracted-from-paper
    source_id: preprint:rustims-v1-covered-6dadf069
    source_sha: bc94017677e1a73da65ed66daee287d74f4188667fa67f411f64bd52496f3b21
    source_context: unknown
    extractor:
      model: claude-opus-4-7
      extracted_at: '2026-06-03T13:52:39Z'
    curator: null
  last_verified:
    commit: null
    date: null
    value: null
    corpus_sha: null
{
  "schema_version": "1.0",
  "purpose": "Raw model tool_input from the extraction call, BEFORE validator filtering or curator review. Auditors read this to see what the model proposed. NOT a promotable manifest.",
  "model": "claude-opus-4-7",
  "extracted_at": "2026-06-03T13:52:39Z",
  "source_id": "preprint:rustims-v1-covered-6dadf069",
  "source_sha": "bc94017677e1a73da65ed66daee287d74f4188667fa67f411f64bd52496f3b21",
  "tool_input": {
    "claims": [
      {
        "id": "rustims-maxquant-peak-matching-error-7p5min",
        "title": "MaxQuant peak matching error reaches up to 30% in 7.5 min, 150,000-peptide setup",
        "claim": "MaxQuant's peak matching error rate rose sharply, reaching up to 30% in the 7.5 min, 150,000-peptide setup.",
        "subject_aliases": [
          "MaxQuant"
        ],
        "tolerances": [
          {
            "metric": "peak_matching_error_rate",
            "op": "<=",
            "value": 30,
            "source_span": "MaxQuant's peak matching error rate rose sharply, reaching up to 30% in the 7.5 min, 150,000-peptide setup",
            "prose": "In the 7.5-minute gradient with 150,000 peptides setup, MaxQuant's peak matching error rate reached up to 30%."
          }
        ]
      },
      {
        "id": "rustims-peaks-xpro-fdr-10k",
        "title": "PEAKS-XPro real FDR on 10k HLA-I dataset",
        "claim": "PEAKS-XPro reported real FDRs of 1.8% (10k) and 1.15% (100k).",
        "subject_aliases": [
          "PEAKS-XPro"
        ],
        "tolerances": [
          {
            "metric": "real_FDR_10k_HLA",
            "op": "<=",
            "value": 1.8,
            "source_span": "PEAKS-XPro reported real FDRs of 1.8% (10k) and 1.15% (100k)",
            "prose": "PEAKS-XPro reported a real FDR of 1.8% on the 10,000-peptide HLA-I dataset."
          },
          {
            "metric": "real_FDR_100k_HLA",
            "op": "<=",
            "value": 1.15,
            "source_span": "PEAKS-XPro reported real FDRs of 1.8% (10k) and 1.15% (100k)",
            "prose": "PEAKS-XPro reported a real FDR of 1.15% on the 100,000-peptide HLA-I dataset."
          }
        ]
      },
      {
        "id": "rustims-fragpipe-fdr-hla",
        "title": "FragPipe real FDR on HLA-I datasets",
        "claim": "FragPipe achieved 0.91% and 1.16% on the same datasets (10k and 100k HLA-I).",
        "subject_aliases": [
          "FragPipe"
        ],
        "tolerances": [
          {
            "metric": "real_FDR_10k_HLA",
            "op": "<=",
            "value": 0.91,
            "source_span": "FragPipe achieved 0.91% and 1.16% on the same datasets",
            "prose": "FragPipe achieved a real FDR of 0.91% on the 10,000-peptide HLA-I dataset."
          },
          {
            "metric": "real_FDR_100k_HLA",
            "op": "<=",
            "value": 1.16,
            "source_span": "FragPipe achieved 0.91% and 1.16% on the same datasets",
            "prose": "FragPipe achieved a real FDR of 1.16% on the 100,000-peptide HLA-I dataset."
          }
        ]
      },
      {
        "id": "rustims-fragpipe-hla-10k-identification",
        "title": "FragPipe identification rate in HLA-I 10k dataset",
        "claim": "Of the 9,825 fragmented peptides in the 10k dataset, FragPipe identified 3,617 (36.8%).",
        "subject_aliases": [
          "FragPipe"
        ],
        "tolerances": [
          {
            "metric": "fraction_of_fragmented_peptides_identified_10k",
            "op": "<=",
            "value": 36.8,
            "source_span": "Of the 9,825 fragmented peptides in the 10k dataset, FragPipe identified 3,617 (36.8%)",
            "prose": "FragPipe identified 36.8% (3,617 of 9,825) of fragmented peptides in the 10k HLA-I dataset."
          }
        ]
      },
      {
        "id": "rustims-peaks-xpro-hla-10k-identification",
        "title": "PEAKS-XPro identification rate in HLA-I 10k dataset",
        "claim": "PEAKS-XPro identified 3,233 (32.9%) of the 9,825 fragmented peptides in the 10k dataset.",
        "subject_aliases": [
          "PEAKS-XPro"
        ],
        "tolerances": [
          {
            "metric": "fraction_of_fragmented_peptides_identified_10k",
            "op": "<=",
            "value": 32.9,
            "source_span": "PEAKS-XPro 3,233 (32.9%)",
            "prose": "PEAKS-XPro identified 32.9% (3,233 of 9,825) of fragmented peptides in the 10k HLA-I dataset."
          }
        ]
      },
      {
        "id": "rustims-fragpipe-hla-100k-identification",
        "title": "FragPipe identification rate in HLA-I 100k dataset",
        "claim": "In the 100k dataset, 92,047 peptides were fragmented, of which FragPipe recovered 28,108 (30.5%).",
        "subject_aliases": [
          "FragPipe"
        ],
        "tolerances": [
          {
            "metric": "fraction_of_fragmented_peptides_identified_100k",
            "op": "<=",
            "value": 30.5,
            "source_span": "FragPipe recovered 28,108 (30.5%)",
            "prose": "FragPipe recovered 30.5% (28,108 of 92,047) of fragmented peptides in the 100k HLA-I dataset."
          }
        ]
      },
      {
        "id": "rustims-peaks-xpro-hla-100k-identification",
        "title": "PEAKS-XPro identification rate in HLA-I 100k dataset",
        "claim": "PEAKS-XPro recovered 27,182 (29.5%) of the 92,047 fragmented peptides in the 100k dataset.",
        "subject_aliases": [
          "PEAKS-XPro"
        ],
        "tolerances": [
          {
            "metric": "fraction_of_fragmented_peptides_identified_100k",
            "op": "<=",
            "value": 29.5,
            "source_span": "PEAKS-XPro 27,182 (29.5%)",
            "prose": "PEAKS-XPro recovered 29.5% (27,182 of 92,047) of fragmented peptides in the 100k HLA-I dataset."
          }
        ]
      }
    ],
    "rejections": [
      {
        "candidate_text": "several dia-PASEF workflows control FDR near the nominal 1% threshold at stripped-sequence level but exhibit inflated true FDR (3–5%) when modified peptidoforms are considered",
        "locator": "Abstract, page 3",
        "reason": "comparator_bound_to_wrong_subject",
        "rationale": "Subject is generic 'several dia-PASEF workflows', not a specific named tool. The 3-5% range is also not tied to a single subject with a comparator."
      },
      {
        "candidate_text": "match-between-runs produced peak-matching errors of up to 30% under high-density conditions",
        "locator": "Abstract",
        "reason": "metric_not_named",
        "rationale": "Same finding is captured more specifically in body text and attributed to MaxQuant; abstract version is not tool-specific."
      },
      {
        "candidate_text": "identifying a 0.65 site-probability cutoff as an optimal tradeoff between sensitivity and false localization",
        "locator": "Abstract",
        "reason": "bound_not_stated",
        "rationale": "This is a recommended cutoff, not an empirical bound on a measured metric of a system."
      },
      {
        "candidate_text": "DIA-NN v2.0, Spectronaut v20, and both tested FragPipe versions (v22 and v23) achieved a true FDR close to the nominal 1% threshold on ion level",
        "locator": "page 6, ~line 99",
        "reason": "hedged_qualitative_only",
        "rationale": "'Close to' is not a strict comparator with a specific numeric bound."
      },
      {
        "candidate_text": "DIA-NN v1.9 ... converged below 1% at higher complexities",
        "locator": "page 6",
        "reason": "hedged_qualitative_only",
        "rationale": "Conditional on 'higher complexities' (not precisely defined) and no specific measurement value tied to a clear comparator+subject in a way suitable for extraction."
      },
      {
        "candidate_text": "we observed 3-4% for Spectronaut, up to 5% for DIA-NN v1.8, and 1.5-2% for DIA-NN v1.9 and v2.0",
        "locator": "page 6, lines 106-107",
        "reason": "bound_not_stated",
        "rationale": "These are observed ranges rather than stated upper/lower bound comparators in the form required."
      },
      {
        "candidate_text": "FragPipe was the only tool controlling FDR at the expected value of 1%",
        "locator": "page 6",
        "reason": "hedged_qualitative_only",
        "rationale": "Qualitative statement without strict comparator and value bound on a specific metric."
      },
      {
        "candidate_text": "in DIA-NN v1.9 and v2.0, the spurious oxidized form replaced the correct unmodified identification in 99% and 86% of cases",
        "locator": "page 6, line 110",
        "reason": "bound_not_stated",
        "rationale": "Reports point values without a comparator (no '<', '>', 'at least', 'at most' phrasing on these percentages)."
      },
      {
        "candidate_text": "all DIA-NN versions identified the largest number of precursors, up to 10% more than FragPipe",
        "locator": "page 6, line 118",
        "reason": "comparator_bound_to_wrong_subject",
        "rationale": "'Up to 10% more' is a vague upper bound on a difference metric; not a cleanly tied claim of (metric, comparator, value, subject)."
      },
      {
        "candidate_text": "FragPipe reached 90% sensitivity of DIA-NN",
        "locator": "page 6",
        "reason": "bound_not_stated",
        "rationale": "Point value without comparator."
      },
      {
        "candidate_text": "MBR contributed to 10–20% of identifications per run",
        "locator": "page 9, line 224",
        "reason": "bound_not_stated",
        "rationale": "Range value without a strict comparator."
      },
      {
        "candidate_text": "Overall identification errors remained below 1% across both tools",
        "locator": "page 8, line 206",
        "reason": "comparator_bound_to_wrong_subject",
        "rationale": "Subject is 'both tools' generically; not tied to a single named subject for which the validator can match a source_span."
      },
      {
        "candidate_text": "Each tool also contributed 10–15% unique identifications",
        "locator": "page 9",
        "reason": "bound_not_stated",

codex
**Potential bugs**

1. Current Rust `ManifestEvidence` requires `oracle: Vec<String>`. Concordance works only if YAML writes `oracle: []`; “reject non-empty oracle” is not enough. For observation, either add `#[serde(default)] oracle` or make the draft explicitly require `oracle: []`. Otherwise valid-looking observation manifests fail deserialization before translator invariants run.

2. Add disjointness rejects for `last_verified` and probably `case`. Observation uses `last_concorded.json` and `paper_locator`; letting `last_verified` or measurement-style `case` through creates the same ambiguity PR5f removed for `source`.

3. Require an `evidence` block, or explicitly define observation-without-replay as allowed/research-only/not-assessed. The design says replay is load-bearing, but the invariants only reject `evidence.oracle`.

4. Add numeric sanity: finite values, `epsilon >= 0` or preferably `> 0`, finite `prior_value`, finite `ratio`, and maybe `ratio > 1.0` already. YAML/JSON float edge cases can otherwise leak NaN/inf into comparator/render.

**Answers**

1. Reuse is right internally, but `prior_value` is a bad public manifest name for this kind. I would reuse `ConcordancePattern` in Rust/Python, but expose an observation-side manifest wrapper with `observed_value` or `reference_value`, then translate into the internal enum. Keeping `prior_value` verbatim saves code but makes curator docs, rendered output, MCP results, and future audits semantically misleading.

2. Do not factor a shared base yet. A small internal helper for pattern validation is fine, but a shared schema type will blur the review semantics that justify the new kind. `ConcordanceDeclaration` has prior audit fields; `ObservationDeclaration` has tool/metric/replay semantics. Duplication here is cheaper than premature abstraction.

3. Yes, distinct `ClaimKind` is strong enough. The review and promotion semantics are materially different. The corpus-query argument is weaker because nested flags are queryable, but the first two arguments carry it. Also, “concordance with paper_self prior” is conceptually backwards: there is no prior, just a replayable observation.

4. Missing invariants: reject `last_verified`; reject/define `case`; require non-empty `paper_locator`; require evidence presence and replay contract; default or reject missing `evidence.oracle`; require non-empty/known `pattern.metric_path` where present; finite numeric fields; require `third_party_tool` not just non-empty but not equal to the paper’s own system identity if such identity exists. Also, `monotone_with` has no `prior_value`, so draft text saying `prior_value` carries paper’s observed value is not universally true.

5. No hard contradiction with existing schema, but the boundary needs sharper wording. `metadata_compatibility` is for declarative config facts: “package X declares version/range Y,” not empirical behavior. `measurement` can still cover a third-party-named tool only when the paper authors own/frame it as their system. `third_party_observation` is for benchmarked external tools run by the paper. Add this decision rule explicitly.

6. The 7/7 success claim is plausible only after curator splitting/normalization. The raw rustims candidates include multi-value claims like PEAKS-XPro 10k/100k FDR and FragPipe 10k/100k FDR; scalar `numeric_band` cannot represent those as one clean claim unless split or encoded as `ordinal_match`/another structure. “Roughly 7/7 candidates become representable after splitting multi-value candidates” is defensible. “7/7 translate cleanly as-is” is overstated.
tokens used
80,059
**Potential bugs**

1. Current Rust `ManifestEvidence` requires `oracle: Vec<String>`. Concordance works only if YAML writes `oracle: []`; “reject non-empty oracle” is not enough. For observation, either add `#[serde(default)] oracle` or make the draft explicitly require `oracle: []`. Otherwise valid-looking observation manifests fail deserialization before translator invariants run.

2. Add disjointness rejects for `last_verified` and probably `case`. Observation uses `last_concorded.json` and `paper_locator`; letting `last_verified` or measurement-style `case` through creates the same ambiguity PR5f removed for `source`.

3. Require an `evidence` block, or explicitly define observation-without-replay as allowed/research-only/not-assessed. The design says replay is load-bearing, but the invariants only reject `evidence.oracle`.

4. Add numeric sanity: finite values, `epsilon >= 0` or preferably `> 0`, finite `prior_value`, finite `ratio`, and maybe `ratio > 1.0` already. YAML/JSON float edge cases can otherwise leak NaN/inf into comparator/render.

**Answers**

1. Reuse is right internally, but `prior_value` is a bad public manifest name for this kind. I would reuse `ConcordancePattern` in Rust/Python, but expose an observation-side manifest wrapper with `observed_value` or `reference_value`, then translate into the internal enum. Keeping `prior_value` verbatim saves code but makes curator docs, rendered output, MCP results, and future audits semantically misleading.

2. Do not factor a shared base yet. A small internal helper for pattern validation is fine, but a shared schema type will blur the review semantics that justify the new kind. `ConcordanceDeclaration` has prior audit fields; `ObservationDeclaration` has tool/metric/replay semantics. Duplication here is cheaper than premature abstraction.

3. Yes, distinct `ClaimKind` is strong enough. The review and promotion semantics are materially different. The corpus-query argument is weaker because nested flags are queryable, but the first two arguments carry it. Also, “concordance with paper_self prior” is conceptually backwards: there is no prior, just a replayable observation.

4. Missing invariants: reject `last_verified`; reject/define `case`; require non-empty `paper_locator`; require evidence presence and replay contract; default or reject missing `evidence.oracle`; require non-empty/known `pattern.metric_path` where present; finite numeric fields; require `third_party_tool` not just non-empty but not equal to the paper’s own system identity if such identity exists. Also, `monotone_with` has no `prior_value`, so draft text saying `prior_value` carries paper’s observed value is not universally true.

5. No hard contradiction with existing schema, but the boundary needs sharper wording. `metadata_compatibility` is for declarative config facts: “package X declares version/range Y,” not empirical behavior. `measurement` can still cover a third-party-named tool only when the paper authors own/frame it as their system. `third_party_observation` is for benchmarked external tools run by the paper. Add this decision rule explicitly.

6. The 7/7 success claim is plausible only after curator splitting/normalization. The raw rustims candidates include multi-value claims like PEAKS-XPro 10k/100k FDR and FragPipe 10k/100k FDR; scalar `numeric_band` cannot represent those as one clean claim unless split or encoded as `ordinal_match`/another structure. “Roughly 7/7 candidates become representable after splitting multi-value candidates” is defensible. “7/7 translate cleanly as-is” is overstated.
