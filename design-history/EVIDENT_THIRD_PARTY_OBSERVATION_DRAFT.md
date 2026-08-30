# Design — `kind: third_party_observation` (v3 DRAFT)

> v3 incorporates the codex review of v2
> (`EVIDENT_THIRD_PARTY_OBSERVATION.codex-review-v2.md`).
> Substantive changes:
>
> - **`tier:ci` / `tier:release` promotion gate strengthened**
>   (codex v2 F-CR2 — the v2 rule "evidence present" was gameable
>   by a `command: echo '{...}'` stub). v3 requires the
>   `last_concorded.json` entry to carry `comparison_status: pass`,
>   not just `evidence.replay_status: available`. Promotion to
>   `tier:ci` MUST have a recent pass; promotion to `tier:release`
>   MUST additionally pin the docker `image_digest` immutably.
> - **`prior_value` field MUST NOT leak** into any external
>   surface (codex v2 F-CR1). Internal Rust enum keeps the name
>   for code reuse, but parse errors, render output, MCP results,
>   and validation messages on the observation side use
>   `observed_value`. Tests assert on error message strings.
> - **`monotone_with` explicit field documentation** (codex v2
>   F-CR3). `MonotoneWith` already carries `direction`,
>   `metric_path`, `parameter_path` from PR5f; the shape lives
>   there, not in prose. `metric_definition` clarifies what the
>   series elements ARE (the domain meaning of "complexity" or
>   "dataset size"), but the comparator semantics come from the
>   structured fields.
> - **"Ordered precedence rules", not "disjoint questions"**
>   (codex v2 F-CR4). The 4-step boundary decision rule is
>   precedence-ordered: rule (k) applies only if (1)…(k-1)
>   didn't. Three edge cases documented explicitly: wrapper
>   systems, declarative-then-empirical claims,
>   prior-cited-but-not-concorded claims.
> - **`#[serde(default)] oracle` regression risk** (codex v2
>   F-CR5). Test that measurement / concordance / metadata
>   claims still enforce their pre-PR5i oracle rules.
> - **Success criteria split**: expected ~10–12 representable
>   after splitting; minimum credible 6–8 translate without
>   weakening (codex v2 F-CR6).
> - **Sidecar UI text**: `last_concorded.json` shape is reused,
>   but render/MCP output for observation claims says "observed"
>   not "concorded" (codex v2 F-CR5 second note).
>
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

### Ordered precedence rules for kind selection (v3 codex F-CR4)

When a curator is choosing which kind to author, apply these
rules in order. Rule (k) applies only if rules (1)…(k-1)
didn't fire. The rules are NOT inherently disjoint — the
ordering does the work.

1. **Is the bound asserted about the paper's own system?**
   → `measurement`. Subject test: ask "who owns the bound
   the paper is asserting?" If the wrapper system the paper
   built, this rule fires even when the wrapped tool is
   third-party. ("Our system achieves FDR < 1% via
   FragPipe's algorithm" → measurement, because the bound is
   about "our system").
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
shipped; v3 documents them here so the boundary is explicit.

**Edge cases the ordering resolves** (codex v2 F-CR4):

- **Wrapper system around a third-party tool.** A paper
  reports the wrapped tool's internal metric. If the asserted
  bound is about the wrapper/system performance, rule (1)
  fires (measurement). If the assertion is about the
  third-party tool's behavior under benchmark conditions
  with no bound on the wrapper, rule (4) fires (observation).
  Curator tiebreaker: "what is the paper holding itself
  accountable for?"
- **Declarative-then-empirical claims.** "Tool X requires
  Python ≥ 3.10 and fails on 3.9." The declaration ("X
  requires Python ≥ 3.10") is `metadata_compatibility`
  (rule 2). The behavior ("fails on 3.9") is
  `measurement` or `third_party_observation` depending on
  whose system is failing. Author both as separate claims;
  the kinds are disjoint.
- **Prior literature cited but NOT for concordance.** "Tool
  X has 30% error on our data (cf. Meier 2024 §3)." If the
  claim isn't asserting Meier's value matches the paper's
  observation, rule (3) does NOT fire. The Meier citation is
  context-only; rule (4) fires (observation).

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
12. Evidence presence + promotion gate by tier (v3 codex F-CR2
    strengthened — v2's "evidence + replay_status: available"
    was gameable by a `command: echo '{...}'` stub):
    - `tier:research`: evidence is OPTIONAL. Missing evidence
      renders as `replay_status: not_attempted` and the
      claim's status is `NotAssessed`.
    - `tier:ci`: evidence is REQUIRED (docker contract
      present), `replay_status` MUST be `available`, AND the
      `last_concorded.json` overlay MUST carry
      `comparison_status: pass` for this claim_id. A stub
      docker contract without a sidecar pass entry fails the
      promotion gate.
    - `tier:release`: all `tier:ci` requirements, PLUS the
      docker image MUST be pinned by content-addressable
      digest (`@sha256:...`), the artifact path MUST be
      pinned, the `artifact_schema_version` MUST be
      explicitly set (no implicit version). v3 codex flagged
      the v2 rule allowed mutable `latest`-style tags.
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

**Expected** (v3 codex F-CR6 split): ~10–12 per-cell
observations are representable after splitting.

**Minimum credible success**: 6–8 translate cleanly without
weakening metric definitions or overloading scalar patterns to
mean things they shouldn't. Some raw candidates will probably
collapse because the paper text doesn't pin down the metric,
tool version, condition cell, or numeric value tightly
enough. Those non-translations should count as **honest
non-representability** (the framework correctly says "we can't
express this") rather than framework failure.

After this lands, the rustims paper's extraction-rate metric
becomes "representable claim count" (after splitting):
**~1 measurement + 6–12 observations** — vs. the previous 1/7
raw acceptance. The framework can express most of the paper's
load-bearing claims; whether the curator wants to split them
all is a separate question.
