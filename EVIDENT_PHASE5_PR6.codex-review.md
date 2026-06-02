Reading additional input from stdin...
OpenAI Codex v0.136.0
--------
workdir: /scratch/TMAlign/evident
model: gpt-5.5
provider: openai
approval: never
sandbox: workspace-write [workdir, /tmp, $TMPDIR]
reasoning effort: medium
reasoning summaries: none
session id: 019e8a0e-363b-7622-8d44-1481134cfaa2
--------
user
Review this implementation plan for Phase 5 PR6 — the paper extractor walker + CLI for EVIDENT's evident-agent. Background: PR1-PR5 of Phase 5 have shipped (5 PRs, 320 Python + 170 Rust tests, all green). PR6 is the last Phase 5 PR — it composes PR4's validator/framing/render with PR5's redaction to handle a NEW input type (paper instead of repo).

You reviewed each prior PR plan + code and made findings that were addressed.

Focus on:

1) SCOPE — is this the right slice for PR6? Alternative slicings: (a) ship markdown only, defer PDF to PR6b (the parent v3 plan called native-PDF 'best-effort'); (b) ship PDF only via pdftotext (markdown is so simple it's basically free); (c) skip PDF entirely until OCR is ready in PR6b. Which slicing actually closes Phase 5?

2) PDF EXTRACTION QUALITY — pdftotext on real papers produces messy output (column breaks split sentences, math notation garbles, tables reflow into prose). The plan documents this as the biggest risk. But: should PR6 require pdftotext, document it as required, or treat PDF as 'experimental' with a warning banner in EXTRACTION.md? If PR6 ships PDF and the validator fails because pdftotext destroyed the local-binding context, is that a PR6 bug or a known limitation?

3) BIBLIOGRAPHY REDACTION — papers cite OTHER papers in their references. PR5's bibliography heading regex (## References / ## Bibliography / ## Works Cited with optional trailing punct + Setext) is paper-friendly. But: scientific papers often use numbered references (1. Smith et al.) without a markdown heading at all — the references section starts at 'References' as plain text, no heading. Does PR5's regex catch that? If not, what's the fix in PR6?

4) SOURCE-ID DETECTION — extracts arXiv id or DOI from the first 4 KiB. What if the paper has an embedded title page (PDF) where the arXiv id is at the bottom of the abstract, not the first 4 KiB? Or in a footer (text extraction order is unreliable)? Worth scanning more, or scanning specific patterns regardless of position?

5) IMAGE-TABLE REJECTION — left to the MODEL with a prompt hint. Is that enough? Concrete failure mode: the paper's prose says 'our method achieves 95.2% accuracy (see Table 3)', and Table 3 has the breakdown. The model might extract 'accuracy > 95%' from the prose. The validator accepts (the bound is stated). Should the prompt explicitly cover this case, or is it correctly extractable since the prose itself states the value?

6) WRONG_SUBJECT_BINDING FIXTURE — the test asserts the validator rejects. But codex (you) flagged this exact case in PR4's review and the validator catches it. Is this fixture necessary in PR6, or is it redundant with PR4's coverage?

7) MIXED_PAPER FIXTURE — three claim types in one paper. Concrete shape: how does the model know to extract one and reject two? Is the framing strong enough to handle the mix, or does the model treat the paper as a single source and either over- or under-extract?

8) REFACTORING redact() — the plan says move to a shared module. Is the LOC estimate realistic? Code-shaped: extract/redaction.py exports redact(), Redaction, _redact_pattern, _trim_trailing_url_noise, the regex constants. Should the move be split into its own preliminary PR to avoid mixing refactor + new feature?

9) MARKDOWN VS PDF UNIFIED OUTPUT — both produce the same WalkedSource. But the PDF extractor will produce ONE big section while markdown might naturally have headings. Does the local-binding rule care? Specifically: a paper PDF section that's 50 KB of prose with one comparator-bound co-occurrence still passes the validator. Is that right, or does the prompt need to be stricter on 'find the specific sentence' for papers?

10) ANY OTHER ISSUES — load-bearing failure modes the plan doesn't anticipate, places where it composes badly with PR4/PR5, scope creep.

If the plan is ready to implement, say so and identify the single biggest implementation risk. If not, give at most 3 P1-or-P2 findings that MUST be addressed before code. Concrete and specific only. Cap ~1000 words.

<stdin>
# PR6 plan — Phase 5-ii: paper extractor (walker + CLI)

