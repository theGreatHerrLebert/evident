# EVIDENT Workflow Blueprint

This directory sketches how an EVIDENT trust workflow can be shipped and
replayed without turning every case study into one giant container.

The key idea is separation:

- the **manifest** declares claims, evidence, commands, tolerances, and
  reproducibility tier
- the **base runner** checks repo-native evidence structure
- optional **case runners** provide heavy domain environments such as OpenMM,
  CUDA, or large benchmark data

Docker packages the reproducibility boundary. It should not hide the trust
logic. The trust logic belongs in explicit manifests that can be reviewed
without running the container.

## Workflow Shape

```text
evident.yaml              trust claims and replay recipes
workflow/
  README.md               workflow model
  Dockerfile              lightweight base runner
  validate_manifest.py    structural manifest checks
cases/
  proteon.md              interpreted case summary
  proteon/                source repo submodule
  cu-ims-primitives.md    interpreted case summary
  cu-ims-primitives/      source repo submodule
```

## Evidence Tiers

`ci`
: Cheap enough to run frequently. Should not require large external datasets,
  GPUs, or difficult source builds.

`release`
: Heavier checks that should be run before publishing a scientific or numerical
  claim. These may install large oracle tools or use larger datasets.

`research`
: Exploratory or mid-flight checks. Useful evidence, but not yet a stable
  release gate.

## Claim Contract

Each claim should answer:

- What is being claimed?
- Which case or component does it belong to?
- What trust strategy is used: understanding, validation, proof, or a mix?
- What oracle or reference is used?
- What tolerance or decision rule makes the result acceptable?
- Which command reproduces the evidence?
- Which artifact should be produced?
- What assumptions and failure modes remain?

## Container Strategy

The default Docker image is deliberately small. It validates the manifest and
can run documentation-level checks.

Heavy validation should be split into optional images or compose profiles:

- `case-proteon`: Proteon plus release-grade structural-bioinformatics oracles.
- `case-cu-ims-primitives`: CUDA-enabled environment for GPU parity and
  benchmark replay.

This keeps the base workflow useful on any machine while allowing cases to
define their own expensive reproducibility envelopes.

## Base Runner Usage

Build the lightweight validator image:

```bash
docker build -f workflow/Dockerfile -t evident-base .
```

Run it against the current checkout:

```bash
docker run --rm -v "$PWD:/workspace" evident-base
```

This checks manifest structure and local paths. It does not run domain oracles.
Domain validation belongs in case-specific runners that consume the same
manifest contract.
