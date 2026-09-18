# EVIDENT

**Scientific test-driven development for computational claims.**

Before treating a computational result as established, write down the claim, how it
could fail, and the evidence path that would test it. Replay that path when the work
changes. Keep the final judgment human.

EVIDENT is for scientific software you did not fully write or inspect — increasingly,
software made with AI. It does not ask whether the code “looks right.” It asks:

> What is being claimed? What observation supports it? Under what conditions? What
> would falsify it? Who accepted the remaining uncertainty?

## The thin core

A claim lives in `evident.yaml`. It has a stable identity, a stated scope, an executable
oracle and tolerance, a command and artifact, plus assumptions and known failure modes.
The manifest contract is in [`workflow/SCHEMA.md`](workflow/SCHEMA.md).

`evident-agent replay` runs the cited procedure and records an observation. The
[`typed-trust/`](typed-trust) engine then renders a deterministic TrustReport from the
manifest, observations, and review events. It does not ask a model for a verdict.

```text
claim + oracle + tolerance ──► replayable observation ──► human review ──► TrustReport
                                  (what happened)          (what it means)
```

That distinction is deliberate:

- **Verified** means a named procedure produced a recorded observation.
- **Judged** means a person interpreted evidence and gave reasons.
- **Absent** means evidence was sought and not found.

These are different kinds of statements. EVIDENT keeps them separate so an observation,
a model suggestion, and a scientific conclusion cannot quietly become the same thing.

## Start here

Read the one-page [overview](docs/OVERVIEW.md), then run the worked examples in
[`evident-agent/EXAMPLES.md`](evident-agent/EXAMPLES.md). They show the practical loop:
author a small claim manifest, replay it, inspect the evidence, and ask why a claim
should be believed.

The repository contains:

- [`workflow/`](workflow) — the claim-manifest contract and validator.
- [`typed-trust/`](typed-trust) — the deterministic trust engine and read-only MCP server.
- [`evident-agent/`](evident-agent) — replay, extraction, review, and agent-facing tools.
- [`cases/`](cases) and [`experiments/`](experiments) — applications and evaluation work.

## What it is not

EVIDENT is not a trust score, a replacement for scientific judgment, or a way to make
weak evidence strong. An oracle can be wrong; a passing replay does not establish external
validity; and the framework makes those limits visible instead of hiding them behind green
checks.

It should remain small. Claims with consequential, replayable evidence may warrant richer
provenance, challenge history, and scheduling; ordinary claims should feel little heavier
than a serious test.

## Status

Early but real: the manifest validator, `typed-trust` engine, `evident-agent` CLI, and
Claude/Codex driver exist and are tested. The human-only review boundary is specified but
not fully enforced yet; closing that gap is prerequisite work for autonomous scheduling.
See [`docs/concepts/typed-trust.md`](docs/concepts/typed-trust.md) §14 and the proposed
[`EVIDENT loop`](docs/proposals/evident-loop.md).

License: MIT.
