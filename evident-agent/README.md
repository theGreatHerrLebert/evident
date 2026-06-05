# evident-agent

The agent layer for [EVIDENT](../README.md). It does two jobs:

1. **Populates the trust corpus** — runs cited procedures (`replay`), extracts draft
   claims from repos and papers (`extract*`), and records review events, writing the
   sidecars that the `typed-trust` engine renders into a TrustReport.
2. **Drives an agent over that corpus** — `evident-agent drive` launches a terminal
   agent (Claude Code or Codex) wired to the EVIDENT tool belt, so you can ask *"why
   should I trust this claim?"* and have it answered from evidence, not vibes.

The design principle behind the driver: **MCP is the neutral waist.** Every tool lives
behind an MCP server, so the model runtime is a swappable config detail — the same tool
belt and the same prompt drive either Claude or Codex.

```
  claude  ⟷  codex            pick one at launch (config only)
        │   both are MCP clients
  ┌─────┴──────────────────────────────────────┐
  │  MCP TOOL BELT                               │
  │   typed-trust-mcp   READ — query the graph   │
  │   evident-agent-mcp EXEC — run procedures     │
  └─────┬──────────────────────────────────────┘
        ▼   evident-agent CLI  +  typed-trust engine
```

---

## Install

```bash
# the Python agent (editable)
cd evident-agent
pip install -e .

# the Rust read server + engine (needed by `drive` and `replay --render`)
cd ../typed-trust
cargo build --release        # produces typed-trust and typed-trust-mcp
```

`drive` finds `typed-trust-mcp` on `PATH` or in the sibling
`typed-trust/target/{release,debug}/` build, and fails with a clear message if it is
missing. `extract*` and `--allow-extract` need an `ANTHROPIC_API_KEY`.

---

## Quick start — drive an agent

From the root of an EVIDENT corpus (a directory containing one or more `evident.yaml`
manifests):

```bash
# read-only auditor (safe default: replay/extract run dry)
evident-agent drive --model claude
evident-agent drive --model codex

# enable real execution (replay runs docker; extract calls the model)
evident-agent drive --model claude --allow-docker --allow-extract
```

Then talk to it:

> *"List the claims in this corpus and tell me which have a current verified observation."*
> *"Why should I trust `ball-electrostatic-synthetic-challenge`? Replay it if there's no current observation."*
> *"Draft candidate claims from ./some-repo."*

The agent reads the trust graph first, runs procedures only when reading isn't enough,
and labels everything by how it was established (Verified / Judged / Absent / Unknown /
Inconclusive). It will **not** assert that a claim holds without a tool result behind it.

### What `drive` actually does

| | |
|---|---|
| `--model` | `claude` or `codex` — the only choice that matters. |
| `--root` | corpus root the servers may access (default: cwd). |
| `--allow-docker` | let `replay` run docker. **Off by default** → replay returns a dry-run. |
| `--allow-extract` | let `extract_*` call the Anthropic API. **Off by default** → extract runs dry. |
| `--driver-prompt` | override the canonical [`EVIDENT_DRIVER.md`](../EVIDENT_DRIVER.md). |

It is **invocation-local** — it writes no persistent global state:

- **Claude**: an ephemeral `--mcp-config` + `--strict-mcp-config` (your user/project MCP
  config does not leak in) + `--append-system-prompt-file`.
- **Codex**: `codex -C <session> --ignore-user-config -c 'mcp_servers...'` with the prompt
  as an ephemeral `AGENTS.md`. It does **not** run `codex mcp add` and never writes to your
  repo. Codex runs the **interactive** TUI, where each MCP tool call goes through its
  approval prompt — the intended human-in-the-loop gate.

### Safety model

Execution capability lives in the **server**, not the model's discretion:

- Without `--allow-docker` / `--allow-extract`, the docker/model tools refuse real
  execution and run dry. A dry-run is reported as **Inconclusive**, never as evidence.
- Tool calls may only touch paths under `--root` (an allow-list, symlink-resolving).
- On Codex, every tool call additionally requires your interactive approval.

---

## The other commands

| Command | What it does |
|---|---|
| `replay` | Run a measurement claim's docker procedure, score the artifact, write the observed value to the sidecar. `--render` then prints the TrustReport. |
| `extract` | Draft `tier:research` claims from a local repo (model call). |
| `extract-metadata` | Deterministically read compatibility claims from `pyproject.toml`/`Cargo.toml`/`package.json` (no model). |
| `review` | Author Endorse / Dissent / Challenge review events on claims. |
| `review-extracted` / `promote` / `drop` / `rephrase` | Curate extracted drafts. |

Run any with `--help` for options. `replay` and the `extract*` family are the same logic
the MCP `replay` / `extract_*` tools wrap, so behavior is identical whether invoked by a
human or by the driver agent.

---

## The MCP servers

- **Read** — `typed-trust-mcp` (Rust, in `../typed-trust`): 11 query tools over the trust
  graph (`list_claims`, `read_report`, `query_observation`, `walk_backing_chain`, …).
- **Exec** — `evident-agent-mcp` (this package): four tools that run procedures
  (`replay`, `extract_repo`, `extract_paper`, `extract_metadata`).

`drive` wires both for you. To register them in another MCP client, or for the tool
schemas, allow-list, and error model, see [`docs/mcp-server.md`](docs/mcp-server.md).

---

## Troubleshooting

- **`typed-trust-mcp not found`** — build it: `cd ../typed-trust && cargo build --release`.
- **Codex cancels every tool call** — you're in headless `codex exec`. Codex routes MCP
  tool calls through an interactive approval prompt; use `drive --model codex` (the TUI),
  where you approve them. This is by design, not a bug.
- **Claude says "Invalid API key"** when a nested launch runs — a stale `ANTHROPIC_API_KEY`
  in the environment overrides your subscription credentials. Unset it for the launch
  (`env -u ANTHROPIC_API_KEY evident-agent drive ...`).
- **`replay` / `extract` returned a dry-run** — the server lacks `--allow-docker` /
  `--allow-extract`. That's the safe default; pass the flag to enable real execution.

---

## Tests

```bash
pip install -e . && python -m pytest
```
