# EVIDENT examples

Worked examples for every workflow. New to EVIDENT? Read [`../OVERVIEW.md`](../OVERVIEW.md)
first.

Each example is tagged:

- 🟢 **runnable as-is** — uses a fixture in this repo; copy-paste and it works.
- 🧩 **template** — substitute your own paths (`<...>`); shown for shape, not copy-paste.
- 🐳 needs Docker + the replay image · 🔑 needs `ANTHROPIC_API_KEY` · 🤖 needs an
  authenticated `claude`/`codex` CLI · ⌨️ interactive.

**Setup.** Install once (see [`README.md`](README.md)): `pip install -e .` in this dir, and
`cargo build --release` in `../typed-trust/`. The 🟢 examples below **assume one shell
session** with these two variables set, run from the **repo root** (`evident/`):

```bash
TT=typed-trust/target/release/typed-trust     # the engine binary
F=evident-agent/tests/fixtures                  # fixtures the 🟢 examples use
```

CLI examples (sections A–B) run from the repo root against its own fixtures. The **driver**
(section C) runs against *any* corpus, so you `cd` there — and the `typed-trust`/agent
binaries must be on your `PATH` (or use absolute paths).

---

## A. The trust report — the engine

### 1. A claim with no evidence yet → `Absent` 🟢

The engine turns a claim manifest into a deterministic report. With nothing backing the
claim, EVIDENT says so plainly rather than guessing:

```bash
$TT --format md "$F/adversarial/ball_challenge/evident.yaml"
```
```
**Claim:** `ball-electrostatic-synthetic-challenge`   **Status:** Current ✓
### electrostatic_relative_error < 0.02
- **Result:** Not assessed — no observation in evidence for this criterion
- **Tolerance:** `electrostatic_relative_error < 0.02`
```

The claim and its decision rule are explicit, and the report states that no evidence backs
it yet (the observation is `Absent`). Scenario 2 produces that evidence.

### 2. The same claim with an observation → `Verified` 🟢

`replay` (next section) records an observation in a **sidecar** —
`claim_id → {value, date, commit}`. Render *with* that sidecar and the criterion is now
assessed against its tolerance. To see it without Docker, hand-write the sidecar `replay`
would have written:

```bash
echo '{"ball-electrostatic-synthetic-challenge":{"value":0.013,"date":"2026-06-06","commit":"abc1234"}}' > /tmp/last_verified.json
$TT --format md --last-verified-sidecar /tmp/last_verified.json "$F/adversarial/ball_challenge/evident.yaml"
```
```
### electrostatic_relative_error < 0.02
- **Result:** Pass ✓
- **Observed value:** `0.013`
- **Tolerance:** `electrostatic_relative_error < 0.02`
```

That's the whole point in two renders: a claim is `Absent` until a procedure produces a
**Verified** observation, and the report never fakes the gap. (`--format` also takes
`json`, `html`, `mermaid`.)

---

## B. Gathering evidence — the `evident-agent` CLI

### 3. Replay a measurement claim

**3a — preview the procedure** 🟢 (`--dry-run` prints the command, runs nothing):
```bash
python3 -m evident_agent.cli replay \
  --manifest "$F/adversarial/ball_challenge/evident.yaml" \
  --claim ball-electrostatic-synthetic-challenge --dry-run
```
```
[1/1] ball-electrostatic-synthetic-challenge
  cmd:      docker run … proteon-evident:latest replay ball-electrostatic-synthetic-challenge
(--dry-run) sidecar NOT written; 1 claims would be processed
```

**3b — actually run it** 🐳🧩 — runs the claim's procedure in Docker, scores the artifact,
writes the `last_verified.json` you faked in Scenario 2, then re-renders (so the report
shows `Pass ✓` for real):
```bash
python3 -m evident_agent.cli replay --manifest <your-manifest> --claim <your-claim> --render md
```
A complete real replay — building the image and all — is scripted in
[`examples/proteon_sasa_release.sh`](examples/proteon_sasa_release.sh).

### 4. Extract metadata claims (deterministic) 🟢

No model — reads `pyproject.toml` / `Cargo.toml` / `package.json` and emits
`metadata_compatibility` claims (the declaration *is* the claim):
```bash
python3 -m evident_agent.cli extract-metadata \
  --repo "$F/extract/metadata/pyproject_repo" --output-dir /tmp/ex-meta
head -12 /tmp/ex-meta/evident.yaml
```
```
extracted 3 metadata claim(s) from .../pyproject_repo
# evident.yaml:
- id: pyproject_repo-pyproject-requires-python
  kind: metadata_compatibility
  tier: research
  claim: pyproject.toml declares requires-python = '>=3.10'
```
Also try `cargo_repo`, `package_json_repo`, `multi_file_repo`, `uv_workspace_repo`.

