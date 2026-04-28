# Concepts

Concepts define the shared language of this repository.

They are intentionally short, precise, and debatable.

---

## Understanding

Understanding refers to the ability to explain, reason about, and (at higher levels) reproduce the behavior of a computational component.

### Levels of Understanding

0. Surface — can run it  
1. Functional — knows what it does  
2. Algorithmic — knows why it works  
3. Implementation — knows how it works  
4. Reconstructive — can reimplement it  
5. Proven — behavior formally guaranteed  

Understanding is not binary. It determines how much validation is required.

---

## Validation

Validation establishes confidence through empirical evidence.

Examples:
- comparison to reference tools
- benchmark datasets
- simulation with ground truth
- cross-implementation agreement

### Levels of Validation

0. Smoke — it runs
1. Example — passes hand-picked inputs
2. Property — invariants hold over generated inputs
3. Oracle — agrees with an independent reference within tolerance
4. Multi-oracle — agrees with references that disagree with each other
5. Reference — adopted as the reference by external users

The Understanding and Validation ladders trade off: lower understanding requires higher validation.

Validation depends on assumptions and can fail systematically.

---

## Proof

Proof establishes guarantees independent of empirical testing.

Examples:
- correctness proofs
- numerical bounds
- invariants

Proof is rare in complex systems but valuable where feasible.

---

## Trust

Trust is not a property of code, but of justification.

It arises from a combination of:
- understanding
- validation
- proof

### Trust Envelope

Trust is relational. A claim states:

- purpose — what the component is used for
- tolerance — how much error is acceptable for that purpose
- environment — hardware, dataset, dependency versions
- expiry — what would invalidate the claim

A claim outside its envelope is not the same claim.

---

## Failure Modes

Systems often fail in ways that produce plausible results.

Understanding failure modes is as important as demonstrating correctness.

---

## Oracles

An oracle is an external reference used to validate behavior.

Examples:
- established tools
- analytical solutions
- simulated ground truth

---

## Provenance

The chain of custody of evidence. Separable from Understanding (mental model) and Validation (results).

Includes:
- source — port, transcription, generation, paper-inspired, original
- attribution — authorship and license boundaries
- audit trail — oracle version, fixture commit, environment, run

A result without provenance is not reproducible evidence.
