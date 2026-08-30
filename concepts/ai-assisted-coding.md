# EVIDENT as an AI-assisted coding layer

A position on what EVIDENT is for, beyond reproducibility manifests for
scientific libraries. Discoverable by either humans or AI authors;
useful to both.

---

## Position

**EVIDENT's claim-with-tolerances pattern is a forcing function. The
forcing function is valuable in proportion to the author's tendency
to handwave under confident-sounding prose. Both LLMs and humans do
this; LLMs do it more often, and at higher volume.**

The schema is hostile to vibes. "This matches OpenMM" without a
`metric / op / value` triple plus a reproducible command does not pass
the validator. That collapses the most common scientific-code
failure mode by construction — confident-sounding agreement with no
defensible threshold.

Critically, the pattern is **not AI-only**. The forcing function works
on humans too; the same audit story (PhD examiner, peer reviewer,
downstream consumer) applies regardless of who wrote the code. A
framework that locks out human review because it is AI-shaped fails;
EVIDENT works for both at the same forcing-function level.

---

## Foundation: the scientific method

The pattern is durable because it is not new. It is the epistemology
of empirical science, applied to code:

- **Default skepticism.** A claim is not believed because the author
  is confident, persuasive, or trusted. It is believed in proportion
  to the verifiable evidence behind it. The validator instantiates
  this directly: a claim without `metric / op / value` does not pass.
  No quantity of confident prose changes the verdict.
- **Falsifiability.** A claim must be specific enough to be wrong.
  "Agrees with OpenMM" is not falsifiable. "Median relative error
  < 0.5% on 1000 PDBs at NoCutoff" is. The schema requires the
  second form.
- **Reproducibility.** Evidence must be re-executable by a third
  party from the cited artifacts. `evidence.command` is the
  protocol; `pinned_versions` is the materials list. A claim
  whose evidence cannot be re-run by an outsider is not evidence,
  it is testimony.
- **Provenance.** Who ran what, against what, when. The
  `last_verified.{commit, date, value, corpus_sha}` block is the
  audit trail. Without it, a green test today is not evidence
  next year — it is a memory.

This is why the pattern works for AI-authored and human-authored code
symmetrically. The scientific method does not care who wrote the
claim — it cares whether the evidence stands. EVIDENT inherits that
property by construction. The framework's durability is bound not to
a particular tooling era (LLMs, CI providers, programming languages)
but to the older and more robust epistemology underneath.

A useful test for any future EVIDENT extension is to ask: does it
strengthen one of these four pillars (skepticism, falsifiability,
reproducibility, provenance), or does it sit alongside them as
convenience? Convenience is fine but should not be confused with
the foundation.

---

## Why it fits AI-assisted coding

Four properties make EVIDENT particularly well suited as a layer
between AI authors and a reviewable codebase.

### 1. Hostile to vibes by construction

Free-prose claims (`# this matches biopython`) are LLM-friendly to
write but human-hostile to verify. Structured claims with explicit
tolerances are LLM-equally-easy to write — the schema constrains the
shape — but human-easy to verify, because the assertion is a
finite triple, not a paragraph.

### 2. Claims are AI-writable as contracts, not comments

LLMs round-trip reliably on contracts (YAML with a validator) and
unreliably on comments (free prose with no gate). The validator is
the gate that makes claim authoring AI-tractable: an LLM can produce
a claim file that validates, and the validation result is binary.

### 3. `kind: reference` is the LLM "I'm not sure" escape hatch

Without a structured place for "we don't have an oracle for this
yet," an AI's incomplete knowledge surfaces as hallucinated
certainty in code or comments. With it, the absence becomes a
queryable manifest entry — preserved across sessions, visible to
reviewers, deletable when the gap closes.

### 4. The audit unit shifts from diff to claim

This is the conceptual core. Today's review-the-diff pattern does
not scale to AI-volume code production: the AI can produce more
diff than humans can review. EVIDENT shifts the audit boundary to
"did this satisfy these claims" — finite, structured, queryable.

A human reviewer signing off on a claim diff is doing strictly
more focused work than reviewing a 2000-line code diff for a
similar effect on numerical correctness.

---

## Why it is not AI-only

The forcing function predates AI and survives without it. Three
reasons humans benefit equally:

- **Writing the claim surfaces vagueness** the author would
  otherwise leave implicit. A human committing "agrees with
  OpenMM within 0.5%" had to think through which corpus, which
  cutoff convention, which tolerance metric. That thinking
  happened because the schema demanded it, not because an LLM
  was involved.
- **Claims outlive their authors.** A claim written by an LLM and
  signed off by a human reviewer is reviewable by the next human
  six months later in the same way. Provenance is orthogonal to
  the contract.
- **Audit pattern is identical.** PhD examiners, peer reviewers,
  and downstream consumers care about the same structured
  defensibility regardless of who wrote the code.

A framework optimised for AI-only authoring would lose this. EVIDENT
should not.

---

## What exists

The roadmap this essay originally carried has largely been built:

- **Replay loop** — `evident-agent replay` re-executes `evidence.command`
  in Docker and writes `last_verified.json`; `typed-trust` turns the
  observation into a Pass/Fail criterion.
- **Claim-aware authoring** — `evident-agent extract-repo` /
  `extract-paper` draft claims behind a source-span validator;
  `evident-agent curate` gates promotion.
- **Cross-project queries** — the `typed-trust` read MCP server's
  `query_claims`.

Still open: a slim adoption story (`pip install`, `evident init`) and
richer trust-strategy vocabulary — note that `trust_strategy` is a
*closed* vocabulary per `workflow/GRAMMAR.md` and changes only with a
spec bump, not via `vocabularies`.

---

## What hardening does not solve

- **Bad claims are still bad claims.** A claim with a tolerance set
  too wide passes the validator and is a comfortable lie. The
  schema cannot tell. Peer review of the claim itself, not just the
  validator's verdict, is the only check on this.
- **Replay does not prove the claim is the right claim.** A green
  `last_verified` says the cited assertion held; it does not say the
  assertion was the right thing to assert in the first place.
- **AI authoring does not absorb domain expertise.** An LLM can
  draft a claim file from a test. Whether the tolerance, oracle,
  and corpus choices are appropriate is a domain judgment the
  schema cannot encode.

These are limits of the pattern, not arguments against it. Every
verification framework has them.

---

## Open questions

Worth debating as the framework matures:

- `last_verified` is a sidecar (`last_verified.json`, written by the
  runner); inline `last_verified` in the YAML remains accepted for
  hand-authored fixtures. Decided.
- Is `kind: reference` the right name for "documented gap"? Some
  reviewers read "reference" as "this is a reference / gold
  standard." A rename to `kind: gap` or `kind: deferred` would
  be more honest about the intent.
- What is the right rhythm for claim-aware AI workflows? "Write
  a claim, then write code to satisfy it" (TDD-shape) vs "write
  code, then extract a claim from the resulting tests"
  (after-the-fact) vs both, gated by tier?
- How does this compose with property-based testing harnesses
  (Hypothesis, QuickCheck)? A property-based generator IS a
  claim, in some sense; the manifest could absorb it as
  `evidence.command: hypothesis run --property X`.

---

## One-line summary for the tooling roadmap

**Build the replay loop first; everything else is interesting but
swallowed by claim rot if `last_verified` is null.**

The deeper one-line summary, for the framework as a whole:

**EVIDENT is the scientific method applied to code — don't believe
by default; believe based on verifiable evidence — and that
foundation is what makes the pattern durable across the AI
transition rather than dependent on it.**
