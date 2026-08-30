# EVIDENT docs

Start with [`OVERVIEW.md`](OVERVIEW.md).

| Directory | What it holds | Normative? |
|---|---|---|
| `concepts/` | [`typed-trust.md`](concepts/typed-trust.md) — the engine spec (types, invariants, status rule, known enforcement gaps in §14); [`README.md`](concepts/README.md) — shared vocabulary (understanding / validation / proof ladders, layers, oracles, provenance); [`not-just-a-unit-test.md`](concepts/not-just-a-unit-test.md) and [`ai-assisted-coding.md`](concepts/ai-assisted-coding.md) — positioning essays | typed-trust.md: yes |
| `reference/` | [`patterns.md`](reference/patterns.md), [`anti-patterns.md`](reference/anti-patterns.md), [`rules.md`](reference/rules.md), [`checklist.md`](reference/checklist.md) — short material for writing and reviewing claims | guidance |
| `proposals/` | schema changes under discussion; currently [`claim-statement.md`](proposals/claim-statement.md) (`claim.statement`, `claim.plain`, `claim.as_stated`) | no, until merged into `workflow/SCHEMA.md` |
| `design-history/` | drafts that shaped shipped code, kept for provenance | no |

The manifest contract itself lives next to its validator in
[`../workflow/`](../workflow/) (`SCHEMA.md`, `GRAMMAR.md`), and the agent's
operating contract next to the agent in
[`../evident-agent/EVIDENT_DRIVER.md`](../evident-agent/EVIDENT_DRIVER.md).
