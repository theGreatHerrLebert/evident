# EVIDENT Manifest Schema

Reference for the manifest format consumed by `validate_manifest.py` and
`evident.py`. Designed to be **machine-queryable** so claims can be
composed into requirement profiles, not only read by humans.

## Design principles

The schema enforces a small set of rules. The rationale lives in
`GRAMMAR.md`; this is the one-line summary so a reader knows what the
format is trying to protect before scanning the field reference.

1. **The prose is the docstring; the structured fields are the data.**
   No inference or query layer is required to parse English.
2. **Primitives stay tight; expressiveness comes from composition.**
   Cross-cutting enums (`tier`, `tolerance_metric`, `tolerance_op`,
   `input_class`, `kind`, `provenance`, `trust_strategy`) are closed
   sets and grow only by intentional spec change.
3. **The manifest is a record, not an interpretation.** Scoring,
   ranking, risk modeling, and probabilistic inference belong in
   separate tools that read the manifest.
4. **Closed cross-cutting vocabularies; open project vocabularies.**
   `subsystem`, `oracle`, and `capability` are declared per-manifest;
   everything else is fixed by the framework.
5. **Identity and reference are structured, not nominal.** Cross-claim
   relationships are claim ids and vocabulary terms, not prose.
6. **The escape hatch is `tier: research`, and only there.** Loosely-
   structured claims are admissible only at research tier and cannot
   be promoted without acquiring structure.

See `GRAMMAR.md` for the admissibility table by tier, the composition
contract, and the anti-patterns to refuse on review.

## Manifest top-level

```yaml
version: 0.1
project: proteon     # required; canonical project name. Must appear as a
                     # key in every measurement claim's pinned_versions.
vocabularies:        # optional; merged with framework defaults
  subsystem:    [...]
  oracle:       [...]
  capability:   [...]
  tolerance_metric: [...]   # extends base set
  tolerance_op:     [...]   # extends base set
  input_class:      [...]   # extends base set
include:             # optional; flat (no chained includes)
  - claims/foo.yaml
claims:              # optional if include: provides them
  - { ... }
```

Extraction outputs whose `project` starts with `extracted/` may contain
an empty `claims: []` list. That records a negative extraction result
without forcing a dummy claim into the corpus.

### Framework-provided vocabulary defaults

The validator ships these and unions them with whatever the consumer
declares:

| Vocabulary         | Default values                                                    |
|--------------------|--------------------------------------------------------------------|
| `tolerance_metric` | `relative_error`, `median_relative_error`, `absolute_error`, `pass_rate`, `recall`, `precision`, `f1`, `drift` |
| `tolerance_op`     | `<`, `<=`, `>=`, `>`, `==`                                         |
| `input_class`      | `single-chain`, `multi-chain`, `random-sample`, `synthetic`, `fixture` |
| `subsystem`        | empty — consumer must define                                       |
| `oracle`           | empty — consumer must define                                       |
| `capability`       | empty — consumer must define                                       |

Unknown values in any vocabulary axis are a validation error. This is
deliberate: composability dies the moment "Biopython" and "biopython"
silently mis-match.

## Claim fields

