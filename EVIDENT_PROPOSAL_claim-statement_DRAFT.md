# Proposal: the exact claim is first-class (`claim.statement` + `claim.plain` + `claim.as_stated`)

Status: DRAFT proposal. Not yet normative. Targets `workflow/SCHEMA.md` (manifest surface)
and `workflow/GRAMMAR.md` (discipline), with a note on the `concepts/typed-trust.md` seam.

## The hole

The manifest carries one prose field for the claim:

```yaml
claim: >
  A mid-flight GPU proteomics pipeline should be evaluated in layers ...
```

`workflow/GRAMMAR.md` principle 1 already says this prose "must be redundant with the
structured fields, never the source of truth." But in practice the single `claim:`
field collapses two different objects and anchors neither:

1. A **headline** — a human gloss, useful for scanning a card.
2. The **exact claim** — the precisely-scoped, falsifiable proposition that the
   evidence is actually marshaled for.

When those collapse, the headline wins, because it is shorter and reads well. The
result is **gloss drift**: a card passes its tolerances against a statement weaker
or broader than what was actually claimed. Nothing in the schema forces the claim
to be stated at the resolution at which it is falsifiable, and nothing anchors it
to where it was made.

This is the mirror image of the `load-bearing prose` anti-pattern. There, prose
asserts *more* than the structure supports. Here, the headline asserts *less* —
it silently drops scope (system, perturbation, magnitude, the negative control) —
so verification is graded against an easier claim than the real one.

## Fit-test that surfaced it

Two peer-reviewed papers were rendered as EVIDENT cards (Leidel lab, Cell 2015 and
2025; cards in the consumer project). Headline vs. exact claim, one example:

- **headline:** "mcm5s2U counteracts the m6A decoding penalty."
- **exact claim:** "In human HEK293T cells, mcm5s2U at U34 alleviates the
  m6A-induced increase in A-site occupancy *specifically at the m6A-modified,
  mcm5s2U-dependent codons AAA/AGA/GAA*; loss of mcm5s2U biogenesis (ELP1-KO or
  CTU2-KO) intensifies the pausing, and the increase is *reversed by STM2457*."

The headline reads as a general law. The exact claim is scoped to particular
codons, a particular cell system, and a particular knockout-plus-rescue
intervention — which is exactly what a reviewer probes and exactly what the
evidence supports. Every card in the fit-test had this gap until rewritten.

## The engine already supports this

`concepts/typed-trust.md` §5 already types the claim correctly:

```rust
struct Claim {
    text: String,        // the exact statement
    kind: ClaimKind,
    source: SourceSpan,  // where it was made
    explicit: bool,      // stated verbatim vs. inferred
    ...
}
```

So this is **not** a new engine concept. `text` is meant to be the exact statement;
`explicit` already distinguishes verbatim from paraphrase; `source: SourceSpan`
already anchors it. The gap is purely on the **manifest surface** and in the
**authoring discipline**: the shipping `claim:` blob neither separates headline
from statement nor requires the source anchor that `SourceSpan` + `explicit`
presuppose.

## Proposal

Replace the single `claim:` prose field with a small structured block:

```yaml
title: mcm5s2U counteracts the m6A decoding penalty     # existing field; the headline
claim:
  statement: >
    In human HEK293T cells, mcm5s2U at U34 alleviates the m6A-induced increase in
    ribosomal A-site occupancy specifically at the m6A-modified, mcm5s2U-dependent
    codons AAA, AGA, and GAA; loss of mcm5s2U biogenesis (ELP1-KO or CTU2-KO)
    intensifies the pausing, and the increase is reversed by STM2457.
  plain: >
    Ribosomes slow down at certain codons when the mRNA carries an m6A mark. This
    claim says a particular tRNA modification (mcm5s2U) cancels that slowdown at
    three specific codons in human cells — and that removing the modification
    makes the slowdown worse, while a drug that blocks m6A removes it.
  as_stated:
    quote: "mcm5s2U in tRNA modulates the decoding of m6A-modified codons"
    locator: "Highlights; Fig 4C"
    verbatim: true          # maps to Claim.explicit
```

- **`title`** (existing) — the headline. Already on every claim; this proposal
  does not add a separate `headline` field. Never the falsifiable object;
  renderers show it as the card title but must not treat it as the claim.
- **`statement`** — the exact, fully-scoped proposition. The object that evidence
  supports and challenges target. Its scope qualifiers (system, perturbation,
  magnitude, negative control) MUST be reconstructable from the structured fields
  (`subsystem`, `inputs`, `outputs`, `tolerances`, `pinned_versions`), per GRAMMAR
  principle 1. `statement` projects to typed-trust `Claim.text`.
