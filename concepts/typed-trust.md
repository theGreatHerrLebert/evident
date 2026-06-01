# Typed Trust

The verification core for EVIDENT, expressed as a small typed model.
Complements `EVIDENT_DESIGN.md` (the framing essay) and the shipping
manifest schema (`workflow/SCHEMA.md`, the project-facing surface).

**The contract in one sentence.** Every assertion EVIDENT carries records
*how it was established* as part of its type: `Verified` (reproducible
procedure), `Judged` (interpretation), or `Absent` (sought and not found).
A model judgment and a human judgment are the same kind of derivation;
the framework restricts what a *stage* may produce, not what an *author*
may be.

This document specifies the types, the invariants they enforce, and the
small set of validator rules that hold the system together. It is
deliberately compact: ~10 first-class types, ~9 supporting enums.
Complexity that does not earn its surface area lives in validator rules,
project conventions, and the shipping manifest's vocabularies instead.

---

## 0. Scope

Typed Trust types **propositional empirical claims** — assertions about
the world that can in principle be Verified, Judged, or recorded as
Absent. This is broader than "measurement" (Causal, Existence, and
Provenance claims are propositional too) but narrower than the full
shipping-manifest surface.

Out of scope, deliberately:
- **Policy claims** — prescriptions, not propositions. Stay in the manifest.
- **Reference claims** — pointers, not assertions. Stay in the manifest.

Cross-layer queries ("which measurements satisfy this policy?") are
**tooling concerns**. The shipping manifest already links claims to
cases, patterns, and subsystems via path references; tools that want to
answer cross-layer questions read both layers. The type system does not
materialize the link.

---

## 1. Identity

One identity type. The distinction between "performer" and "judge" is
enforced by where the field appears in `Derivation` and by a validator
rule, not by the type hierarchy.

```rust
struct Identity {
    kind: IdentityKind,
    name: String,                       // human name / model id / CI service / "anonymous"
    details: Vec<IdentityDetail>,       // structured key/value: orcid, affiliation,
                                        // ci_run, version, anonymity_reason, ...
}

struct IdentityDetail {
    key: String,
    value: String,
}

enum IdentityKind {
    Human,
    Model,
    Automated,
    Organization,
    Anonymous,
}
```