| Field             | Required | Type   | Description |
|-------------------|----------|--------|-------------|
| `id`              | yes      | string | Stable identifier; unique across the merged manifest |
| `title`           | yes      | string | One-line human title |
| `kind`            | no       | enum   | `measurement` (default), `policy`, `reference`, `implementation`, `pipeline`, `scientific`, `performance`, `release_gate`, `metadata_compatibility`, `behavioral_concordance`, `third_party_observation` |
| `subsystem`       | yes¹     | string | From `vocabularies.subsystem` |
| `case`            | yes²     | path/string | Markdown writeup or locator; resolved as a path for full workflow claims |
| `source`          | yes²     | path/string | Code/artefact root or source locator; resolved as a path for full workflow claims |
| `tier`            | yes      | enum   | `ci`, `release`, `research` |
| `trust_strategy`  | yes      | list   | Subset of `validation`, `understanding`, `proof` |
| `pattern`         | no       | path   | Optional pointer into `patterns/` |
| `capabilities`    | no       | list   | From `vocabularies.capability` — what user requirements this claim *satisfies* |
| `inputs`          | yes¹     | object | `{corpus, n, class, corpus_sha}` — what the claim is asserted over |
| `outputs`         | no       | object | Named measured quantities: `{name: {unit, description}}` |
| `pinned_versions` | yes¹     | object | Source release/SHA + oracle/environment versions |
| `claim`           | yes      | string | Prose statement of the claim |
| `tolerances`      | yes¹     | list   | Structured tolerance entries (see below) |
| `evidence`        | yes²     | object | `{oracle, command, artifact}` for measurement/workflow claims; `{command, artifact}` for concordance/observation claims |
| `metadata`        | yes³     | object | Declaration block for `kind: metadata_compatibility` |
| `concordance`     | yes⁴     | object | Prior-literature comparison block for `kind: behavioral_concordance` |
| `observation`     | yes⁵     | object | Paper-self third-party tool observation block for `kind: third_party_observation` |
| `provenance`      | no       | enum/object | `automatic` (default), `human`, `peer-reviewed`, extractor provenance, or structured provenance (see below) |
| `reviewers`       | no⁶      | list   | Named reviewers backing a `peer-reviewed` claim |
| `last_verified`   | no       | object | `{commit, date, value, corpus_sha}` — staleness signal |
| `assumptions`     | yes      | list   | Prose strings |
| `failure_modes`   | yes      | list   | Prose strings |

¹ Required only when `kind: measurement`. Policy and reference claims may
omit `subsystem`, `inputs`, `pinned_versions`, and `tolerances`.

² Required for full workflow claims (`measurement`, `policy`,
`reference`, `implementation`, `pipeline`, `scientific`, `performance`,
`release_gate`). `metadata_compatibility` may carry `source` and `case`
as source locators, but they are not resolved as local paths.
Research-tier claims with structured provenance
`kind: extracted-from-paper` or `kind: extracted-from-repo` are draft
extractions: they may omit `trust_strategy`, `assumptions`, and
`failure_modes`, and may use source-local metric/oracle names before
curation promotes them into project vocabularies.

³ Required only when `kind: metadata_compatibility`; forbidden on other
kinds.

⁴ Required only when `kind: behavioral_concordance`; forbidden on other
kinds.

⁵ Required only when `kind: third_party_observation`; forbidden on other
kinds.

### Pinning rule for measurement claims

A numerical claim is only reproducible if every degree of freedom that
could shift the result is pinned. The schema enforces this for
`kind: measurement`:

- **Source**: `pinned_versions` must include an entry naming the project
  under test, with either a release tag (`proteon: 0.1.1`) or a git SHA
  (`proteon: 47fe1ab`).
- **Oracles**: `pinned_versions` must include every oracle named in
  `evidence.oracle` with the version used to produce the cited result
  (`Biopython: 1.83`).
- **Inputs**: `inputs.corpus_sha` is required at `tier: release` — the
  1000-PDB pool is meaningless without a hash. CI-tier claims may omit
  `corpus_sha` only when the corpus is a single fixture file checked
  into the source tree (then the source SHA pins it transitively).
- **Outputs**: `outputs` is optional but recommended when more than one
  quantity is measured (e.g. a forcefield claim spans bond, angle, vdW,
  electrostatic, GB components — each is its own output). When set, each
  `tolerances[].output` should reference an entry by name.

A claim that fails any of these is structurally not a numerical claim —
the validator will demand it be downgraded to `kind: reference` (a
pointer to where the work happens) or `kind: policy` (a process rule).

### Typed declaration kinds

Three claim kinds are typed declarations rather than full measurement
workflow claims. They are consumed by the `typed-trust` translator and
keep their evidence model separate from measurement claims.

#### `kind: metadata_compatibility`

Use for declarative facts read directly from configuration files. The
declaration is the evidence, so `evidence` and `tolerances` are
forbidden.

```yaml
kind: metadata_compatibility
metadata:
  field: python_version_requirement
  declared_value: ">=3.10"
  source_file: pyproject.toml
  source_path: project.requires-python
```