- **`plain`** — the claim in plain words: one or two sentences a non-specialist
  can read to learn *what is actually being tested and why it matters*. It is an
  **explanation, not a restatement** — it may omit scope and precision, and it
  is not required to be complete or to track the statement word-for-word. It is
  never normative: evidence does not support it, challenges do not target it,
  and the validator never reads its content. In typed-trust terms it is a
  `Judged` gloss of `statement` — human-authored, or model-drafted and then
  adopted by a curator (until adopted it is a `Proposed` draft) — and
  renderers MUST label it as such ("In plain words — not the claim") and show
  it *next to* `statement`, never instead of it, so a reader can see the
  simplification and spot drift. Optional at every tier.
- **`as_stated`** — the source anchor. `quote` is verbatim source text (or `null`
  when the statement is inferred, not quoted); `locator` names where (figure,
  section, line); `verbatim` projects to `Claim.explicit`. This is what lets a
  reader check that `statement` did not drift from the source. `locator` projects
  to `Claim.source` (a coarse `SourceSpan`).

## Validator rules

- `claim.statement` required at every tier. `claim.plain` optional at every
  tier; when present it must be a non-empty string. The validator never checks
  its content — fidelity of a simplification is a `Judged` comparison.
- `as_stated.verbatim: true` requires a non-empty `quote`.
- When `as_stated.quote` is present, the validator does NOT check it against the
  statement (that is a Judged comparison, out of scope for a structural validator)
  — but a renderer SHOULD surface both so a human can.
- Scope-in-structure check (lint, not hard fail outside release): if `statement`
  names a scope token (an organism, a cell line, a perturbation, a numeric
  threshold) that appears in NO structured field, warn — the scope is hiding in
  prose. At release tier this is an error.

## Admissibility by tier

| Constraint                                   | research | ci      | release |
|----------------------------------------------|:--------:|:-------:|:-------:|
| `claim.statement` present                    | required | required| required|
| `claim.statement` scope reconstructable from structure | recommended | required | required |
| `claim.plain` present                        | optional | optional| optional|
| `claim.as_stated.locator` present            | optional | required| required|
| `claim.as_stated.quote` (verbatim) present   | optional | optional| required for `explicit` claims |

Research tier may carry a statement whose scope is still prose-only (the claim is
being scoped). It may not be promoted without moving that scope into structure.

## New anti-pattern (for `anti-patterns/`)

- **Headline understatement / gloss drift.** A `claim` whose only prose is a short
  headline broader or weaker than what the evidence actually establishes, so the
  card verifies against an easier claim than the one made. Fix: write the exact
  `statement` with its scope, and anchor it with `as_stated`. The tell: the
  headline would still "pass" if the experiment had been done in a different
  system or with a weaker perturbation.
- **Plain-words overreach.** A `claim.plain` that asserts something the
  `statement` does not (a broader system, a causal reading of a correlation, a
  general law from a scoped result). `plain` may *omit* scope; it must never
  *contradict* it. Fix: rewrite `plain` so that every sentence is implied by
  `statement`. The tell: a reader who only saw `plain` would be surprised by a
  qualifier in `statement`.

## `plain` and the agent

Because `plain` is a `Judged` gloss, it is the natural field for
`evident-agent extract-*` to draft (model-written, from `statement` plus the
structured fields) and for `evident-agent curate` to approve or rewrite. A
model-drafted `plain` that was never curator-approved should render with its
provenance visible, like any other Judged value. The driver (`EVIDENT_DRIVER.md`)
may quote `plain` when explaining a claim to a user, but must tag it Judged and
must answer "what is tested?" from `statement` and the tolerances.

## Migration

- `0.2` → `0.3`: `claim` becomes a block. A legacy string `claim: "..."` is read
  as `claim.statement` with `plain` absent and `as_stated` absent; the
  validator warns that the source anchor is missing. No existing claim is
  invalidated; they degrade to "statement-only, unanchored," which research tier
  already permits.

## Out of scope

- **Automated quote↔statement equivalence checking.** Whether `statement`
  faithfully paraphrases `as_stated.quote` is a `Judged` act; it belongs to a
  review event, not the structural validator (consistent with §3 invariant 2:
  synthesis introduces no new judgment).
- **Rich source spans** (byte offsets, DOIs-with-anchors). `locator` stays a free
  string; a precise `SourceSpan` type is a later concern.

## Changing this document

This is a proposal. If accepted it implies a `0.3` schema bump, a `workflow/GRAMMAR.md`
principle-1 amendment (the title/statement/plain split is the concrete mechanism for
"prose is the docstring": `statement` is the docstring that must be redundant with
structure; `plain` is the comment that need not be), two new anti-pattern entries,
and the migration note above.
