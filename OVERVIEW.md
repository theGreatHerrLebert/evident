# EVIDENT — a one-page intro

**What it is.** EVIDENT is a framework for **typed trust**: a discipline for justifying
*why a computational result should be believed* — especially when you didn't write or
fully inspect the code that produced it. It doesn't ask "does this look right?" It asks:
*what claim is being made, what evidence supports it, and what would falsify it?*

**The problem it answers.** AI now produces analyses, code, and scientific conclusions
faster than anyone can verify them. The bottleneck has moved from *generation* to
*verification*. EVIDENT is infrastructure for that shift — it moves you from *"I trust
this because I understand it"* to *"I trust this claim because it has sufficient evidence,
understanding, or guarantees."*

**The core idea.** Facts and judgments are not the same thing, so EVIDENT keeps them in
**separate types** and never lets one masquerade as the other. Every assertion records
*how it was established*:

- **Verified** — a specified procedure ran and produced the recorded observation under
  identified conditions (a benchmark, a query, a hash check), so a third party can repeat
  it. (Bit-for-bit reproducibility depends on pinning the environment; EVIDENT records the
  method so the re-run is *possible* and *auditable*.)
- **Judged** — a model or human interpretation. Non-reproducible; carries a rationale;
  *never rendered as fact*.
- **Absent** — looked for, not found. A first-class result, not a blank.

The binding rule: the final trust report is **deterministic — synthesis calls no model.**
The model is pushed to the edges; the thing that issues the verdict only assembles
already-attested evidence.

**The shape (what's in the repo).**

```
  manifest (claim → oracle → tolerance → command → artifact → assumptions)
        │
  evident-agent ──── runs cited procedures (replay/docker), drafts claims (extract),
        │            records reviews. Exposes them as the EXEC MCP server.
  typed-trust ────── the engine: turns manifest + evidence into a TrustReport.
        │            Exposes the graph as the READ MCP server. Deterministic.
  evident-agent drive ── a terminal agent (Claude OR Codex) wired to both MCP servers
                         + a canonical prompt, so you can just *ask* the corpus
                         "why should I trust claim X?" and have it answered from evidence.
```

The driver works because **MCP is the neutral waist**: every tool lives behind an MCP
server, so the model runtime (Claude/Codex) is largely a swappable config detail — the
same tools and prompt drive either engine. (The runtimes still differ in how they
authenticate and approve tool calls; the launcher writes no persistent global state.)

**How you use it.** Two surfaces, two jobs: *author a claim manifest and run procedures to
gather evidence, then render a TrustReport* — or *drive an agent over the corpus* to do
and explain that for you, with every answer tagged by how it was established.

**Status.** Early but real: the framework, the `typed-trust` engine + read server, the
`evident-agent` CLI + exec server, and the Claude/Codex driver all exist and are tested. A
deterministic **orchestrator** (a reproducible runner over the full pipeline) is sketched
as the next direction.

---

New here? Start with worked, copy-pasteable examples for every workflow:
[`evident-agent/EXAMPLES.md`](evident-agent/EXAMPLES.md).
