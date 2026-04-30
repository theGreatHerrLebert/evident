# EVIDENT vs unit tests

A common objection: "this is just an expensive unit test." The objection
is partly right and worth engaging seriously.

---

## What the objection gets right

At the **runtime layer**, EVIDENT does not invent new test technology.

`evident replay` runs `evidence.command`, captures pass/fail, writes a
date. That is a test runner. The claim's `metric: relative_error,
op: <, value: 0.005` translates one-to-one into `assert rel < 0.005`.
Pytest already does this. CI already runs it.

If a project's audience is **only its own authors and CI**, EVIDENT
really is overhead the project does not need. Concede that. Do not
defend the framework on terrain where the colleague is right.

---

## What the objection misses

Unit tests stop at the test runner. EVIDENT continues. Six things unit
tests structurally cannot do:

### 1. Survive outside the source tree

A unit test asserts privately. A claim is **published** — visible to
consumers, peer reviewers, downstream library users who never read the
project's `tests/` directory. Different audience, different
commitment.

### 2. Compose across projects

> Find every library claiming TM-score parity within 0.5% on a
> ≥1000-structure corpus.

That query works against an EVIDENT manifest. It does not work against
any pile of pytest files no matter how cleanly written. Cross-project
query is the killer feature; pure unit-test ecosystems cannot deliver
it because the assertions are not structured.

### 3. Carry provenance as first-class fields

`pinned_versions: {OpenMM: 8.1.2, proteon: a48c0b9}` plus
`last_verified: {commit, date, corpus_sha}` survives a year.
"Reconstruct what OpenMM version this test last passed against in
2024" is forensic archaeology against pytest history. It is a single
JSON read against a sidecar.

### 4. Carry peer-review attestation

`provenance: peer-reviewed` plus a named reviewer with ORCID is an
epistemic object pytest cannot produce. Peer review is what makes
scientific claims credible; CI passing is what makes code work. They
are not the same thing.

### 5. Surface gaps as queryable entities

`kind: reference` documents "we do not have an oracle for X yet." A
unit test cannot say that — at best, a `# TODO` rots in a comment. A
documented gap is reviewable, budget-able, and queryable. An undocumented
gap is forgotten.

### 6. Force tolerance honesty by construction

"Agrees with OpenMM" passes lint as prose and passes CI as long as the
underlying (unspecified) test is green. The schema makes it a
**structural error** to claim agreement without naming the metric, the
operator, the value, the oracle, and the corpus. Unit tests are happy
with vibes; assertions can pin numbers, but tests do not *force* you
to.

---

## The frame

EVIDENT is **a test runtime + a public contract + an audit format**.
The runtime is unit-test-shaped because that is what test runners do.
The contract and the audit format are what unit tests do not have and
cannot get cheaply.

The "expensive" part of the framework is the contract layer. Whether
that expense is justified depends entirely on the audience.

---

## When EVIDENT is overkill

Be honest about the cost-benefit.

- **Internal-only library, no external consumers.** Unit tests are
  enough. The contract layer pays for itself only when someone
  outside the project will read it.
- **Hobby script with no claims of correctness.** Same.
- **Pure application code without numerical assertions.** EVIDENT is
  designed around quantitative claims (`metric/op/value`); a CRUD
  service has no analogue and should not adopt it.
- **A project where peer review will never happen.** `provenance:
  peer-reviewed` is dead weight if no reviewer is going to sign off.

For these cases, the colleague is right. Do not adopt EVIDENT to
adopt EVIDENT.

---

## When EVIDENT pays

- Scientific or numerical libraries claiming agreement with external
  oracles.
- Projects with downstream consumers who need to audit claims without
  reading the source.
- Code aimed at peer review, paper supplements, regulatory
  submissions, or external certification.
- Any project where "trust this for your work" is part of the
  value proposition.

The framework cost is small relative to the cost of unverifiable
claims in this audience: papers retracted because supplementary code
did not reproduce, downstream consumers misled by handwaved
tolerances, "agrees with X" assertions that nobody can audit.

---

## One-liner for the conversation

> The runtime is unit-test-shaped. The framework is not the runtime —
> it is what stays after the runtime exits. A unit test ends at green;
> a claim starts there.

---

## Related reading

- [`concepts/ai-assisted-coding.md`](ai-assisted-coding.md) — why the
  same forcing function matters more in an AI-assisted-coding world,
  with the four scientific-method pillars (skepticism, falsifiability,
  reproducibility, provenance) the schema operationalises.
- [`workflow/GRAMMAR.md`](../workflow/GRAMMAR.md) — the schema's
  rules and rationale, including why structured tolerances are not
  optional.
