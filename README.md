# Trust in AI-Assisted Scientific Software

Modern software development has changed.

With AI-assisted programming, we can generate complex systems faster than we can fully understand them. This breaks a long-standing assumption in scientific computing: that the person who writes the code understands it well enough to justify its behavior.

This repository is a starting point for a community-driven effort to answer a simple question:

> **How do we justify trust in computational results when we did not fully author or inspect the code that produced them?**

---

## Core Idea

We shift from:

- “I trust this because I understand it”

to:

- **“I trust this because I have sufficient evidence, understanding, or guarantees.”**

---

## Foundations of Trust

Trust in a computational component can be established through three complementary mechanisms:

- **Understanding** — explaining why it should work  
- **Validation** — showing that it behaves correctly  
- **Proof** — guaranteeing properties under defined assumptions  

In practice, most systems rely on a combination of these.

> **The less we understand, the stronger the validation must be.**

---

## What This Repository Provides

This is not a fixed standard. It is a growing collection of:

- **Concepts** → shared vocabulary (e.g. understanding levels, validation types)
- **Rules** → actionable guidelines
- **Patterns** → repeatable solutions
- **Anti-patterns** → common failure modes
- **Checklists** → practical tools for day-to-day use
- **Workflow blueprint** → manifest-driven evidence replay

---

## Structure
/evident.yaml     → example claim manifest
/concepts        → definitions and mental models
/rules           → enforceable guidelines
/patterns        → how to solve recurring problems
/anti-patterns   → how things go wrong
/checklists      → practical evaluation tools
/cases           → real-world examples
/workflow        → Docker and manifest validation blueprint

---

## Design Principles

- **Modular** — contributions don’t need to be complete
- **Practical** — rules should be actionable
- **Transparent** — assumptions and limitations are explicit
- **Debatable** — disagreement is expected and encouraged

---

## Why This Matters

Scientific results depend on computational systems.

If we cannot justify how those systems behave, we cannot defend the conclusions drawn from them.

AI-assisted development increases both capability and risk:
- systems grow faster
- inspection becomes harder
- errors become easier to miss

This repository explores how to respond to that shift.

---

## Contributing

Start small:

- Add a concept
- Propose a rule
- Document a failure case
- Share a validation strategy

You don’t need to be complete — partial insights are valuable.

---

## Status

Early stage. Expect rough edges, open questions, and evolving definitions.
