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

## Hardening roadmap

Five things to build, ranked. The first is the keystone; the rest
are interesting but the rot problem swallows them all if claims are
not actively gated.

### 1. Replay loop (keystone)

Today every claim has `evidence.command` but `last_verified` is
`null` everywhere. Without active replay, claims rot silently —
same problem as having tests without CI.

The framework needs a runner that:

- re-executes `evidence.command` periodically against the cited
  artifact path,
- writes back `last_verified.{date, commit, value, corpus_sha}`,
- surfaces stale-by-N-days claims as a queryable filter.

Once `last_verified` is live, claims stop being aspirational text
and become a continuously-verified contract.

### 2. Claim-aware authoring scaffold

Today writing a claim is 30–90 minutes by hand. An LLM scaffold
constrained by the schema gets that to ~5 minutes. The interface:

```
evident draft --from-test tests/oracle/test_x.py --tier ci
# emits a claim YAML stub the author then refines
```

This is where the AI-assisted coding layer framing has the most
leverage — turning "write a claim for this test" into a reliable
LLM operation with the validator as the gate.

### 3. Cross-project composition

Manifest-to-manifest queries:

```
evident query \
  --capability alignment-tmscore-parity \
  --tolerance 'relative_error < 0.001' \
  --tier release
```

Returns claims across all installed manifests that match. The
schema is structured enough for this; the tooling is not there
yet.

This is where the framework becomes useful beyond a single
project — when a downstream consumer can ask "which library
satisfies my requirement profile" and get a structured answer.

### 4. Richer trust strategies

The current vocab is `[validation, understanding, proof]`. AI-era
extensions: `ai-generated-property-tested`, `human-reviewed`,
`fuzz-validated`, `formally-verified-fragment`. The schema
already extends cleanly via the `vocabularies` block.

The point is not to discriminate against AI-authored claims.
It is to make the provenance of trust queryable so a reviewer
can apply different scrutiny to different categories.

### 5. Slim adoption story

Right now adopting EVIDENT means cloning the framework repo,
vendoring the CLI, and writing a manifest. Lowering this to:

```
pip install evident-cli
evident init
evident validate
```

is the difference between "interesting pattern" and "thing other
projects pick up."

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

- Should `last_verified` be a sidecar (one JSON file keyed by claim
  id, written by the runner) or live inside each claim YAML?
  Sidecar keeps manifests clean; in-YAML makes a single claim
  fully self-describing. The current schema allows both; pick.
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