> Stacks on PR5 (#23). Last PR of Phase 5. Composes PR4's
> validator/framing/render + PR5's redaction + tool-response
> processor with a new paper-specific walker.

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
2. **PDF text extraction.** Subprocess to `pdftotext -layout`
   (poppler-utils). Failure modes:
   - `pdftotext` not installed → walker returns empty
     `WalkedSource` with `notes` entry pointing the curator at the
     install command and `skipped` entry for the file with reason
     `pdftotext_unavailable`. CLI surfaces this clearly; doesn't
     fall over.
   - `pdftotext` returns non-zero / empty output → same.
   - PDF is image-only (scanned) → text extraction produces no
     readable claims; the model's response will be near-empty
     and `EXTRACTION.md` flags the source as low-yield.
3. **Apply redaction.** Reuse PR5's `redact()` function unchanged.
   The bibliography section, DOI/arXiv/preprint URLs, and inline
   citations get redacted from the assembled text. The model
   never sees citations to other papers.
4. **Size policy.** Same 200 KiB truncation rule as PR5. Most
   papers fit; a 50-page methods paper might hit it. The
   curator sees the truncation note in `EXTRACTION.md`.

### Source-id resolution

For a paper, the canonical id is:

1. If the user passes `--source-id` explicitly, use that.
2. If the paper's text contains an `arXiv:` id near the start
   (first 4 KiB), use `arxiv:<id>` with the id from the paper.
3. If the paper's text contains a DOI near the start, use
   `doi:<doi>`.
4. Fall back to `paper:<basename>@<sha256-of-bytes>`.

The detection runs **before** redaction so the source's own id
isn't redacted alongside the bibliography. The walker remembers
the detected id but strips it from the assembled-for-model text
so the model can't accidentally extract from the citation itself.

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
PR6 needs to call it from `extract/paper.py`. Two options:

- **A**: import from `repo.py` directly — but `paper.py`
  importing `repo.py` is a confusing dependency arrow.
- **B**: move shared redaction code into a new
  `extract/redaction.py` module, have both `repo.py` and
  `paper.py` import from there.

Recommend B. The move is mechanical (~250 LOC moved, 0 logic
changed). Small follow-up PR could also extract `_read_text_safely`
into a shared file-policy module, but PR6 keeps it in `repo.py`
since `paper.py` doesn't need it.

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

### Tests (~14 expected)

`test_extract_paper.py` (~8):

- `walk_paper` reads a markdown file as a single section
- `walk_paper` detects arxiv id from paper text and uses it as
  `source_id`
- `walk_paper` falls back to `paper:<basename>@<sha>` when no
  embedded id
- `walk_paper` applies the same bibliography redaction as repos
  (`## References` section dropped)
- `walk_paper` redacts inline `[1]` citations after a
  bibliography is redacted
- `walk_paper` truncates files > 200 KiB and flags it
- `walk_paper` handles PDF with `pdftotext` available (skip if
  binary not present, test marker)
- `walk_paper` returns clean skip + note when `pdftotext` is
  missing on a PDF input

`test_extract_paper_cli.py` (~6):

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

**PDF extraction quality.** `pdftotext` output on a real paper
is messy: column breaks merge, math notation becomes garbled,
table cells get reflowed into prose. The model is reasonably
robust to mess, but the *validator* is regex-strict. A column
break that splits "our method achieves rmsd" from "< 0.5 across
the suite" would defeat the local-binding check.

Mitigations:
- The test suite ships with one PDF fixture (a clean,
  single-column paper) to lock in basic behaviour.
- Real-world PDF mess is a known limitation, documented in
  `EXTRACTION.md`'s per-paper output.
- A curator running `--dry-run` against a noisy PDF sees the
  extracted text and can decide to convert to markdown
  manually instead.
</stdin>
codex
Not ready as written. The slice is broadly right, but I’d require these fixes before code.

**P1: Bibliography redaction is insufficient for papers.**  
PR5’s markdown-heading regex will not reliably catch plain-text paper sections like:

```text
References
1. Smith et al. ...
2. Jones et al. ...
```

That is a real contamination path: cited-paper claims can enter the model context and look like extractable claims from the target paper. PR6 should add paper-mode bibliography detection that recognizes standalone `References`, `Bibliography`, `Works Cited`, etc. lines without markdown syntax, then redacts from that line to EOF or to a clearly detected appendix boundary if you support one. It should be tested with numbered references and DOI/arXiv entries under a plain-text `References` heading.

**P2: PDF support needs a clearer contract.**  
To close Phase 5, I’d ship markdown plus best-effort PDF via `pdftotext`; skipping PDF entirely leaves the “paper” input surface incomplete, while PDF-only makes no sense because markdown is cheap and higher quality.

But PDF should be documented as **experimental / best-effort**, not as a hard-quality guarantee. `pdftotext` should be an optional external dependency with a clear missing-tool note, not a mandatory package requirement for normal installs. If `pdftotext` mangles context and the validator rejects a valid claim, that is a known limitation. If mangled PDF text causes an accepted false claim, that is still a PR6 bug unless the output clearly shows the exact accepted evidence sentence and the curator gate is expected to catch it.

Add an `EXTRACTION.md` warning banner for PDF inputs, especially when extraction is truncated, low-yield, or unsectioned.

**P2: One huge PDF section weakens the validator/framing composition.**  
A single 50 KiB section makes “local binding” too permissive unless downstream validation is strictly sentence/window based. PR6 should either split PDFs into page-based sections using `pdftotext` page breaks/form-feed, or make the prompt/tool schema require the model to return the specific evidence sentence/span used for each tolerance. The prompt should be explicit: values mentioned in prose are extractable only when the subject, comparator, and bound are all stated in nearby machine-readable prose; “see Table 3” alone is not enough.

Other notes:

- Source-id detection should scan more than the first 4 KiB. Prefer explicit `--source-id`, then scan pre-bibliography text for arXiv/DOI patterns, with a larger cap like 64 KiB or first few pages. Avoid scanning references.
- The image-table case is correctly extractable if the prose itself says “our method achieves 95.2% accuracy.” It is not extractable if the prose only says “see Table 3” and the value lives only in the table/raster.
- `wrong_subject_binding.md` is redundant with PR4 unit coverage, but useful as PR6 integration coverage proving paper framing plus validator still compose correctly. Keep it if test runtime is cheap.
- `mixed_paper.md` is reasonable only if the model contract is candidate-level: extract accepted claims and separately report rejected candidates. Otherwise it will be flaky.
- Moving `redact()` into `extract/redaction.py` can stay in PR6 if it is a pure move with no behavior change. I would not split a separate PR unless the diff becomes noisy.

Single biggest implementation risk: false positives from bibliography/reference contamination, not `pdftotext` quality. PDF mess mostly causes misses; unredacted references can create plausible but wrong accepted claims.
tokens used
6,163
Not ready as written. The slice is broadly right, but I’d require these fixes before code.

**P1: Bibliography redaction is insufficient for papers.**  
PR5’s markdown-heading regex will not reliably catch plain-text paper sections like:

```text
References
1. Smith et al. ...
2. Jones et al. ...
```

That is a real contamination path: cited-paper claims can enter the model context and look like extractable claims from the target paper. PR6 should add paper-mode bibliography detection that recognizes standalone `References`, `Bibliography`, `Works Cited`, etc. lines without markdown syntax, then redacts from that line to EOF or to a clearly detected appendix boundary if you support one. It should be tested with numbered references and DOI/arXiv entries under a plain-text `References` heading.

**P2: PDF support needs a clearer contract.**  
To close Phase 5, I’d ship markdown plus best-effort PDF via `pdftotext`; skipping PDF entirely leaves the “paper” input surface incomplete, while PDF-only makes no sense because markdown is cheap and higher quality.

But PDF should be documented as **experimental / best-effort**, not as a hard-quality guarantee. `pdftotext` should be an optional external dependency with a clear missing-tool note, not a mandatory package requirement for normal installs. If `pdftotext` mangles context and the validator rejects a valid claim, that is a known limitation. If mangled PDF text causes an accepted false claim, that is still a PR6 bug unless the output clearly shows the exact accepted evidence sentence and the curator gate is expected to catch it.

Add an `EXTRACTION.md` warning banner for PDF inputs, especially when extraction is truncated, low-yield, or unsectioned.

**P2: One huge PDF section weakens the validator/framing composition.**  
A single 50 KiB section makes “local binding” too permissive unless downstream validation is strictly sentence/window based. PR6 should either split PDFs into page-based sections using `pdftotext` page breaks/form-feed, or make the prompt/tool schema require the model to return the specific evidence sentence/span used for each tolerance. The prompt should be explicit: values mentioned in prose are extractable only when the subject, comparator, and bound are all stated in nearby machine-readable prose; “see Table 3” alone is not enough.

Other notes:

- Source-id detection should scan more than the first 4 KiB. Prefer explicit `--source-id`, then scan pre-bibliography text for arXiv/DOI patterns, with a larger cap like 64 KiB or first few pages. Avoid scanning references.
- The image-table case is correctly extractable if the prose itself says “our method achieves 95.2% accuracy.” It is not extractable if the prose only says “see Table 3” and the value lives only in the table/raster.
- `wrong_subject_binding.md` is redundant with PR4 unit coverage, but useful as PR6 integration coverage proving paper framing plus validator still compose correctly. Keep it if test runtime is cheap.
- `mixed_paper.md` is reasonable only if the model contract is candidate-level: extract accepted claims and separately report rejected candidates. Otherwise it will be flaky.
- Moving `redact()` into `extract/redaction.py` can stay in PR6 if it is a pure move with no behavior change. I would not split a separate PR unless the diff becomes noisy.

Single biggest implementation risk: false positives from bibliography/reference contamination, not `pdftotext` quality. PDF mess mostly causes misses; unredacted references can create plausible but wrong accepted claims.
