# EVIDENT

**Typed trust for scientific software you did not fully write or inspect.**

EVIDENT starts from one question:

> How do we justify trust in a computational result when nobody has read all
> of the code that produced it?

Its answer is not "look harder at the code." It is: state the **claim**, bind
it to a **falsifiable check** (oracle + tolerance + replayable command), and
record **how every assertion was established** — as a type, so a fact and an
interpretation can never be confused.

New here? Read the one-page [`OVERVIEW.md`](OVERVIEW.md), then run the worked
examples in [`evident-agent/EXAMPLES.md`](evident-agent/EXAMPLES.md).

---

## The core

Three things, and everything else in this repository serves them.

**1. A claim manifest** (`evident.yaml`, spec in [`workflow/SCHEMA.md`](workflow/SCHEMA.md)
and [`workflow/GRAMMAR.md`](workflow/GRAMMAR.md)).
Every claim carries a trust strategy, an oracle, a structured tolerance
(`metric / op / value`), a reproducible command, an artifact, assumptions and
failure modes. Above research tier, prose tolerances are rejected by the
validator (`workflow/validate_manifest.py`).

**2. A deterministic trust engine** — [`typed-trust/`](typed-trust)
(spec in [`concepts/typed-trust.md`](concepts/typed-trust.md)).
Every value is `Attested<T>` with a derivation of exactly one kind:

- **Verified** — a named procedure ran and produced this observation.
- **Judged** — a human's interpretation; carries a rationale; never rendered as fact. A model's view is only ever *Proposed* until a person adopts it.
- **Absent** — sought and not found; a first-class result, not a blank.

Review is a graph of events (endorse / dissent / challenge / supersede) over
claims, and a claim's status (`Current` / `Contested` / `Superseded`) is
*computed* from that graph at synthesis time. **Synthesis calls no model.**

**3. A replay-and-review agent** — [`evident-agent/`](evident-agent)
(usage in [`evident-agent/README.md`](evident-agent/README.md)).
Runs each claim's cited command in Docker and writes the observation back as a
`last_verified.json` sidecar; drafts claims from repos and papers with a
source-span validator that refuses claims the source does not state; records
adversarial reviews; and exposes all of it over MCP so a Claude or Codex
session (`evident-agent drive`, prompt in [`EVIDENT_DRIVER.md`](EVIDENT_DRIVER.md))
can answer *"why should I trust claim X?"* from evidence, with every sentence
tagged by how it was established.

```text
  evident.yaml ──► evident-agent replay ──► last_verified.json ─┐
       │           evident-agent review ──► review_events.json ─┤
       │                                                        ▼
       └──────────────────────────────────────────────► typed-trust ──► TrustReport
                                                        (deterministic)
```

---

## Layout

```text
OVERVIEW.md            one-page introduction
EVIDENT_DRIVER.md      the agent's operating contract (status vocabulary, hard rules)
evident.yaml           this repo's own (small) example manifest
workflow/              manifest SCHEMA, GRAMMAR, and the validator
typed-trust/           Rust engine + read-only MCP server
evident-agent/         Python CLI (replay, extract, review, curate, drive) + exec MCP server
concepts/              typed-trust spec, shared vocabulary, positioning essays
patterns/ anti-patterns/ rules/ checklist/   short reference material for claim review
cases/                 real consumer projects as submodules + interpreted summaries
experiments/           the extraction-rate experiment (pre-registered; in progress)
design-history/        drafts that shaped shipped code; not normative
latex/                 the paper draft
```

---

## Claim layers and trust strategies

Claims live at one of three layers — **implementation** (a component matches a
spec), **pipeline** (inputs transform to outputs reproducibly), **scientific**
(outputs support an interpretation under assumptions) — and evidence at one
layer never automatically validates the next.

Trust in a component comes from **understanding** (why it should work),
**validation** (showing it does), or **proof** (guaranteeing it), in
combination. *The less we understand, the stronger the validation must be.*

---

## What EVIDENT does not do

It does not make weak evidence strong. It can record an oracle without knowing
the oracle is right, require a tolerance without judging whether it is
scientifically meaningful, and it deliberately refuses to emit an aggregate
"trust score" — it makes the inputs to that judgment explicit and reviewable.

---

## Status

Early but real. The engine, the agent, the driver and the validator exist and
are tested (`cargo test` in `typed-trust/`, `pytest` in `evident-agent/`).
Open work, in order: run the pre-registered extraction-rate experiment to
completion; close the remaining gaps between the typed-trust spec and its
enforcement (listed in `concepts/typed-trust.md` §14); land the
[`claim.statement` proposal](EVIDENT_PROPOSAL_claim-statement_DRAFT.md).

License: MIT.
