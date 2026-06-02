"""Phase 5: claim extraction from papers and repos.

This package is the input-side counterpart to evident-agent's existing
``replay`` / ``review`` subcommands. It produces a *draft* claim
manifest in the schema typed-trust already understands, suitable for
human curator review before promotion to ``ci`` / ``release`` tiers.

See ``EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md`` for the full design.

Module layout:

- ``validator`` — load-bearing source-span validator with local-binding
  rule. Kills silent threshold invention.
- ``framing`` — Anthropic tool schema + default-deny system prompt the
  extractor uses to talk to the model.
- ``render`` — output writer for the ``extracted/<artifact-id>/``
  directory shape (evident.yaml + cited.md + EXTRACTION.md).

Walkers (``paper.py``, ``repo.py``) live in follow-up PRs.
"""

from __future__ import annotations
