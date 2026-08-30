# PR5 plan — Phase 5-i: repo extractor (walker + CLI) — v3

> Stacks on PR4 (#22). Uses the PR4 framing/validator/render package
> unchanged; this PR adds a per-source walker and CLI wiring.
>
> **v2** (`EVIDENT_PHASE5_PR5.codex-review.md`) integrated v1's
> three findings: metadata deferred to PR5b, citation redaction at
> walker (not prompt), dry-run = source audit.
>
> **v3** (`EVIDENT_PHASE5_PR5.codex-review-v2.md`) integrates
> codex's v2 review. Changes from v2:
>
> - **Expanded citation redaction** (P1): adds non-DOI preprint
>   domains (biorxiv, medrxiv, semanticscholar, openreview,
>   aclanthology, papers.nips.cc, ssrn), DOI URLs with `.pdf` /
>   `/full` / query-string suffixes, and conservative inline-
>   citation handling activated **only after** bibliography
>   removal (so README prose mentioning `[1]` stays unless the
>   `## References` section was redacted).
> - **Distinguishes model-rejections from validator-rejections**
>   (P2): the validator's reason enum is a fixed set from PR4;
>   the model's rejection list carries open-ended strings.
>   `roadmap_claim` is a model-level reason, NOT a validator
>   reason — PR5's `future_tense_repo` test asserts 0 claims in
>   the final manifest, not a specific reason string.
> - **Concrete conflict-claim manifest shape** (P2):
>   `conflict_repo` produces two distinct claims with different
>   ids, each carrying its own source_span pointing at the
>   originating file. `EXTRACTION.md` lists the conflict; PR5
>   does NOT pick authority.
> - `marketing_repo` test broadened: verify BOTH
>   (a) model emits 0 claims and (b) model emits marketing-language
>   claims that the validator rejects, both produce empty
>   manifests.
> - `cite_this_repo` bibliography false-positive test added so the
>   "## Citation" / "## How to cite" heading common in
>   research-software READMEs is NOT redacted.

## Scope

The smallest meaningful Phase 5-i slice that exercises PR4 on
realistic repo input. Reads README + CHANGELOG + `docs/`, redacts
citations, asks the model to extract, validates with PR4's
validator, writes through PR4's render.

## What ships

### CLI subcommand

```bash
evident-agent extract --repo <path> --output-dir <dir>
                      [--model claude-opus-4-7]
                      [--dry-run]
```

`--repo` is a local path (working tree). The walker reads text
files from a fixed allow-list (no eval, no exec, no network beyond
the Anthropic call).

`--dry-run`: source-audit mode. **No model call is made; no
claims are extracted or validated.** EXTRACTION.md contains:

- Source id resolution (`github:org/repo@<sha>` or
  `local:<abs-path>@<HEAD-sha>` fallback)
- Included files in priority order with byte counts
- Skipped files with structured reasons
  (`binary`, `too_large`, `not_allowlisted`, `symlink_outside_repo`)
- Detected external citation count + locators (DOIs, arXiv ids,
  paper URLs) — flagged as out-of-scope
- Estimated total input size for the model
- Explicit string `"No model call was made; no claims were extracted or validated."`
- A `dry_run.json` sidecar with the structured audit data

The dry-run mode does **not** write `evident.yaml`. A curator
reading the output dir cannot confuse it with a real negative
extraction.

### `evident_agent/extract/repo.py` — the walker

Two main entry points:

```python
def walk_repo(path: Path, source_id: str | None = None) -> WalkedSource: ...
def assemble_for_model(walked: WalkedSource) -> str: ...
```

`WalkedSource` carries:

```python
@dataclass
class WalkedSource:
    source_id: str
    source_sha: str
    sections: list[SourceSection]
    redactions: list[Redaction]
    skipped: list[SkippedFile]
    notes: list[str]                # human-facing warnings
```

```python
@dataclass
class SourceSection:
    path: str              # repo-relative (README.md, docs/intro.md)
    text: str              # post-redaction text the MODEL sees
    text_raw: str          # pre-redaction text for the audit
    kind: str              # "readme" | "changelog" | "docs"
    truncated: bool
```

```python
@dataclass
class Redaction:
    section_path: str
    span_start: int
    span_end: int
    reason: str            # "external_doi" | "external_arxiv" | "paper_url"
    original: str          # the redacted text (kept for audit)
```

```python
@dataclass
class SkippedFile:
    path: str
    reason: str            # "binary" | "too_large" | "not_allowlisted" | "symlink_outside_repo"
    size_bytes: int | None
```

### Reading order

1. `README.md` (also accepts `README.rst`, `README`, `README.txt`)
2. `CHANGELOG.md` / `RELEASE_NOTES.md`
3. `docs/*.md` (one level deep; deeper subdirs are skipped with
   `not_allowlisted`)

### Citation redaction (codex P1, v3 expanded)

Before any text is offered to the model, the walker scans each
section for the following classes of references and replaces each
match with `[external reference omitted: <kind>]`. Raw text stays
in `text_raw` for the EXTRACTION.md audit.

**DOI references.** `doi:10.xxx/yyy`, `https://doi.org/10.xxx/yyy`,
`dx.doi.org/10.xxx/yyy`, and the same URLs followed by `.pdf`,
`/full`, `/abstract`, `/epdf`, `?query=...`, or trailing
punctuation. Regex skeleton (final form in code, tested
exhaustively):
`\b(?:doi:|https?://(?:dx\.)?doi\.org/)10\.\d{4,9}/[-._;()/:%A-Z0-9]+(?:\.\w+|/\w+|\?\S*)?`

**arXiv references.** `arXiv:NNNN.NNNNN(vN)?`,
`arxiv.org/abs/NNNN.NNNNN(vN)?`, `arxiv.org/pdf/NNNN.NNNNN(vN)?`,
`arxiv.org/pdf/NNNN.NNNNN(vN)?.pdf`. Both old-style
(`arXiv:cs/0301012`) and new-style (`arXiv:2501.12345`).

**Non-DOI preprint domains** (codex v2 add). Any URL whose host
matches any of:

- `biorxiv.org`, `medrxiv.org`, `chemrxiv.org`, `osf.io`,
  `ssrn.com`, `papers.ssrn.com`
- `semanticscholar.org`, `s2-research.org`
- `openreview.net`, `aclanthology.org`, `papers.nips.cc`,
  `proceedings.neurips.cc`, `proceedings.mlr.press`,
  `papers.ssrn.com`
- `pubmed.ncbi.nlm.nih.gov`, `ncbi.nlm.nih.gov/pmc`
- `dl.acm.org`, `ieeexplore.ieee.org`,
  `link.springer.com`, `sciencedirect.com`, `nature.com/articles/`,
  `science.org/doi/`

Hosts and their path segments are matched as a single redaction
unit so a URL followed by query string or `.pdf` is fully captured.

**Bibliography section detection.** A heading whose normalised
text matches one of `references`, `bibliography`, `works cited` is
treated as the start of a bibliography. Everything from that
heading through the next `^#` heading (or EOF) is redacted as
`reason: bibliography`. The raw lines are kept in `walked.redactions`
for audit.

**Tighter than v2:** `citations` is no longer a matched heading
because research-software READMEs commonly use `## Citation` /
`## How to Cite` to tell users how to cite the repo itself.
Codex v2 explicitly flagged this false positive. The
`cite_this_repo` fixture pins the contract.

**Inline-citation handling (post-bibliography only).** v3 adds a
conservative second pass: if a section had a bibliography
redacted, the walker also redacts inline citation markers in
that section's remaining text:

- Numeric: `[1]`, `[12, 13]`, `[1-3]` (only when adjacent to
  surrounding prose, not in code blocks)
- Author-year parenthetical: `(Smith 2024)`, `(Smith et al., 2024)`
- Inline author-year: `Smith et al. (2024)`,
  `Smith and Jones (2023)`

Each becomes `[citation omitted]`. **Inline redaction does NOT
fire if no bibliography was redacted from the section** — this
keeps the redaction conservative, so a README that mentions `[1]`
in some installation-step context doesn't get over-redacted.

**Defense-in-depth.** The system prompt still tells the model not
to extract from cited artifacts; the walker is the load-bearing
enforcement.

**Test coverage targets** (in addition to v2's):

- DOI URL with `.pdf` suffix → fully redacted
- DOI URL with query string → fully redacted
- biorxiv.org / openreview.net / aclanthology.org URLs each
  redacted in isolated tests
- `## Citation` ("how to cite this repo") heading NOT redacted
- `## References` heading triggers inline-citation redaction in
  the same section
- A README with no bibliography but a stray `[1]` is NOT inline-
  redacted (avoid over-redaction)

### File limits

- Per-file cap: 200 KiB of UTF-8 text. Files larger are
  **truncated at 200 KiB** with `truncated=True` and a note
  ``"file truncated at 200 KiB"`` in
  `walked.notes`. (v1 said skip; codex correctly pointed out that
  skipping a long README is worse than reading its top sections.)
- Binary detection: file is treated as binary if either
  - extension is NOT in the allow-list
    (`.md, .rst, .txt, .markdown` for now; pyproject/CI files
    don't exist as candidates in PR5), OR
  - the first 8 KiB contains a NUL byte, OR
  - UTF-8 decode of the first 8 KiB raises `UnicodeDecodeError`.
- Symlinks: rejected if the resolved target lives outside the
  `--repo` root.

### `evident_agent/extract/cli.py` — the wiring

Composes:

```python
walked = repo.walk_repo(args.repo, source_id=...)

if args.dry_run:
    audit.write_dry_run_outputs(walked, args.output_dir)
    return 0

assembled = repo.assemble_for_model(walked)
request = framing.build_request(
    source_text=assembled, source_id=walked.source_id, model=args.model,
)
response = client.messages.create(**request)
result = _process_tool_response(response, walked)
render.write_outputs(result, output_dir=args.output_dir, project=...)
```

`_process_tool_response` walks the model's `submit_extracted_claims`
output. For each claim:

- Runs `validator.validate_tolerance(...)` on each tolerance with
  the claim's `subject_aliases`.
- If a tolerance fails: moves it to the rejections list with the
  validator's `kind` as the structured reason.
- If a claim ends up with zero valid tolerances: drops the claim
  entirely (records every tolerance failure as a rejection).

This is where the validator becomes load-bearing: the model can
emit anything, but only validator-approved tolerances reach the
draft manifest.

### Source-id resolution

Resolves in order:

1. `git config --get remote.origin.url` → `github:owner/repo@<HEAD-sha>`
   (parse SSH and HTTPS forms).
2. `git rev-parse HEAD` exists but no origin URL →
   `local:<basename>@<HEAD-sha>`.
3. No `.git` dir → `local:<basename>@no-git`.

### Fixtures (seven, v3 expanded)

Under `evident-agent/tests/fixtures/extract/repo/`:

- `clean_repo/` — README with one clean extractable empirical claim
  ("Our system sustains throughput greater than 1000 req/sec on the
  production cluster"). Should produce 1 claim.
- `no_claim_repo/` — README is all hedging
  ("performant", "fast", "scalable"). 0 claims.
- `marketing_repo/` — README uses pure marketing language
  ("blazing-fast", "enterprise-ready", "best-in-class") with no
  bounds. **Two test paths** (v3): (a) model emits 0 claims and
  the manifest is empty; (b) model emits 2 marketing-language
  tolerances and the validator rejects them, manifest is empty,
  EXTRACTION.md records both rejections with `kind: missing_value`
  or `missing_comparator`. The point of the fixture is testing
  the validator-as-safety-net, not just the model.
- `future_tense_repo/` — README states a roadmap claim
  ("We will achieve <0.5s latency in v2"). 0 claims in the
  manifest. The plan does NOT assert a specific rejection reason
  string — the model's rejection list carries an open-ended
  reason like `roadmap_claim` or `aspirational_only`, but PR4's
  validator enum does not (yet) include it. The test asserts the
  *outcome* (zero claims in the manifest), not the discriminator.
- `conflict_repo/` — README says "throughput > 1000 req/sec";
  CHANGELOG says "throughput > 5000 req/sec for v0.2." **v3
  concrete manifest shape**: two distinct claims with ids
  `conflict-repo-throughput-readme` and
  `conflict-repo-throughput-changelog`; each carries its own
  `source` and `source_span` pointing at the originating file.
  EXTRACTION.md has a "Conflicts detected" section listing both
  claim ids and a one-line summary. **PR5 does NOT pick
  authority** — the curator decides.
- `cites_paper_repo/` — README has a `## References` section
  citing a paper with DOI + arxiv id + biorxiv URL. Bibliography
  is redacted; inline `[1]` markers in the README body get
  redacted (because the section had a bibliography); 0 claims
  attributed to the cited paper; `EXTRACTION.md` lists all
  three redacted references with their kinds.
- `cite_this_repo/` (v3 add) — README has a `## Citation` /
  `## How to Cite` heading that tells users how to cite the
  repo itself. The heading is NOT redacted (codex v2 explicit
  call). The repo has 1 normal extractable claim in its body
  that should pass through unaffected.

`copied_external_repo` (`source_context` detection) defers to PR5b.

### Tests (~22 expected, v3 expanded)

`test_extract_repo.py` (~14):

- `walk_repo` reads README + CHANGELOG + `docs/*.md` in priority
  order
- `walk_repo` truncates files > 200 KiB and flags truncation
- `walk_repo` skips binary files (extension)
- `walk_repo` skips binary files (NUL byte in first 8 KiB)
- `walk_repo` does NOT follow symlinks outside the repo
- `walk_repo` returns empty source for an empty directory
- `walk_repo` redacts DOI URLs (plain, `.pdf` suffix, query string)
- `walk_repo` redacts arXiv URLs (old + new style)
- `walk_repo` redacts non-DOI preprint URLs (biorxiv, openreview,
  aclanthology) — table-parametrised
- `walk_repo` redacts bibliography sections after `## References`
- `walk_repo` does NOT redact `## Citation` / `## How to Cite`
  headings (`cite_this_repo` fixture)
- `walk_repo` redacts inline `[1]`/`(Smith 2024)` markers ONLY
  when a bibliography was redacted from the same section
- `assemble_for_model` includes section headers per file
- source_id resolution: `git@github.com:...` URL →
  `github:owner/repo@<sha>`

`test_extract_cli.py` (~8):

- `--dry-run` produces `EXTRACTION.md` + `dry_run.json` but no
  `evident.yaml`
- `--dry-run` EXTRACTION.md contains the "no model call was made"
  string
- model-response processor moves invalid tolerances to rejections
  (mock the Anthropic client at the boundary)
- claim with zero valid tolerances is dropped
- end-to-end mocked: `clean_repo` → 1 claim in the output manifest
- end-to-end mocked: `marketing_repo` (model-emits-0) → 0 claims
- end-to-end mocked: `marketing_repo` (model-emits-marketing,
  validator-rejects) → 0 claims AND EXTRACTION.md records 2
  validator rejections
- end-to-end mocked: `conflict_repo` → 2 claims with distinct
  ids and distinct source_spans; EXTRACTION.md contains a
  "Conflicts detected" section

## Out of scope (PR5b / later)

- pyproject.toml / Cargo.toml / package.json metadata extraction
  (needs a `metadata_compatibility` claim kind that doesn't fit
  PR4's validator)
- `.github/workflows/*.yml` / `Makefile` / `noxfile.py` /
  `Dockerfile` CI workflow extraction
- `tests/` folder usage as evidence for README claims
- `benchmarks/` / `bench/` script extraction
- `source_context: copied_external_text` detection
- Live API record fixture
- Multi-repo batch extraction

## Mocking pattern

Following codex's recommendation: minimal at the SDK boundary,
maximal at the response processor.

- `framing.build_request` tested directly (no mocking) — verifies
  section headers, source_id, tool schema present.
- CLI tests inject a fake `client.messages.create` response shaped
  like the SDK's `Message.content` list, with one
  `tool_use` block carrying a `submit_extracted_claims` payload.
- A small fixture helper `_fake_response(claims, rejections)`
  constructs the SDK-shaped object so tests stay readable.

## File-size and binary policy (v2 explicit)

| Check | Trigger | Action |
|---|---|---|
| Extension allow-list | Not `.md`/`.rst`/`.txt`/`.markdown` | Skip with `not_allowlisted` |
| NUL-byte detection | NUL in first 8 KiB | Skip with `binary` |
| UTF-8 decode | Decode fails on first 8 KiB | Skip with `binary` |
| File size | UTF-8 length > 200 KiB | Truncate; flag `truncated` |
| Symlink resolution | Resolves outside `--repo` root | Skip with `symlink_outside_repo` |

## Estimated size

- Python: ~700–850 LOC + ~500 LOC tests (up from v2's 600–700
  because of expanded redaction regex set, inline-citation
  handling, model-vs-validator rejection distinction, and
  conflict-claim handling)
- Fixtures: 7 small synthetic repos
- No Rust changes
- No new typed-trust schema additions
- No new validator rejection enum values (model-rejections stay
  open-ended; validator rejections stay fixed)

## Open decisions

1. **DOI regex strictness** — the codex-suggested pattern
   `10\.\d{4,9}/[-._;()/:A-Z0-9]+` is conservative. False positives
   on long numeric tokens? Worth a test.
2. **Bibliography heading detection** — case-insensitive substring
   match against `references | bibliography | citations | works
   cited`. Drop the rest of the section (until the next
   `^#` heading or EOF). Documented in EXTRACTION.md as a
   "everything after `## References` was redacted as
   bibliography."
3. **Dry-run + redaction** — should `dry_run.json` show the
   *post-redaction* assembled text the model WOULD have seen, or
   the raw text the curator can read? Recommend: both.
   `dry_run.json.assembled_text_for_model` is post-redaction;
   `dry_run.json.raw_sections[*].text_raw` is pre-redaction.

## Single biggest implementation risk

(Codex's call across v1 + v2.)

v1's biggest risk was the metadata/tolerance semantic mismatch
(killed by v2's deferral to PR5b). v2's biggest risk was citation
redaction false negatives. v3 narrows it further:

**Inline-citation false-negative rate, specifically in sections
where a bibliography was redacted.** The conservative
inline-redaction rule (only fire after bibliography removal)
keeps over-redaction in check, but a paper's `[1, 2]` style
citation in the README body could still leak text the model
treats as load-bearing context. The expanded test suite covers
the listed forms; new forms (`[Smith24]`, `[SMITH-2024]`,
footnote markers `[^1]`) are deliberate followups.

Defense-in-depth via the system prompt is the backstop. The
load-bearing layer is the walker.
