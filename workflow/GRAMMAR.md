# EVIDENT Grammar

The discipline that the schema enforces, and the rationale behind it.
Companion to `SCHEMA.md`: SCHEMA.md tells you what a valid manifest
looks like; GRAMMAR.md tells you why the rules exist and what would
break if you bent them.

Audience is anyone proposing a new field, a new vocabulary axis, a new
tier, or a new claim shape. Read this first. The rules are short; the
reasons are not.

## Goals

The manifest format must support, in roughly this order of priority:

1. **Reproducibility** — given the manifest, a third party can re-run
   the evidence and judge for themselves.
2. **Query** — a reader can ask structured questions across many claims
   ("what release-tier claims about SASA exist, and which were verified
   in the last 30 days?") without parsing prose.
3. **Composition** — claims can be combined into requirement profiles,
   workflows, and dependency graphs.
4. **Forward compatibility with quantitative interpretation** — a future
   layer should be able to read the same manifest and produce
   posteriors, coverage scores, or risk estimates. The format does not
   commit to any one interpretation, but it must not foreclose them by
   hiding load-bearing semantics in prose.

Goals 2–4 are why we are strict about structured fields. A format that
hits goal 1 only is a notes file, not a manifest.

## Design principles

### 1. The prose is the docstring; the structured fields are the data.

A claim's semantic content must live in fields that an interpreter can
read without parsing English. Prose may accompany every field for
humans reading individual cards, but no inference, query, or
composition layer is ever required to read it.

Concretely: if you cannot reconstruct a serviceable one-line statement
of the claim from `subsystem`, `outputs`, `inputs`, `tolerances`, and
`pinned_versions`, then the claim is not yet structured. The prose
`claim:` field is allowed to exist, but it must be redundant with the
structured fields, never the source of truth.

### 2. Primitives stay tight; expressiveness comes from composition.

The format does not try to be a universal description language. The
primitives — `tolerance_metric`, `tolerance_op`, `input_class`, `tier`,
`provenance`, `kind`, `trust_strategy` — are deliberately small closed
sets. New expressive power comes from *composing* primitives, not from
loosening any one of them.

The cost of an extra enum value is not the line of code; it is that
every consumer of the manifest now has to handle that case correctly,
forever. Default to refusing.

### 3. The manifest is a record, not an interpretation.

Multiple readers should be able to consume the same manifest and reach
different conclusions about the underlying project. The validator
checks structural and vocabulary consistency. Inference, scoring, and
risk modeling belong in *separate* tools that consume the manifest.

This separation is what keeps the schema durable: an interpreter that
embarrasses itself does not invalidate a single existing claim.

### 4. Closed cross-cutting vocabularies; open project vocabularies.

Cross-cutting axes (`tolerance_metric`, `tolerance_op`, `input_class`,
`tier`, `provenance`, `kind`, `trust_strategy`) are closed and grow
only by intentional spec change. They have to be closed for cross-
project queries to mean anything.

Project-specific axes (`subsystem`, `oracle`, `capability`) are open,
declared per-manifest in `vocabularies:`, and validated against the
declared set. They cannot be enumerated centrally because every project
carves up its world differently.

### 5. Identity and reference are structured, not nominal.

When one claim depends on another, fails because of an input class, or
is supplanted by a successor, that relationship is recorded as a
structured cross-reference (a claim id, an `input_class` value, a
capability name) — never as prose ("see also the SASA claim above").
Prose does not propagate through queries or composition.

### 6. The escape hatch is `tier: research`, and only there.

There is exactly one place where loosely-structured claims are
admissible: research-tier claims, which exist precisely to record
work-in-progress where the right structure has not yet been found.
Research claims may carry prose-only tolerances and unspecified
quantities. They cannot be promoted to higher tiers without acquiring
structure.

Do not introduce a second escape hatch. Pressure to relax the schema
at `release` or `ci` is almost always pressure to allow prose to do
load-bearing work; redirect it into research-tier first.

## Admissibility by tier

| Constraint                                  | `research` | `ci`    | `release` |
|---------------------------------------------|:----------:|:-------:|:---------:|
| Structured tolerance (metric/op/value)      | optional   | required| required  |
| `inputs.corpus_sha` for n>1                 | optional   | optional¹| required  |
| `pinned_versions` covers source + oracles   | required   | required| required  |
| Prose-only assumptions/failure_modes        | allowed    | allowed | allowed²  |
| `last_verified` populated                   | optional   | recommended | required |
| `provenance` declared                       | optional   | optional| recommended |

¹ `ci`-tier may pin the corpus transitively via the source SHA when the
corpus is a fixture file checked into the source tree.
² Allowed in the sense that prose is not forbidden, but anything that
is *also* expressible as a structured cross-reference (another claim id,
an `input_class`, a `capability`) must use the structured form. Pure-
prose entries are reserved for genuinely qualitative content.

## Composition contract

For composition layers (query, requirement profiles, future inference)
to be sound, every manifest must obey:

- **Unique identity.** Claim ids are unique across the merged manifest.
  No silent shadowing across `include:`d files.
- **Declared dependencies.** Where one claim relies on another,
  it must say so via a structured field (today: `capabilities`;
  reserved for future use: `depends_on`, `refines`, `supersedes`,
  `contradicts`). Hidden dependencies via prose break composition.
- **No cycles.** The dependency graph among claims is a DAG.
- **Stable semantics under re-execution.** Re-running
  `evidence.command` on the pinned versions produces a result that
  is comparable to the recorded `last_verified.value`. If the command
  is non-deterministic, the claim must say so in `assumptions`.
- **Vocabulary stability within a manifest version.** Removing or
  renaming a vocabulary term is a breaking change. Adding a term is
  not. Plan accordingly.

## Vocabulary discipline

When in doubt about whether to extend a vocabulary:

- **Cross-cutting (closed) axes:** prefer to expand the schema spec and
  the framework defaults together, with a version bump. One PR, one
  rationale, one validator change.
- **Project (open) axes:** declare the term in the consumer manifest's
  `vocabularies:` block. Do not patch the framework defaults to
  accommodate one project's `subsystem` taxonomy.
- **Case and spacing.** Vocabulary terms are case-sensitive and treated
  as opaque. `Biopython` and `biopython` are different terms; the
  validator will not silently merge them. This is by design.

## Anti-patterns

Concrete shapes to refuse on review:

- **Load-bearing prose.** A `claim:` string that asserts something not
  recoverable from the structured fields. Fix by promoting the
  structure (add an output, a tolerance, an input class), not by
  leaving the prose authoritative.
- **Prose-only tolerance at `release` or `ci`.** A tolerance entry with
  only `prose:` and no `metric/op/value` outside `tier: research`.
  Either supply the structured triple or demote the claim.
- **Untyped assumption that is really another claim.** "We assume
  Biopython 1.83 is correct on this input" is itself a claim and
  should be a referenced claim id under a structured `depends_on:` once
  that field exists; in the interim, name the dependency in
  `pinned_versions` and `evidence.oracle`, not free-form in
  `assumptions:`.
- **Silent vocabulary drift.** Two terms that are spelling variants of
  the same concept (`OpenMM` vs `openmm`, `OBC` vs `OBC2`). The
  validator treats them as distinct on purpose. Decide which term is
  canonical and migrate.
- **Provenance inflation.** Marking a claim `peer-reviewed` without
  named external reviewers in `reviewers:` whose `source` they are not
  authors of. The validator enforces presence; the spirit of the rule
  enforces independence.
- **Schema relaxation requested under deadline pressure.** Almost
  always a sign that a research-tier claim is being miscast as
  release-tier. Demote, ship the lower-tier claim, and revisit the
  structure offline.

## Out of scope (intentionally)

- **Probability semantics.** The format is intended to be readable by a
  future probabilistic interpreter (priors from `provenance`, likelihoods
  from `tolerances`+`last_verified`, conditioning from
  `pinned_versions`), but no such interpreter is part of the framework
  today, and none of the schema fields commit to any specific Bayesian
  parametrization. Keep it that way; ship the interpreter as a separate
  tool when it exists.
- **Identity / authentication of reviewers.** `reviewers:` records
  what was claimed (name, ORCID, affiliation, date). It does not
  cryptographically attest. Signature schemes are a separate layer.
- **Retraction workflow.** When a claim is wrong, today the answer is
  "edit the manifest and bump the version." A formal retraction
  mechanism may be added; it is not required for the format to be
  useful.

## Changing this document

GRAMMAR.md is a stability commitment. Changes to the principles or the
admissibility table imply that existing manifests may need migration.
Treat each change as a minor version bump of the framework spec, with
a migration note in `SCHEMA.md` of the form already used for the
`evidence.tolerance` removal.