Calibration questions ("how does this judge's output compare to ground
truth?") live outside the type system — a tool correlates
`(Identity, protocol)` pairs to outcomes.

**Validator rules:**
- `Identity { kind: Automated, .. }` cannot appear as the `by` of a
  `Judged` derivation. Automation does not author non-reproducible
  interpretations.
- Renderers and validators key off well-known `IdentityDetail.key`
  values (`orcid`, `affiliation`, `ci_run`, `version`,
  `anonymity_reason`) when present. Unknown keys pass through.

---

## 2. Derivation

```rust
enum Derivation {
    Verified {
        method: ToolInvocation,
        ran_by: Identity,
        reruns: Vec<Rerun>,             // chronological; latest is most recent
    },
    Judged {
        by: Identity,
        protocol: Option<String>,       // rubric / prompt / decoding spec
        rationale: String,              // required; non-empty
        confidence: Confidence,
    },
    Absent {
        sought: String,
        searched: Vec<Locator>,
        searched_by: Identity,
    },
}

struct Rerun {
    at: Timestamp,
    by: Identity,
    observed: Vec<MetricObservation>,   // each binds to a Criterion (§7)
    corpus_sha: Option<Hash>,
    outcome: ReproductionOutcome,
}

enum ReproductionOutcome {
    Matched,
    Diverged { detail: String },
}

enum Confidence { Low, Moderate, High }

struct Attested<T> {
    value: T,
    derivation: Derivation,
    at: Timestamp,
}
```

Reruns are embedded inside `Verified`, not promoted to a graph object.
In practice nobody files a Challenge against "the third rerun
specifically" — they challenge the benchmark. The rare case ("this
rerun is fraudulent") is expressible as a Claim whose source is the
rerun.

`protocol` is a free string, not a typed `Protocol` struct. Calibration
discipline is project-driven, not type-enforced.

---

## 3. Invariants

1. **Nothing contestable masquerades as a fact.** Verified/Judged is a
   reproducibility distinction, not an authorship distinction.
2. **Synthesis introduces no new judgment, by anyone.** The forbidden
   act is non-reproducible interpretation in the deterministic stage —
   model or human alike.
3. **No bare aggregate confidence.** No model-emitted "overall confidence."
   Aggregates, if present, are named pure functions of the criteria.
4. **Absence is first-class.** Information sought and not found is
   `Absent`, never silently defaulted to `false`, `None`, or empty.
5. **Every `Judged` derivation carries a non-empty `rationale` and an
   `Identity`.**
6. **Challenges are claims when they assert substantive content.** A
   substantive `Challenge` carries `backed_by: Some(ClaimId)`; the
   backing Claim's TrustReport governs whether the challenge moves
   render-time status. Closed *procedural* `ChallengeCategory` variants
   (§6) may move status without backing.
7. **Reviewability is author-symmetric.** No framework rule mentions a
   specific author kind.
8. **Endorsements have no semantic weight by default.** They strengthen
   the displayed rationale; they do not affect criterion results or
   aggregate.
9. **`Automated` cannot judge.** Validator rule.
10. **Release-tier review events require a protocol.** A `ReviewEvent`
    with `kind ∈ {Endorse, Dissent, Challenge}` targeting a release-tier
    claim must carry a non-empty `protocol`. Otherwise "peer-reviewed"
    degrades to a badge — the report has no way to tell what was
    reviewed (code, command, artifact, tolerance, prose). Validator rule
    driven by the shipping manifest's tier system, not by a typed
    `Tier` concept inside this layer.

---

## 4. Reproducibility boundary

| Stage | Class | Tools? | Output |
|---|---|---|---|
| Claim extraction | Judged | no | `Vec<Attested<Claim>>` |
| Evidence discovery | Mixed | yes | `Vec<Evidence>` |
| Adversarial review | Judged | no | `Vec<Attested<Claim>>` + `Vec<ReviewEvent>` |
| Provenance analysis | Verified (preferred) | yes | `ProvenanceRecord` |
| Synthesis | Deterministic | no | `TrustReport` |

A stage marked Judged produces non-reproducible interpretations
regardless of who staffs it.

---

## 5. Claim

```rust
struct Claim {
    id: ClaimId,
    text: String,
    kind: ClaimKind,
    source: SourceSpan,
    explicit: bool,                     // stated verbatim vs. inferred
    decomposes_into: Vec<ClaimId>,
    requires_assumptions: Vec<Attested<Assumption>>,
}

enum ClaimKind {
    Performance,
    Comparison,
    Causal,
    Existence,
    Reproducibility,
    Provenance,
    Other(String),
}

struct Assumption {
    text: String,
    load_bearing: bool,
}
```

`ClaimKind` is first-order only. Review actions (endorse, dissent,
challenge, supersede) live in `ReviewEvent`, not as Claim variants.

---

## 6. ReviewEvent

```rust
struct ReviewEvent {
    id: EventId,
    target: Target,
    by: Identity,
    protocol: Option<String>,           // required at release tier (invariant 10)
    rationale: String,
    at: Timestamp,
    kind: ReviewKind,
}

enum ReviewKind {
    Endorse,
    Dissent,
    Supersede { successor: AttestedId },
    Challenge {
        category: ChallengeCategory,
        backed_by: Option<ClaimId>,
    },
}

enum Target {
    Claim(ClaimId),
    ClaimAttestation(AttestedId),
    Evidence(EvidenceId),
    SupportRelation(EvidenceId),
    Provenance(ProvenanceId),
    TrustReport(ReportId),
    Criterion { report: ReportId, criterion: CriterionId },
    ReviewEvent(EventId),
}

enum ChallengeCategory {
    // Substantive — REQUIRE backing Claim to move status:
    MissingControl,
    WeakStatistics,
    Confound,
    UnverifiableAssumption,
    MissingBenchmark,
    ReproducibilityRisk,

    // Procedural — closed list, MAY move status without backing:
    ArtifactUnavailable,
    HashMismatch,
    CommandFailure,
    ConflictOfInterest,
    PeerReviewUnverifiable,

    // Open project vocab — ALWAYS requires backing:
    Other(String),
}
```

The typed `Target` enum is load-bearing: different objections target
different objects (claim text vs. support relation vs. provenance vs. a
specific criterion), and the type catches that ambiguity at construction
time.

Multi-judge sign-off is N Endorse events targeting the same id. Tools
that need formal panel semantics compute them over the graph; there is
no `PanelJudgment` type.

---

## 7. TrustReport, Criterion, Tolerance, MetricObservation

```rust
struct TrustReport {
    claim: ClaimId,
    criteria: Vec<Criterion>,
    challenges: Vec<EventId>,           // rendered references to ReviewEvent
                                        // graph; report is the user-facing
                                        // object and should not require
                                        // graph traversal
    gaps: Vec<Gap>,
    aggregate: Option<Attested<Aggregate>>,   // default None
    status: RenderStatus,               // synthesized; see §8
}

enum RenderStatus {
    Current,
    Superseded,
    Contested,
}

struct Criterion {
    id: CriterionId,                    // stable; observations bind here
    name: String,
    tolerance: Option<Tolerance>,
    result: Attested<CriterionResult>,
}

struct Tolerance {
    metric: String,                     // free string + project vocab
    op: ComparisonOp,
    value: f64,
    output: Option<String>,
    prose: String,                      // required human gloss
}

enum ComparisonOp { Lt, LtEq, GtEq, Gt }
// Eq deliberately omitted. Float equality is AbsoluteError + LtEq with
// an explicit epsilon.

struct MetricObservation {
    criterion: CriterionId,             // binds observation to criterion
    value: f64,
    unit: Option<String>,
}

enum CriterionResult {
    Pass,
    Fail,
    Partial { detail: String },
    NotApplicable,
    NotAssessed { reason: String },     // distinct from Fail
}

struct Gap {
    description: String,
    would_satisfy: Vec<String>,
    author_actionable: bool,
}
```

`Tolerance.metric` is a free string + project vocab, matching the
shipping manifest's pattern. One source of truth — the manifest's
`tolerance_metric` vocabulary — instead of duplicating it as a typed
enum.

`MetricObservation.criterion: CriterionId` is the structural bind. Name
matching between observation and tolerance is not enough — silent drift
(`relative_error` vs `RelativeError` vs `rel_error`) defeats the
falsifiability anchor.

`RenderStatus` is a closed enum on the report, not a free string.
Downstream CI gates and doc generators consume a typed value; render
status is computed once at synthesis and carried in the report.

---

## 8. Render status — computed, not propagated

Per-attestation contestation is computed at synthesis and surfaces as
`TrustReport.status` (and per-criterion equivalents). It is not a typed
field on `Attested<T>`, because in steady state every attestation is
`Current` — propagating a `Currency` enum through every value to support
the dispute case would tax the common case.

Rule:

> A `TrustReport` (or a Criterion in one) is rendered as:
>
> - **Superseded** if a `ReviewEvent { kind: Supersede { successor },
>   target: A }` exists targeting the underlying attestation;
> - **Contested** if a `ReviewEvent { kind: Challenge, target: A }`
>   exists AND either (a) `category` is in the closed procedural list,
>   or (b) `backed_by: Some(C)` and `C`'s TrustReport synthesizes to a
>   passing-criteria result;
> - **Contested** if the graph reachable from A contains a cycle in
>   challenge edges;
> - **Current** otherwise.

Endorsements and Dissents do not appear in the calculation. They
strengthen displayed rationale only (invariant 8).

---

## 9. The shape that didn't survive

Documenting what got cut and why, so future contributors don't
re-discover the same temptations:

| Cut | Replaced by | Why |
|---|---|---|
| `Principal` taxonomy (5 variants, 5 sub-structs, 2 wrappers) | `Identity { kind, name, details }` | Type-level discrimination across identity classes never paid for itself; one validator rule (`Automated can't judge`) captured the only load-bearing case |
| `PerformedBy` / `JudgedBy` wrapper structs | Field position in `Derivation` | The field name (`ran_by`, `by`, `searched_by`) carries the role; structural wrappers added a layer without earning it |
| `Protocol { id, name, version }` typed struct | `protocol: Option<String>` plus invariant 10 | Calibration discipline is project-driven; release-tier rule plus closed `PeerReviewUnverifiable` challenge category catches the abuse case |
| `PanelJudgment`, `Agreement` enums | N Endorse events with shared target | Joint-act semantics distinct from sequential confirmation rarely shows up in practice; tools can compute panel semantics over the graph |
| `Currency` enum on `Attested<T>` | `RenderStatus` on `TrustReport` | Contestation matters at report time, not value time; promoting it everywhere taxed the steady-state common case |
| `ValidationProfile` | Manifest tier system handles externally | The typed layer should not invent a parallel admissibility vocabulary |
| `ManifestBinding`, `BindingRelation` | Tooling reads both layers | Typed cross-layer queries are a parallel type system; the shipping manifest already has paths, ids, cases, patterns, subsystems |
| `ChallengeEffect`, `ProceduralHold` | Closed procedural categories baked into `ChallengeCategory` | Effect-vs-category orthogonality is mostly empty in practice |
| `MetricName` typed enum | `Tolerance.metric: String` + project vocab | Duplicating the shipping manifest's `tolerance_metric` vocabulary as a typed enum forced two places to stay in sync |
| `VerificationRun` as graph object | `Vec<Rerun>` embedded in `Verified` | Independent challengeability of individual reruns is rarely exercised |

The systemic move is to push complexity out of the type system into:
- **Validator rules** (small ruleset enforced at construction).
- **Project conventions** (each project's tooling).
- **Shipping manifest vocab** (one place to declare metric names,
  challenge categories, identity detail keys).

---

## 10. Type and concept count

| Category | Count |
|---|---|
| Core types (carry framework semantics) | 10 — `Attested`, `Derivation`, `Identity`, `Claim`, `ReviewEvent`, `Evidence`, `ProvenanceRecord`, `TrustReport`, `Criterion`, `Tolerance` |
| Supporting structs | 7 — `Rerun`, `MetricObservation`, `IdentityDetail`, `Assumption`, `Gap`, `SourceSpan`, `ToolInvocation` |
| Tag/payload enums | 10 — `IdentityKind`, `ClaimKind`, `ReviewKind`, `Target`, `ChallengeCategory`, `ReproductionOutcome`, `Confidence`, `ComparisonOp`, `CriterionResult`, `RenderStatus` |

A user writing a manifest learns one identity type, one tolerance type,
one review event type. There is no panel concept, no protocol struct,
no currency to propagate, no profile to declare.

---

## 11. Relationship to the shipping manifest

Typed Trust is the verification engine for propositional empirical
claims; the manifest (`workflow/SCHEMA.md`) is the project-facing
surface for all claim kinds. They coexist:

- Every `kind: measurement` manifest claim projects into Typed Trust
  types. The seam is documented in this section's mapping table (TBD,
  follow-up: produce a normative binding spec rather than a prose table).
- `kind: policy` and `kind: reference` claims stay in the manifest and
  are not represented as `Claim` here.
- Vocabularies (`tolerance_metric`, `oracle`, `subsystem`,
  `capability`) live in the manifest and are referenced by Typed Trust
  values as free strings.
- Tier-driven admissibility rules (e.g., invariant 10) are enforced by
  the validator against the manifest, not by the type system itself.

The next milestone is a precise translator spec — a deterministic
mapping from the manifest's measurement-class fields to Typed Trust
constructors, so a tool can round-trip without manual judgment.

---

## 12. Iteration trail

This design is the result of three review rounds against the v0.2
framing essay in `EVIDENT_DESIGN.md`, plus one worked fit-test against
the proteon SASA / Biopython parity claim shape. Working artifacts
preserved alongside this document:

- `EVIDENT_DESIGN.md` — the v0.2 framing essay (origin, motivation, prose).
- `EVIDENT_DESIGN_v0.3_DRAFT.md` and forward — the typed iteration.
- `EVIDENT_DESIGN_v0.3.codex-review.md`, `v0.5`, `v0.6` — independent
  reviews by OpenAI Codex that drove the structural moves (the
  Claim/ReviewEvent split, the `Target` enum, the `Tolerance` type, the
  compactness pass).
- `EVIDENT_DESIGN_v0.4_FIT_TEST.md` — worked example that surfaced the
  three gaps closed in v0.5 (`Tolerance`, reproduction history, scope).

These are not normative. The contract is what's in this document.

---

## 13. Next

1. Scaffold the types as Rust in a sibling crate or directory. The
   first awkward write — the first type that resists construction
   against a real claim — is the next thing to revisit.
2. Build the normative manifest-to-Typed-Trust binding spec referenced
   in §11. Without it, the seam between layers is informal.
3. Test the contract against a second real claim (peer-reviewed, with
   active challenges) once active challenges exist in the corpus.
