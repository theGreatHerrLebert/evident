# EVIDENT

EVIDENT is a claim-based evidence workflow for AI-assisted scientific
software. It does not ask whether code "looks right"; it asks what claim is
being made, what evidence supports it, and what would falsify it.

It starts from a simple question:

> **How do we justify trust in computational results when we did not fully author or inspect the code that produced them?**

---

## Core Workflow

EVIDENT shifts from:

- “I trust this because I understand it”

to:

- **“I trust this claim because it has sufficient evidence, understanding, or guarantees.”**

The workflow is:

```text
claim
  -> trust strategy
  -> oracle/reference
  -> tolerance or decision rule
  -> reproducible command
  -> artifact
  -> assumptions and failure modes
```

Claims may be about different layers:

- **Implementation claim** — a component behaves according to a specification.
- **Pipeline claim** — a workflow transforms inputs into outputs reproducibly.
- **Scientific claim** — outputs support an interpretation under stated assumptions.

Evidence for one layer does not automatically validate the next.

---

## Trust Strategies

Trust in a computational component can be established through three complementary mechanisms:

- **Understanding** — explaining why it should work  
- **Validation** — showing that it behaves correctly  
- **Proof** — guaranteeing properties under defined assumptions  

In practice, most systems rely on a combination of these.

> **The less we understand, the stronger the validation must be.**

---

## What This Repository Provides

This is not a fixed standard. It is a small framework for making computational
trust claims explicit and reviewable:

- **Manifest** → claim, evidence, command, artifact, assumptions, failure modes
- **Workflow blueprint** → lightweight manifest checks plus case-specific replay
- **Cases** → real examples of release-grade and research-grade evidence
- **Patterns** → repeatable evidence structures
- **Anti-patterns** → common ways evidence becomes misleading
- **Rules** → actionable guidelines
- **Concepts and checklists** → shared vocabulary for review

---

## Structure

```text
evident.yaml      example claim manifest
workflow/         Docker and manifest validation blueprint
cases/            real-world examples
patterns/         reusable evidence structures
anti-patterns/    misleading evidence patterns
rules/            actionable guidelines
concepts/         definitions and mental models
checklist/        practical review prompts
```

---

## Design Principles

- **Modular** — contributions don’t need to be complete
- **Practical** — rules should be actionable
- **Transparent** — assumptions and limitations are explicit
- **Debatable** — disagreement is expected and encouraged

---

## Why This Matters

Scientific computing already has trust problems: legacy code, copied
algorithms, opaque dependencies, numerical convention gaps, and tools used
beyond the user's understanding. AI-assisted development makes those problems
easier to encounter because complex systems can now be produced faster than
they can be fully inspected.

If we cannot justify how a computational system behaves, we cannot defend the
conclusions drawn from it.

This repository explores how to respond to that shift.

---

## Contributing

Start small:

- Add a claim
- Tighten an oracle or tolerance
- Document a failure mode
- Improve a replay command

You don’t need to be complete — partial insights are valuable.

---

## Status

Early stage. Expect rough edges, open questions, and evolving definitions.