#### `kind: behavioral_concordance`

Use when this artifact claims that its measured behavior tracks a value
from prior literature. The `concordance` block is required; top-level
`source`, `tolerances`, `metadata`, and `observation` are forbidden.
For `tier: ci` and `tier: release`, `evidence` is required and must not
carry `oracle`.

```yaml
kind: behavioral_concordance
concordance:
  paper_locator: source/cited.md#figure-3
  pattern:
    pattern_kind: numeric_band
    metric_path: outputs.tool.error_rate_pct
    epsilon: 5.0
    prior_value: 30.0
  prior_binding:
    prior_unit: percent
    prior_metric_definition: Peak matching error rate
    locator: "Meier 2024 Figure 3"
    prior_extraction_note: Value transcribed from plotted label.
    source_id: doi:10.example/meier-2024
```

#### `kind: third_party_observation`

Use when a paper observes a third-party tool's behavior on the paper's
own data, with no prior-literature value being concorded. The
`observation` block is required; top-level `source`, `case`,
`last_verified`, `tolerances`, `metadata`, and `concordance` are
forbidden. For `tier: ci` and `tier: release`, `evidence` is required
and must not carry `oracle`.

```yaml
kind: third_party_observation
observation:
  third_party_tool: MaxQuant
  metric_definition: Peak matching error rate against the paper's simulated truth set.
  paper_locator: source/cited.md#maxquant-peak-matching
  pattern:
    pattern_kind: numeric_band
    metric_path: maxquant.peak_matching_error.fraction_pct
    epsilon: 5.0
    observed_value: 30.0
```

`behavioral_concordance` and `third_party_observation` share five
pattern primitives:

- `numeric_band`: `metric_path`, `epsilon`, and `prior_value` or
  `observed_value`
- `relative_band`: `metric_path`, `ratio > 1.0`, and `prior_value` or
  `observed_value`
- `same_order_of_magnitude`: `metric_path` and positive `prior_value`
  or `observed_value`
- `ordinal_match`: `entity_to_path`, `direction`, and a per-entity
  value map whose keys match `entity_to_path`
- `monotone_with`: `metric_path`, `parameter_path`, and `direction`;
  no scalar value field is allowed

### Tolerance entry

```yaml
tolerances:
  - metric: relative_error      # from tolerance_metric vocab
    op: "<"                     # from tolerance_op vocab
    value: 0.02                 # numeric
    prose: |                    # required; what the tolerance means in context
      |proteon_total - biopython_total| / biopython_total < 0.02 on 1crn
```

`metric`, `op`, `value` are all-or-nothing: either supply all three, or
supply none and use only `prose` (e.g. for research-tier deferred-spec
claims). `prose` is always required.

A claim may carry multiple tolerance entries — used when several
simultaneous conditions must hold (median accuracy AND pass-rate, etc.).
Each is queryable independently.

### Inputs

```yaml
inputs:
  corpus: 1crn-fixture           # free-form name; pin in your repo
  n: 1                           # how many structures/items
  class: single-chain            # from input_class vocab
  corpus_sha: <git-or-data-sha>  # required at tier=release; pins corpus version
  fixture_path: tests/fixtures/1crn.pdb   # optional; path under source/
```

### Outputs

```yaml
outputs:
  total_sasa:
    unit: A^2
    description: Per-structure total solvent-accessible surface area.
  per_atom_sasa:
    unit: A^2
    description: SASA distributed across heavy atoms.
```

Each named entry can be referenced by `tolerances[].output` so a single
claim with several measured quantities (forcefield energy components, for
example) keeps the metric/output binding explicit instead of implicit.

### Pinned versions

```yaml
pinned_versions:
  proteon: 0.1.1                 # the project under test (tag or SHA)
  Biopython: "1.83"              # one entry per oracle named in evidence.oracle
  python: "3.12.2"               # interpreter / runtime, when behavior depends on it
```

