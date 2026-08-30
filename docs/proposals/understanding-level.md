# Proposal: `understanding` gets a recorded level, earned by prediction

Status: DRAFT proposal. Not yet normative. Targets `workflow/SCHEMA.md`
(manifest surface), `docs/concepts/typed-trust.md` (attestation shape) and
the AutoCoach seam (who produces the attestation).

## The hole

EVIDENT's rule is *the less we understand, the stronger the validation must
be*. `trust_strategy: [understanding]` is a valid manifest value and
`docs/concepts/README.md` defines a six-step ladder (0 surface → 5 proven),
yet nothing in the schema records **where anyone sits on that ladder**, who
that person is, or how they got there. So:

- "understanding" is a bare label with no evidence behind it — the one
  strategy the framework cannot render as `Verified`, `Judged` or `Absent`;
- the rule that ties low understanding to stronger validation cannot fire,
  because the input is missing;
- the AI-era case the rule was written for — an AI-written CUDA kernel the
  signer cannot read — is invisible in the manifest.

## Principle: understanding is a claim about a person, verified by prediction

A person understands a component to the extent they can **predict** its
behaviour under change, not to the extent they can paraphrase it. A model-
written walkthrough produces the feeling of understanding (the illusion of
explanatory depth); a prediction task produces evidence. Prediction is
checkable against the real code — which makes it an oracle for understanding,
the same shape as every other EVIDENT check.

The level owed is set by the **claim being signed**, not by the artifact. To
endorse "CUDA path matches CPU path within 1e-6 on 1k PDBs" a reviewer must
understand the interface, invariants and failure modes that bear on that
claim (accumulation order, cutoff conventions), not warp scheduling. A claim
card's `assumptions` and `failure_modes` are therefore the lesson spec.

## Proposal

`trust_strategy` entries may be a string (today) **or** a block:

```yaml
trust_strategy:
  - validation
  - understanding:
      level: 2                      # 0 surface · 1 functional · 2 algorithmic
                                    # 3 implementation · 4 reconstructive · 5 proven
      by: { name: "D. Teschner", orcid: "…" }       # a person (invariant 9)
      earned_by: prediction         # prediction | reimplementation | proof | reading
      evidence: evident/understanding/cuda-energy-2026-08-30.md
      at: 2026-08-30
```

- `level` follows the ladder in `docs/concepts/README.md`; it is a claim
  about `by`, not about the code.
- `by` must be a `Human` identity. A model may draft the lesson, generate the
  prediction tasks and check the answers, but it cannot be the one who
  understands (typed-trust invariant 9).
- `earned_by: prediction` requires `evidence`: a record of the prediction
  tasks, the predictions made *before* running, and the observed outcomes.
  `reading` is admissible only for `level ≤ 1` — reading does not earn
  algorithmic understanding.
- Projects to typed-trust as `Attested<Understanding>` with a `Judged`
  derivation whose `by` is the person and whose `rationale` is the evidence
  record. Never `Verified`: the predictions were verified, the understanding
  is inferred from them.

## The rule, made executable

| Recorded understanding on the claim | Minimum validation the validator requires |
|---|---|
| none, or level ≤ 1 | full structured validation at the claim's tier — every tolerance `metric/op/value`, an independent oracle, no `research` escape hatch above research tier |
| level 2–3 | as today |
| level ≥ 4 with `earned_by: reimplementation` or `proof` | may cite the reimplementation / proof as an oracle at `ci` tier |

`trust_strategy: [understanding]` **alone** — no validation, no proof — is
rejected above `research` tier unless `level ≥ 4`. Understanding without
evidence of understanding is prose.

## Validator rules

- A block-form entry must carry `level`, `by`, `earned_by`; `evidence` is
  required when `earned_by ∈ {prediction, reimplementation, proof}`.
- `by` with `kind: model` or `automated` is an error.
- `level ≥ 2` with `earned_by: reading` is an error.
- Staleness: an understanding attestation names the `pinned_versions` it was
  earned against; if the claim's source pin moves past it, the renderer shows
  the level as *stale* (the person understood a previous version).

## Producer: the AutoCoach seam

AutoCoach's job is to raise a person's level on exactly the components a
claim needs them to understand. The contract between the two:

1. **Input**: a claim card (statement, `assumptions`, `failure_modes`,
   `subsystem`, `source`). This is the lesson spec — no separate curriculum
   document is needed.
2. **Lesson**: model-led, grounded in the real source, ending in **prediction
   tasks** ("if the tile size halves, does the electrostatic term change?
   by how much? why?") whose answers are checked by running the code.
3. **Output**: the `understanding` block above plus the evidence record. The
   person signs it; the model never does.

See `EVIDENT-SEAM.md` in the AutoCoach repository for the producer side.

## External framing

Andrew Ng's 2026 letters describe the same shift from outside: developers
stop reading generated code and instead "steer coding agents using the
precise language of software engineering" and "help the agent autonomously
close loops by providing verifiers"; those who "deeply understand how
software works vastly outperform those who vibe code without understanding";
and "don't blindly trust AI's confidently stated conclusions." EVIDENT is what
*providing verifiers* means when the ground truth is a reference
implementation rather than a unit test; this proposal is what *understanding*
means when it has to be recorded rather than assumed.

## Out of scope

- Grading *how well* a prediction record demonstrates a level. That is a
  Judged act by a second person; the validator checks shape only.
- A universal ladder across domains. The six levels are deliberately coarse.
