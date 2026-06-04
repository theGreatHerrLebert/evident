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
session id: 019e91a6-2d50-7403-98de-5c432e98936d
--------
user
Review v2 of this DESIGN DRAFT for a sixth EVIDENT claim kind (third_party_observation).

v1 already had a codex review (see the v2 preamble's 'Substantive changes' list). v2 applied 7 findings:
- Renamed YAML observed_value field (was prior_value)
- Tightened disjointness: reject last_verified, case; require non-empty paper_locator
- Made evidence tier-conditional (research optional, ci/release required)
- Made oracle field optional via serde default
- Added monotone_with carve-out (no observed_value)
- Added boundary decision rule (measurement vs metadata_compatibility vs concordance vs observation)
- Tightened success criteria from '7/7 translate cleanly' to '~10-12 per-cell observations after curator splitting'

Please don't re-flag what v1 already fixed. Focus on:

1. The observed_value rename: I'm keeping prior_value internally (for code reuse with concordance) and translating at the YAML boundary. Is that the right split, or should the internal Rust enum also rename?

2. The tier-conditional evidence rule: does this open a way for a curator to silently promote a non-replayable observation to tier:ci by writing a stub docker contract? Should there be a stronger gate?

3. The monotone_with carve-out: I say 'no observed_value at the schema level — the metric_definition is the observation.' Is that the right framing, or should monotone_with observations carry the expected series shape some other way?

4. The boundary decision rule (4 ordered questions for the curator). Are the 4 questions truly disjoint, or is there an edge case where two would both apply?

5. Anything new that v2 broke (the field rename, the tier-conditional evidence, the monotone carve-out) that I haven't anticipated.

6. Is the success-criteria framing now honest? 'Representable after curator splitting' is the upper bound; what's the realistic lower bound?

Concrete and specific. Flag potential bugs prominently. Cap at 800 words.

<stdin>
# Design — `kind: third_party_observation` (v2 DRAFT)

> v2 incorporates the codex review of v1
> (`EVIDENT_THIRD_PARTY_OBSERVATION.codex-review.md`).
> Substantive changes:
>
> - **YAML field renamed** `prior_value` → `observed_value` on the
>   observation side. Codex flagged that "prior_value" is
>   semantically misleading for this kind (there's no prior — the
>   paper is the source). The internal Rust `ConcordancePattern`
>   enum keeps `prior_value` for code reuse; the translator maps
>   the YAML's `observed_value` into the internal field. Same for
>   render and MCP — surface as `observed_value` externally.
> - **Disjointness invariants tightened**: reject `last_verified`
>   block (observation uses `last_concorded.json`), reject `case`
>   field (observation uses `paper_locator`), require non-empty
>   `paper_locator`, require finite numeric values everywhere
>   (`epsilon > 0`, `ratio > 1.0`, `observed_value` finite).
> - **Evidence is REQUIRED** for non-research tiers. v1 was
>   ambiguous about replay-less observations; v2 says:
>   `tier:research` allows missing evidence (renders as
>   `not_assessed`); `tier:ci` / `tier:release` require evidence
>   with replay contract.
> - **`oracle` field is omitted/empty** for observation evidence.
>   Schema change: `ManifestEvidence.oracle` becomes
>   `#[serde(default)]` so observation manifests don't need to
>   ship `oracle: []`. Translator still rejects non-empty
>   `oracle` for observation.
> - **`monotone_with` carve-out**: no `observed_value` field
>   (the shape IS the observation, captured in
>   `metric_definition`). Explicitly documented.
> - **Boundary decision rule** for `measurement` vs.
>   `metadata_compatibility` vs. `third_party_observation` —
>   added to "Why a distinct kind."
> - **Success-criteria tightened**: "7/7 representable after
>   curator splitting" rather than "7/7 translate cleanly." Codex
>   right: multi-value candidates ("PEAKS-XPro 1.8% / 1.15%")
>   need splitting into per-cell claims.

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
      observed_value: 30.0     # v2: was 'prior_value' in v1; renamed
                               # to reflect that this is the paper's
                               # own observation, not a cited prior.
                               # Internal Rust enum still uses
                               # prior_value for code reuse with
                               # concordance.
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

### Boundary decision rule (codex F-CR5)

When a curator is choosing which kind to author, apply in
order:

1. **Is the bound asserted about the paper's own system?**
   → `measurement`. Even when the metric is named after a
   third-party tool (e.g. the paper authors say "our system
   achieves FDR < 1% using FragPipe's algorithm"), if THEY own
   the claim, it's `measurement`.
2. **Is the claim a declarative fact about a config file?**
   ("`requires-python = ">=3.10"`", `rust-version = "1.70"`)
   → `metadata_compatibility`. Not empirical behavior — the
   declaration IS the evidence.
3. **Is the paper concording with a value from prior
   literature?** ("Our FDR tracks Meier 2024's reported FDR")
   → `behavioral_concordance`. Requires a structured
   `prior_binding` block citing the prior paper.
4. **Is the paper observing a third-party tool's behavior on
   their own data, with no cited prior?** ("MaxQuant's peak
   matching error reaches 30% on our simulated data") →
   `third_party_observation`.

Rule (4) is what this PR adds. Rules (1)–(3) are already
shipped; v2 documents them here to make the boundary explicit.

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

## Translator invariants (codex-pattern from PR5f + v2 codex fixes)

For `kind: third_party_observation`:

1. `observation` block REQUIRED — without it,
   `ObservationClaimMissingBlock` error.
2. `observation.third_party_tool` REQUIRED non-empty.
3. `observation.metric_definition` REQUIRED non-empty.
4. `observation.paper_locator` REQUIRED non-empty (v2 codex
   F-CR4).
5. Reject `tolerances` (the pattern primitive IS the bound).
6. Reject top-level `source` field (use
   `observation.paper_locator`).
7. Reject `case` field (v2 codex F-CR4 — observation uses
   `paper_locator`).
8. Reject `last_verified` block (v2 codex F-CR2 — observation
   uses `last_concorded.json`).
9. Reject `metadata` block (path disjoint).
10. Reject `concordance` block (path disjoint).
11. Reject non-empty `evidence.oracle` (the pattern primitive IS
    the oracle). v2 change: `ManifestEvidence.oracle` becomes
    `#[serde(default)]` so observation manifests can omit the
    field entirely.
12. Evidence presence by tier:
    - `tier:research`: evidence is OPTIONAL. Missing evidence
      renders as `replay_status: not_attempted` and the
      claim's status is `NotAssessed`.
    - `tier:ci` / `tier:release`: evidence is REQUIRED (docker
      contract present), and `replay_status` MUST be
      `available`. Codex F-CR3.
13. Apply the same `ConcordancePattern`-level validation:
    - `OrdinalMatch`: entity_to_path.keys == prior_value.keys
    - `SameOrderOfMagnitude`: `prior_value > 0`
    - `RelativeBand`: `ratio > 1.0`
14. Numeric sanity for finite-floats (v2 codex F-CR-bug-4):
    - `epsilon` finite and `> 0`
    - `ratio` finite and `> 1.0`
    - `observed_value` (== internal `prior_value`) finite
    - Reject NaN, Inf, -Inf on every f64 field
15. `monotone_with` carve-out (v2 codex F-CR-bug-monotone):
    `monotone_with` patterns have NO `observed_value` at the
    schema level — the shape itself is the observation. The
    `metric_definition` prose is required to describe what
    series shape is being asserted. Translator rejects
    `observed_value` on a `monotone_with` observation block.

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

## What success looks like for v1 (codex F-CR6 tightened)

The remaining 6 model-proposed candidates from rustims PR #33 —
the ones that didn't fit measurement or concordance — become
**representable** as `third_party_observation` claims *after
curator splitting*. Codex flagged that several raw candidates
are multi-value (e.g. "PEAKS-XPro 1.8% (10k) and 1.15% (100k)")
and a scalar `numeric_band` can't represent both cells as a
single claim — the curator splits them into per-cell claims
first.

Concretely: each curator-split per-cell observation translates
cleanly. The 7 raw candidates expand to ~10-12 per-cell claims
across the rustims paper; each is one
`third_party_observation` claim with `pattern_kind:
numeric_band` (or `ordinal_match` for the tool-ranking
candidate).

The curator can then hand-author a docker image emitting an
artifact with all per-cell metrics, run the existing
`evident_agent.concordance.evaluate` comparator (no new
comparator code needed), and the rendered report shows
`## Observation result: Pass ✓` (or Fail) for each cell.

After this lands, the rustims paper's extraction-rate metric
becomes "representable claim count" (after splitting):
**~1 measurement + ~10-12 observations** — vs. the previous
1/7 raw acceptance. The framework can express the paper's
load-bearing claims; whether the curator wants to split them
all is a separate question.
</stdin>
codex
**Potential Bugs / Risks**

1. **Internal `prior_value` reuse is acceptable, but brittle at API boundaries.**  
Keeping `prior_value` inside `ConcordancePattern` is the right v1 split if the enum is genuinely shared comparator machinery. Renaming the internal Rust enum now risks churn across concordance, sidecars, Python dispatch assumptions, tests, and render paths for little functional gain.

But I would add one hard rule: `prior_value` must never leak into any third-party-observation external surface: YAML, rendered Markdown/HTML, MCP, schema docs, validation errors, JSON returned by query tools. The highest-risk bug is an error like `missing prior_value` for an observation manifest that correctly used `observed_value`. Add tests for parse error text and MCP output, not just happy-path translation.

Longer term, `reference_value` is probably the better neutral internal name, but I would defer until there is another reason to touch the shared pattern type.

2. **Yes, tier-conditional evidence can be gamed by a stub replay contract.**  
The current rule gates on contract presence, not replay credibility. A curator could write:

```yaml
tier: ci
evidence:
  docker_image: ...
  command: echo '{"maxquant.peak_matching_error.fraction_pct": 30.0}'
  artifact: ...
  replay_status: available
```

and satisfy the schema while not replaying MaxQuant or rustims data. That weakens the stated semantics: “independent party replayed the docker and observed the same value.”

I would not try to solve full semantic replay validation in schema, but v1 needs a stronger promotion gate for `tier:ci` / `tier:release`:

- require a structured review/promotion event with `reproduced_by`, timestamp, and reviewer identity distinct from the extractor/author where possible;
- require the sidecar status to be `pass`, not merely evidence `replay_status: available`;
- require immutable docker digest, command, artifact path, and artifact schema;
- ideally require `replay_status: available` plus `comparison_status: pass` from `last_concorded.json`.

Without the sidecar-pass requirement, `tier:ci` can mean “there is a container,” not “the observation was reproduced.”

3. **`monotone_with` needs more structure than prose-only `metric_definition`.**  
“No `observed_value`; the shape is the observation” is directionally right, but saying the shape lives only in `metric_definition` undercuts machine validation. The comparator still needs to know the ordered independent variable and the expected direction.

If existing `ConcordancePattern::MonotoneWith` already encodes something like `x_path`, `y_path`, and direction, then the draft should say that explicitly: no scalar `observed_value`, because the structured pattern fields define the expected series shape. If the enum does not encode direction/order, prose-only is too weak.

A better framing:

> `monotone_with` observations omit scalar `observed_value`; the expected relation is carried by the structured `monotone_with` pattern fields, while `metric_definition` defines the domain meaning of the series.

If the current primitive cannot express increasing/decreasing/nonincreasing/nondecreasing, add that before using it for observation.

4. **The boundary questions are mostly ordered, not truly disjoint.**  
The ordering saves you, but the rules are not inherently disjoint.

Edge cases:

- A paper’s own system wraps a third-party tool and reports that tool’s internal metric. Rule 1 and rule 4 can both feel applicable. You need “subject of the bound” language: if the asserted bound is about the wrapper/system performance, `measurement`; if about the third-party tool behavior under benchmark conditions, `third_party_observation`.
- A declarative config claim later tested empirically, e.g. “Tool X requires Python >=3.10 and fails on 3.9.” The declaration is `metadata_compatibility`; the failure behavior is observation or measurement depending on subject.
- A prior-literature value measured again on the authors’ own data. If the claim is “we reproduce Meier 2024,” concordance; if the claim is “Tool X has 30% error on our data,” observation, even if Meier is cited for context.

So: call them “ordered precedence rules,” not disjoint questions.

5. **New v2 breakage to watch.**  
The `serde(default)` on `ManifestEvidence.oracle` may accidentally relax oracle requirements for other kinds unless translator validation re-imposes them. Add regression tests proving measurement still requires whatever oracle structure it required before.

Also, sharing `last_concorded.json` for concordance and observation risks semantic confusion in render/MCP labels. The sidecar shape can be reused, but UI text should say “observed” for observations and “concorded” for concordance.

6. **Success criteria are more honest, but the lower bound is likely below 10-12.**  
“Representable after curator splitting” is the right upper-bound framing. Realistic lower bound depends on whether each candidate has enough locator, metric definition, and scalar extractability.

I would state success as:

> Expected: ~10-12 per-cell observations are representable after splitting. Minimum credible success: 6-8 translate cleanly without weakening metric definitions or overloading scalar patterns.

Some raw candidates will probably collapse because the paper text does not pin down the metric, tool version, condition cell, or numeric value tightly enough. That should count as honest non-representability, not framework failure.
tokens used
7,345
**Potential Bugs / Risks**

1. **Internal `prior_value` reuse is acceptable, but brittle at API boundaries.**  
Keeping `prior_value` inside `ConcordancePattern` is the right v1 split if the enum is genuinely shared comparator machinery. Renaming the internal Rust enum now risks churn across concordance, sidecars, Python dispatch assumptions, tests, and render paths for little functional gain.

But I would add one hard rule: `prior_value` must never leak into any third-party-observation external surface: YAML, rendered Markdown/HTML, MCP, schema docs, validation errors, JSON returned by query tools. The highest-risk bug is an error like `missing prior_value` for an observation manifest that correctly used `observed_value`. Add tests for parse error text and MCP output, not just happy-path translation.

Longer term, `reference_value` is probably the better neutral internal name, but I would defer until there is another reason to touch the shared pattern type.

2. **Yes, tier-conditional evidence can be gamed by a stub replay contract.**  
The current rule gates on contract presence, not replay credibility. A curator could write:

```yaml
tier: ci
evidence:
  docker_image: ...
  command: echo '{"maxquant.peak_matching_error.fraction_pct": 30.0}'
  artifact: ...
  replay_status: available
```

and satisfy the schema while not replaying MaxQuant or rustims data. That weakens the stated semantics: “independent party replayed the docker and observed the same value.”

I would not try to solve full semantic replay validation in schema, but v1 needs a stronger promotion gate for `tier:ci` / `tier:release`:

- require a structured review/promotion event with `reproduced_by`, timestamp, and reviewer identity distinct from the extractor/author where possible;
- require the sidecar status to be `pass`, not merely evidence `replay_status: available`;
- require immutable docker digest, command, artifact path, and artifact schema;
- ideally require `replay_status: available` plus `comparison_status: pass` from `last_concorded.json`.

Without the sidecar-pass requirement, `tier:ci` can mean “there is a container,” not “the observation was reproduced.”

3. **`monotone_with` needs more structure than prose-only `metric_definition`.**  
“No `observed_value`; the shape is the observation” is directionally right, but saying the shape lives only in `metric_definition` undercuts machine validation. The comparator still needs to know the ordered independent variable and the expected direction.

If existing `ConcordancePattern::MonotoneWith` already encodes something like `x_path`, `y_path`, and direction, then the draft should say that explicitly: no scalar `observed_value`, because the structured pattern fields define the expected series shape. If the enum does not encode direction/order, prose-only is too weak.

A better framing:

> `monotone_with` observations omit scalar `observed_value`; the expected relation is carried by the structured `monotone_with` pattern fields, while `metric_definition` defines the domain meaning of the series.

If the current primitive cannot express increasing/decreasing/nonincreasing/nondecreasing, add that before using it for observation.

4. **The boundary questions are mostly ordered, not truly disjoint.**  
The ordering saves you, but the rules are not inherently disjoint.

Edge cases:

- A paper’s own system wraps a third-party tool and reports that tool’s internal metric. Rule 1 and rule 4 can both feel applicable. You need “subject of the bound” language: if the asserted bound is about the wrapper/system performance, `measurement`; if about the third-party tool behavior under benchmark conditions, `third_party_observation`.
- A declarative config claim later tested empirically, e.g. “Tool X requires Python >=3.10 and fails on 3.9.” The declaration is `metadata_compatibility`; the failure behavior is observation or measurement depending on subject.
- A prior-literature value measured again on the authors’ own data. If the claim is “we reproduce Meier 2024,” concordance; if the claim is “Tool X has 30% error on our data,” observation, even if Meier is cited for context.

So: call them “ordered precedence rules,” not disjoint questions.

5. **New v2 breakage to watch.**  
The `serde(default)` on `ManifestEvidence.oracle` may accidentally relax oracle requirements for other kinds unless translator validation re-imposes them. Add regression tests proving measurement still requires whatever oracle structure it required before.

Also, sharing `last_concorded.json` for concordance and observation risks semantic confusion in render/MCP labels. The sidecar shape can be reused, but UI text should say “observed” for observations and “concorded” for concordance.

6. **Success criteria are more honest, but the lower bound is likely below 10-12.**  
“Representable after curator splitting” is the right upper-bound framing. Realistic lower bound depends on whether each candidate has enough locator, metric definition, and scalar extractability.

I would state success as:

> Expected: ~10-12 per-cell observations are representable after splitting. Minimum credible success: 6-8 translate cleanly without weakening metric definitions or overloading scalar patterns.

Some raw candidates will probably collapse because the paper text does not pin down the metric, tool version, condition cell, or numeric value tightly enough. That should count as honest non-representability, not framework failure.