The validator requires that every name in `evidence.oracle` appears as a
key in `pinned_versions`, and that exactly one key matches the project
declared by the manifest's `source:` (the project under test). Versions
are strings; quote numeric-looking versions to avoid YAML parsing them
as floats.

### Provenance and reviewers

```yaml
provenance: peer-reviewed     # automatic (default) | human | peer-reviewed | ...
review_status: peer-reviewed  # optional review badge, same reviewer gate
reviewers:                    # required iff provenance/review_status is peer-reviewed
  - name: Jane Doe
    source: external
    orcid: "0000-0000-0000-0000"   # optional
    affiliation: Example University # optional
    date: 2026-04-30                # ISO date the review was completed
```

`provenance` declares **how the claim was vetted**, not whether the
underlying numbers are correct (that is what `tolerances` and
`last_verified` are for). The three levels:

- `automatic` — produced by a runner (CI, an oracle script, a benchmark).
  No human attested to it. Default if omitted.
- `human` — a human inside the project read the case writeup and the
  evidence and judged the claim sound. The author of the code may be
  this human.
- `peer-reviewed` — at least one named reviewer who is **not an author
  of `source`** read the claim and the evidence and signed off, with
  ORCID or affiliation recorded. The manifest entry is the *record* of
  that review, not the review itself.

The validator also accepts provenance values used by extracted and
ported claims: `author`, `maintainer`, `external`, `generated`,
`ported`, `third-party`, `extracted-from-paper`, and
`extracted-from-repo`.

Extractor output may use a structured provenance block:

```yaml
provenance:
  kind: extracted-from-repo
  source_id: github:org/repo@47fe1ab
  source_sha: sha256:...
  source_context: repo_authored   # repo_authored | copied_external_text | unknown
  extractor:
    model: evident-agent.extract-metadata
    model_version: "0.1.0"
    extracted_at: "2026-06-03T13:53:27Z"
  curator: null
```

⁶`reviewers` is required when `provenance == peer-reviewed` or
`review_status == peer-reviewed` and forbidden otherwise. Each entry
must have a non-empty `name`. `source` may be `author`, `maintainer`,
`external`, or `venue`; `peer-reviewed` status requires at least one
`external` or `venue` reviewer when reviewer sources are supplied.
`orcid`, `affiliation`, and `date` are optional strings; ISO dates are
recommended.

`provenance` is a coarse, self-declared signal — it is more honest than
no signal at all, and lets readers discount accordingly. It does not
replace the underlying `tolerances` and `evidence` fields.

### Last verified

```yaml
last_verified:
  commit: 47fe1ab                # source SHA where the claim last passed
  date: 2026-04-13               # ISO date, always absolute
  value: 0.0017                  # primary observed metric, if scalar
  corpus_sha: <sha>              # corpus version, if applicable
```

Populated by the runner that re-executes `evidence.command`. Stays
optional in the manifest itself; readers should treat `null` as
"unknown / never run / staleness explicitly unclaimed". Live status is
typically held in a sidecar (`last_verified.json`) keyed by claim id so
the manifest stays declarative.

## Composition

Structured fields exist to answer queries like:

```
subsystem == sasa
AND tier in {release}
AND oracle ∩ {OpenMM, BALL} != ∅
AND ∃ tolerance with metric=relative_error, op<=, value<=0.01
AND inputs.class includes multi-chain
```

A "requirement profile" is just such a query plus a verdict on each
claim it touches: covered, partially covered, gap. Free-text fields
(`claim`, `prose`, `assumptions`) are for humans reading individual
cards; they do not participate in the query.

## Migration from v0 (pre-schema)

The pre-schema format had `evidence.tolerance` as a single prose string.
That field is removed. Existing claims migrate by:

1. Lift `evidence.tolerance` into one or more `tolerances:` entries.
   Drop the prose into each entry's `prose:` field, and add structured
   `metric`/`op`/`value` where the prose stated a number.
2. Add `subsystem` from your project's vocabulary.
3. Add `kind: policy` to claims that are process rules rather than
   measurements (and they may then drop `subsystem` and `tolerances`).

The framework will reject manifests that still carry `evidence.tolerance`.
