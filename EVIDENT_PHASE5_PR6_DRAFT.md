# PR6 plan — Phase 5-ii: paper extractor (walker + CLI) — v3

> Stacks on PR5 (#23). Last PR of Phase 5. Composes PR4's
> validator/framing/render + PR5's redaction + tool-response
> processor with a new paper-specific walker.
>
> **v2** integrates codex review of v1
> (`EVIDENT_PHASE5_PR6.codex-review.md`). Changes:
>
> - **Paper-mode bibliography detection** (P1): plain-text
>   `References` / `Bibliography` / `Works Cited` lines (no
>   markdown heading) are now redacted. Scientific papers often
>   ship with plain-text bibliography headers; missing them was
>   the largest false-positive contamination path.
> - **PDF is shipped but explicitly experimental** (P2):
>   `EXTRACTION.md` carries a warning banner when source is PDF;
>   `pdftotext` is an optional external dependency with a clean
>   missing-tool note (not a pip dependency).
> - **PDF is split into page-based sections** (P2): the walker
>   splits `pdftotext -layout` output on form-feed (`\f`) into
>   one `SourceSection` per page. This restores the validator's
>   local-binding granularity — the model gets `## page N`
>   section headers and the validator can't accept a comparator
>   that's pages apart from its subject.
> - **Source-id scan extends to 64 KiB** (codex note): the
>   detection runs over the first 64 KiB instead of 4 KiB so
>   embedded arXiv ids in PDF abstracts or title pages aren't
>   missed; still runs BEFORE redaction so the source's own id
>   is preserved.

## Scope

Read a single paper (markdown preferred; PDF via best-effort
text extraction), redact bibliography references to *other*
papers, ask the model to extract structured tolerances, validate
through PR4's validator, write the manifest through PR4's render.

## What ships

### CLI subcommand

```bash
evident-agent extract --paper <path> --output-dir <dir>
                      [--model claude-opus-4-7]
                      [--dry-run]
                      [--project <slug>]
```

`--paper` is a single file (markdown or PDF). The walker reads
it, extracts text if needed, applies the same redaction set PR5
defined.

Mutually exclusive with PR5's `--repo`. The CLI rejects both at
once.

### `evident_agent/extract/paper.py` — the walker

Single function `walk_paper(path, source_id=None) -> WalkedSource`.
Reuses PR5's `WalkedSource` / `SourceSection` / `Redaction` /
`SkippedFile` dataclasses unchanged (one section per paper, kind
`paper`).

Steps:

1. **Detect format.** Extension `.md`, `.rst`, `.txt`,
   `.markdown` → read as text. Extension `.pdf` → extract text
   via `pdftotext` subprocess (best-effort).
2. **PDF text extraction (v3: page-split + no-form-feed contract).**
   Subprocess to `pdftotext -layout` (poppler-utils). Split the
   output on form-feed (`\f`) into per-page strings. Each
   non-empty page becomes one `SourceSection` with `kind:
   "paper-page"` and `path: "page-{n}"`.

   v3 failure-mode contract (codex v2 P2 explicit):

   - `pdftotext` not installed → walker returns `WalkedSource`
     with the file in `skipped` (reason `pdftotext_unavailable`)
     and a `notes` entry with the install hint. CLI exits
     non-zero with a clear message; **diagnostic-only
     `EXTRACTION.md` written** (so the curator sees the install
     hint at the same path as the dry-run audit); no
     `evident.yaml`.
   - `pdftotext` returns non-zero or whitespace-only output →
     same path as not-installed: skipped with reason
     `pdf_extraction_empty`, CLI exits non-zero, diagnostic
     EXTRACTION.md, no manifest.
   - **(v3 P2) `pdftotext` output contains NO form-feeds** —
     codex v2 was explicit that silently treating this as one
     section regresses the local-binding contract. Walker
     skips with reason `pdf_no_page_boundaries`, CLI exits
     non-zero, diagnostic EXTRACTION.md, no manifest. The
     curator's path here is: convert to markdown manually or
     use a PDF extractor that emits page breaks.
   - **(v3) PDF text is non-empty AND form-feed split succeeds
     but is low-yield** (e.g. lots of math notation noise) →
     proceed normally. Model may emit zero claims; that's a
     valid output. `EXTRACTION.md` carries the experimental-PDF
     banner.
3. **Markdown extraction.** Reads as a single `SourceSection`
   with `kind: "paper"` and `path: <basename>`. No page-split
   needed.
4. **Apply redaction.** Reuse `redaction.py::redact()` (moved
   from PR5's `repo.py`). Plus the new paper-mode plain-text
   bibliography detector (v2 P1).
5. **PDF experimental banner (v2 P2).** When the input is a PDF,
   `WalkedSource.notes` gets a banner line like
   `"PDF extraction is experimental; pdftotext-mangled column
   breaks can defeat the validator's local-binding check.
   Inspect the extracted text in dry-run mode before trusting a
   non-dry-run extraction."`. The audit-mode renderer surfaces
   it as a heading-level callout.
6. **Size policy.** Same 200 KiB truncation rule as PR5. Most
   papers fit; a 50-page methods paper might hit it. The
   curator sees the truncation note in `EXTRACTION.md`.

### Source-id resolution (v2: scan extended to 64 KiB)

For a paper, the canonical id is:

1. If the user passes `--source-id` explicitly, use that.
2. Scan the first 64 KiB of the source text (v1 said 4 KiB; v2
   extends to handle embedded arXiv ids in PDF title pages /
   abstracts). For an arXiv id: `arxiv:<id>`. For a DOI:
   `doi:<doi>`.
3. Fall back to `paper:<basename>@<sha256-of-bytes>`.

The detection runs **before** redaction so the source's own id
isn't redacted alongside the bibliography. It also runs **on
the body only** — if a bibliography heading is detected first,
the scan stops there so an arXiv id from a cited paper doesn't
become the source's id.

### Image-table / figure handling

The walker does **NOT** try to detect image-tables itself.
Instead, the system prompt (PR4's framing, augmented here)
explicitly tells the model:

> If a claim's value or comparator appears ONLY inside an image-
> table or figure raster (i.e. you see a reference like "see
> Figure 3" or "Table 4" but no machine-readable values for that
> table in the text), emit no tolerance for that claim and record
> the candidate with `reason: value_only_in_image_table` and the
> page/section locator.

This reuses PR4's existing `value_only_in_image_table` rejection
reason (already in the enum).

### Reusing PR5's components

| PR5 module | PR6 reuse |
|---|---|
| `extract/repo.py::redact()` | Direct reuse |
| `extract/repo.py::_redact_pattern` etc. | Direct reuse |
| `extract/repo.py::WalkedSource` + dataclasses | Direct reuse |
| `extract/cli.py::process_tool_response` | Direct reuse |
| `extract/cli.py::_extract_tool_input` | Direct reuse |
| `extract/audit.py::write_dry_run_outputs` | Direct reuse |
| `extract/render.py` (PR4) | Direct reuse |

PR6 is mostly composition — the new code is the paper walker
(~150 LOC) + CLI wiring (~50 LOC). The fixtures + tests carry
most of the LOC.

### Refactor: shared redaction module

The `redact()` function currently lives in `extract/repo.py`.
PR6 moves it (and the regex constants + helpers) into a new
`extract/redaction.py` module. Both `paper.py` and `repo.py`
import from there.

**(v3 P2 codex)** `repo.py` keeps an explicit re-export so
external callers don't break:

```python
# repo.py
from .redaction import (
    redact, Redaction, REDACTION_DOI, REDACTION_ARXIV,
    REDACTION_PREPRINT, REDACTION_BIBLIOGRAPHY, REDACTION_INLINE,
)
```

Mechanical move (no logic change). Codex v1 confirmed it can
land in PR6 without splitting into a preliminary PR.

### Paper-mode bibliography detector (v2 P1)

`redaction.py` gains a NEW detector for plain-text
bibliography headers — scientific papers (and pdftotext output
from them) often have:

```
References
1. Smith et al., Nature 2024, 12, 345.
2. Jones et al., Science 2023, 89, 12.
```

with no `#` heading. The detector matches a standalone line
whose text (case-insensitive) is one of `References`,
`Bibliography`, `Works Cited` followed by a numbered or
markdown-style reference list within the next ~3 lines. When
matched, everything from that line to EOF is redacted as
`reason: bibliography`.

The numbered-list disambiguation is the safety: a paper's
prose section called "References" (rare but possible) without
references below it does NOT get redacted. The lookahead
checks for `^\s*\d+\.\s+\S` or `^\s*\[\d+\]\s+\S` within the
3 lines after the candidate heading.

The new detector is **paper-mode only** — `walk_repo` doesn't
invoke it because READMEs don't use plain-text bibliography
headings. Tests cover both forms of paper bibliography (markdown
heading from PR5 + new plain-text form).

### Fixtures (six)

Under `evident-agent/tests/fixtures/extract/paper/`:

- `clear_paper.md` — one cleanly-extractable claim
  (`our method achieves median rmsd less than 0.5 across the
  BPTI suite`). Model emits 1 tolerance; validator accepts.
- `hedged_paper.md` — only qualitative language
  ("approximately", "substantially better"). Model emits 0
  claims; manifest is empty.
- `prose_says_better.md` — codex-flagged: prose says
  "outperforms" but the number lives only in a separate
  sentence. The MODEL might emit a tolerance with an invented
  bound; the validator rejects it as
  `comparator_bound_to_wrong_subject`.
- `wrong_subject_binding.md` — codex-flagged: the comparator
  binds to "baseline" not "ours". Same validator-rejects path.
- `mixed_paper.md` — three claim candidates: one clean
  (accepted), one hedged (model rejects), one value-only-in-
  table (model rejects with `value_only_in_image_table`).
  Final manifest: 1 claim. EXTRACTION.md records both
  rejection types.
- `table_only_paper.md` — the only mention of the bound is
  "see Table 3"; the table itself is described as a figure
  ("[Figure 4 shows the table]"). Model rejects with
  `value_only_in_image_table`. Final manifest empty.

### Tests (~18 expected, v2 expanded)

`test_extract_paper.py` (~11):

- `walk_paper` reads a markdown file as a single section
- `walk_paper` detects arxiv id from paper text and uses it as
  `source_id`
- `walk_paper` detects DOI as source_id when present
- `walk_paper` falls back to `paper:<basename>@<sha>` when no
  embedded id
- `walk_paper` applies markdown bibliography redaction
  (`## References` form)
- **(v2 P1)** `walk_paper` applies PLAIN-TEXT bibliography
  redaction (`References` line with no `#`, followed by numbered
  refs) — load-bearing for the codex-flagged contamination path
- **(v2 P1)** `walk_paper` does NOT redact a plain "References"
  line that has no numbered references below it (false-positive
  guard)
- `walk_paper` redacts inline `[1]` citations after a
  bibliography is redacted
- `walk_paper` truncates files > 200 KiB and flags it
- **(v2 P2)** `walk_paper` returns clean skip + install hint
  when `pdftotext` is missing on a PDF input (subprocess shimmed
  in test)
- **(v2 P2)** `walk_paper` splits PDF text on form-feed into
  per-page sections (tested with a synthetic two-page text blob
  shimmed in via subprocess monkeypatch)

`test_extract_paper_cli.py` (~7):

- `--paper clear_paper.md` produces 1 claim end-to-end
- `--paper hedged_paper.md` produces 0 claims, EXTRACTION.md
  notes the model emitted no claims
- `--paper wrong_subject_binding.md` produces 0 claims because
  the validator rejects (codex-flagged failure mode pinned)
- `--paper mixed_paper.md` produces 1 claim + 2 rejections of
  distinct kinds
- `--paper table_only_paper.md` produces 0 claims and a
  `value_only_in_image_table` rejection
- `--dry-run --paper …` writes EXTRACTION.md + dry_run.json
  but no `evident.yaml`
- **(v2 P2)** `--paper <file.pdf>` writes an EXTRACTION.md with
  the experimental-PDF warning banner

## Out of scope (PR6b / later)

- OCR for image-tables (`tesseract` integration)
- pypdf / pdfplumber fallback when `pdftotext` is missing
- arXiv version detection + Supersede-event suggestion when a
  newer version of the paper exists
- Preprint-vs-published drift detection
- Multi-paper batch extraction
- Direct fetch from arXiv id (would compromise the "no network
  beyond Anthropic" property)

## Estimated size

- Python: ~250–350 LOC (new) + ~100 LOC moved into
  `redaction.py` + ~350 LOC tests
- Fixtures: 6 markdown papers + 1 small PDF test fixture
- No Rust changes
- No new typed-trust schema additions

## Open decisions

1. **PDF extractor choice** — `pdftotext` (subprocess, requires
   poppler-utils) is the simplest. Recommend that; document the
   dependency in pyproject `[project.optional-dependencies]
   pdf = ["..."]` for the install hint even though we don't pip-
   install pdftotext.
2. **`--source-id` override** — recommend yes, opt-in. Lets a
   curator pin an explicit source_id when extracting from a
   preprint version where the detected DOI is from the published
   version.
3. **Markdown vs PDF code paths** — both end up producing the
   same `WalkedSource` shape, so downstream code is unified. The
   only difference is the text-extraction step.

## What this commits us to

PR6 completes Phase 5's input-side surface. After it lands, the
extractor accepts a paper or a repo, produces a draft manifest
the framework can consume, and the curator workflow (PR3's
`PromoteFromExtracted` event) is the gate to higher tiers.

## Single biggest implementation risk

(v2 update per codex review.)

v1's biggest risk was PDF extraction quality. Codex correctly
pushed back: PDF mess mostly causes *misses* (validator rejects
a real claim because column-break split it), which is a known
limitation, not a corpus-corrupting failure.

**v2's biggest risk is bibliography contamination from
plain-text paper references.** A paper's `References` section
without a markdown heading would have leaked into the model's
input under v1, producing accepted claims attributed to the
target paper that actually came from cited papers. The new
plain-text bibliography detector (v2 P1) closes this; the
test suite has to cover it directly because the rest of the
system can't detect the leak.

The PDF page-split (v2 P2) restores the validator's local-
binding granularity. Without it, a 50-page paper extracted as
one section could accept a comparator and bound that are
cited in different chapters.
