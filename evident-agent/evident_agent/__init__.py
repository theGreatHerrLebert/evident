"""EVIDENT agent — Phase 1 deterministic replay orchestrator.

The agent populates inputs required by typed-trust's ``synthesize()``
by delegating per-claim execution to proteon's existing Docker image
and running proteon's ``claim_scoring.py`` locally to extract observed
values from the produced artifacts. Results are written to a sidecar
``last_verified.json`` (framework convention) which typed-trust then
consumes via ``--last-verified-sidecar``.

The agent is not the judge. It does not invoke a language model.
``synthesize()`` is the judge; the agent only acquires inputs.

This is Phase 1 — deterministic. Phase 2 (LLM-driven ReviewEvent
generation) is a separate effort that layers on top via the same
sidecar pattern.
"""

__version__ = "0.1.0"