### 5. Draft claims from a repo

**5a — preview** 🟢 (`--dry-run`: the walker runs, the model does not):
```bash
python3 -m evident_agent.cli extract \
  --repo "$F/extract/repo/clean_repo" --output-dir /tmp/ex-repo --dry-run
ls /tmp/ex-repo      # → EXTRACTION.md  dry_run.json  (what WOULD be sent to the model)
```

**5b — real extraction** 🔑🧩 — calls the model, validates each candidate, writes a draft
manifest of `tier:research` claims (drafts, never facts — they await curation):
```bash
export ANTHROPIC_API_KEY=sk-...
python3 -m evident_agent.cli extract --repo ./some-repo --output-dir /tmp/drafts
# → /tmp/drafts/evident.yaml, EXTRACTION.md, raw_extraction.json
```

### 6. Draft claims from a paper

Same shape with `--paper`. 🟢 dry-run / 🔑 real (PDF papers also need `pdftotext`):
```bash
python3 -m evident_agent.cli extract \
  --paper "$F/extract/paper/clear_paper.md" --output-dir /tmp/ex-paper --dry-run
```

### 7. Review a claim (adversarial panel) 🔑🧩

One or more models Endorse / Dissent / Challenge a claim; verdicts (each a `Judged`
derivation) aggregate deterministically. `--no-api` runs the wiring without a model call
(useful to see the flow); drop it and add `--model ...` for a real review:
```bash
python3 -m evident_agent.cli review --manifest <manifest> --claim <claim> --no-api
python3 -m evident_agent.cli review --manifest <manifest> --claim <claim> --model claude-opus-4-7 --record
```

### 8. Curate extracted drafts ⌨️🧩

Walk the `tier:research` drafts and accept / drop / rephrase / promote:
```bash
python3 -m evident_agent.cli review-extracted --manifest /tmp/drafts/evident.yaml --curator "you <orcid:...>"
python3 -m evident_agent.cli promote --manifest <m> --claim <c> --to-tier ci --rationale "..."
python3 -m evident_agent.cli drop    --manifest <m> --claim <c>
```

---

## C. Driving an agent — the integrated experience

Hand the corpus to a terminal agent that reads the trust graph and runs procedures on
request, instead of running the steps by hand. 🤖⌨️ — needs an authenticated `claude` or
`codex` CLI on your `PATH`. **Run from your corpus root** (the evident repo root works — it
has `cases/` and the fixtures).

### 9. Ask "why should I trust this?" (no docker, no extraction)
```bash
cd /path/to/corpus
evident-agent drive --model claude
```
> *"List the claims and tell me which have a current verified observation."*
> *"Why should I trust `ball-electrostatic-synthetic-challenge`? What would falsify it?"*

The agent reads the graph first and tags every answer: **Verified** (a tool produced it),
**Judged** (its interpretation), **Absent** (searched, not found), **Unknown** (not
checked), **Inconclusive** (a tool errored or returned a dry-run).

Note: the driver *is* a model (that's the `--model`). The capability flags gate what its
**tools** may do, not its reasoning — without `--allow-docker`/`--allow-extract`, `replay`
returns a dry-run (reported as **Inconclusive**) and `extract` makes no model call.

### 10. Let its tools run procedures 🐳🔑
```bash
evident-agent drive --model claude --allow-docker --allow-extract
```
> *"Replay `<claim>` if there's no current observation, then tell me if it holds."*

### 11. Same corpus, Codex instead
```bash
evident-agent drive --model codex
```
The tools and prompt are the same; the runtime differs in how it authenticates and
**approves tool calls** (Codex prompts you before each one — its safety gate).

---

## D. Use the MCP servers in another client 🧩

The driver wires both servers for you; you can also register them in any MCP client — e.g.
a Claude Code `.mcp.json`:
```json
{ "mcpServers": {
  "typed-trust":  { "command": "typed-trust-mcp",  "args": ["--allow-manifest", "/path/to/corpus"] },
  "evident-exec": { "command": "evident-agent-mcp", "args": ["--allow-root",     "/path/to/corpus"] }
}}
```
Tool schemas, the allow-list, the capability flags, and the error model:
[`docs/mcp-server.md`](docs/mcp-server.md).

---

## Where output lands

| Step | Writes |
|---|---|
| `replay` | `<manifest-dir>/last_verified.json` (observations) |
| `extract` / `extract-metadata` | `<output-dir>/evident.yaml`, `EXTRACTION.md` (+ `raw_extraction.json` for model extract) |
| `extract --dry-run` | `<output-dir>/EXTRACTION.md`, `dry_run.json` (no model call) |
| `review` | a review-events sidecar |
| `drive` | nothing persistent of its own — it drives the tools above |
