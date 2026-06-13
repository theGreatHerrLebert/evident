# Proposal: the exact claim is first-class (`claim.statement` + `claim.as_stated`)

Status: DRAFT proposal. Not yet normative. Targets `SCHEMA.md` (manifest surface)
and `GRAMMAR.md` (discipline), with a note on the `concepts/typed-trust.md` seam.

## The hole

The manifest carries one prose field for the claim:

```yaml
claim: >
  A mid-flight GPU proteomics pipeline should be evaluated in layers ...
```

`GRAMMAR.md` principle 1 already says this prose "must be redundant with the
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
claim:
  headline: mcm5s2U counteracts the m6A decoding penalty
  statement: >
    In human HEK293T cells, mcm5s2U at U34 alleviates the m6A-induced increase in
    ribosomal A-site occupancy specifically at the m6A-modified, mcm5s2U-dependent
    codons AAA, AGA, and GAA; loss of mcm5s2U biogenesis (ELP1-KO or CTU2-KO)
    intensifies the pausing, and the increase is reversed by STM2457.
  as_stated:
    quote: "mcm5s2U in tRNA modulates the decoding of m6A-modified codons"
    locator: "Highlights; Fig 4C"
    verbatim: true          # maps to Claim.explicit
```

- **`headline`** — the gloss. Free prose. Never the falsifiable object; renderers
  may show it as the card title but must not treat it as the claim.
- **`statement`** — the exact, fully-scoped proposition. The object that evidence
  supports and challenges target. Its scope qualifiers (system, perturbation,
  magnitude, negative control) MUST be reconstructable from the structured fields
  (`subsystem`, `inputs`, `outputs`, `tolerances`, `pinned_versions`), per GRAMMAR
  principle 1. `statement` projects to typed-trust `Claim.text`.
- **`as_stated`** — the source anchor. `quote` is verbatim source text (or `null`
  when the statement is inferred, not quoted); `locator` names where (figure,
  section, line); `verbatim` projects to `Claim.explicit`. This is what lets a
  reader check that `statement` did not drift from the source. `locator` projects
  to `Claim.source` (a coarse `SourceSpan`).

## Validator rules

- `claim.statement` required at every tier. `claim.headline` optional (defaults to
  a truncation of `statement` for display; the truncation is never authoritative).
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

## Migration

- `0.2` → `0.3`: `claim` becomes a block. A legacy string `claim: "..."` is read
  as `claim.statement` with `headline` absent and `as_stated` absent; the
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

This is a proposal. If accepted it implies a `0.3` schema bump, a `GRAMMAR.md`
principle-1 amendment (the headline/statement split is the concrete mechanism for
"prose is the docstring"), one new anti-pattern file, and the migration note above.
