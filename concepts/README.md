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
