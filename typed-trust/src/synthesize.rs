//! Synthesis — see §4/§8 of `concepts/typed-trust.md`.
//!
//! Takes a claim, its translated criteria, its evidence, and any
//! ReviewEvents targeting the report, and produces a [`TrustReport`].
//! Deterministic: same inputs → same output. Per invariant 2,
//! synthesis introduces no new judgment by anyone — it compares
//! observed values against tolerances, applies the §8 rule for
//! render status, and assembles the report.

use std::collections::HashSet;

use crate::derivation::{Attested, Derivation, Rerun, ToolInvocation};
use crate::identity::{Identity, IdentityKind};
use crate::evidence::Evidence;
use crate::ids::{ClaimId, CriterionId, EventId, Timestamp};
use crate::report::{
    ComparisonOp, Criterion, CriterionResult, RenderStatus, Tolerance, TrustReport,
};
use crate::review::{ChallengeCategory, ReviewEvent, ReviewKind, Target};
use crate::translate::TranslatedCriterion;

/// Build a [`TrustReport`] from a claim + criteria + evidence + review events.
///
/// For each criterion:
/// - Find the latest [`MetricObservation`](crate::report::MetricObservation)
///   matching this criterion's id, across all the evidence's reruns.
/// - Compare it against the tolerance using the criterion's `op`.
/// - Result is `Pass` if true, `Fail` if false, or `NotAssessed` if
///   there's no matching observation (CI tier without a populated
///   `last_verified`).
///
/// Then apply the §8 rule for render status:
/// - `Superseded` if a `Supersede` event targets the report or any criterion.
/// - `Contested` if a `Challenge` event targets the report or a criterion
///   AND is either procedural (closed category list) or has a backing
///   claim.
/// - `Current` otherwise.
///
/// `challenges` in the output is the list of EventIds whose target
/// touches the report (Claim, Criterion, or CriterionResult).
/// `backing_reports` carries already-synthesized reports for any
/// claims referenced via `Challenge { backed_by: Some(...) }`. They
/// are consulted to decide whether a substantive challenge actually
/// sustains: a backing report whose `status == Current` sustains the
/// challenge; any other status (Contested, Superseded), or no backing
/// report at all, leaves the challenge unsustained and the original
/// report is NOT moved to Contested. This is the invariant 6 / §8
/// behavior; passing `&[]` means "no challenge can sustain via
/// backing," only procedural challenges can move status.
pub fn synthesize(
    claim: ClaimId,
    criteria: Vec<TranslatedCriterion>,
    evidence: &[Evidence],
    review_events: &[ReviewEvent],
    backing_reports: &[TrustReport],
    at: Timestamp,
) -> TrustReport {
    let synth = synthesizer_identity();
    let result_criteria: Vec<Criterion> = criteria
        .into_iter()
        .map(|tc| build_criterion(tc, evidence, &synth, &at))
        .collect();

    let status = compute_render_status(
        &claim,
        &result_criteria,
        review_events,
        backing_reports,
    );

    let challenges: Vec<EventId> = review_events
        .iter()
        .filter(|e| matches!(&e.kind, ReviewKind::Challenge { .. }))
        .filter(|e| target_touches_report(&e.target, &claim, &result_criteria))
        .map(|e| e.id.clone())
        .collect();

    TrustReport {
        claim,
        status,
        criteria: result_criteria,
        challenges,
        gaps: vec![],
        aggregate: None,
    }
}

fn build_criterion(
    tc: TranslatedCriterion,
    evidence: &[Evidence],
    synth: &Identity,
    at: &Timestamp,
) -> Criterion {
    let name = name_for_translated_criterion(&tc);
    let result = synthesize_result(&tc.id, tc.tolerance.as_ref(), evidence, synth, at);
    Criterion {
        id: tc.id,
        name,
        tolerance: tc.tolerance,
        result,
    }
}

fn synthesize_result(
    criterion_id: &CriterionId,
    tol: Option<&Tolerance>,
    evidence: &[Evidence],
    synth: &Identity,
    at: &Timestamp,
) -> Attested<CriterionResult> {
    // Prose-only tolerance: no structured threshold to apply. Always
    // NotAssessed with a documented reason.
    let Some(tol) = tol else {
        return Attested {
            value: CriterionResult::NotAssessed {
                reason: "no structured tolerance (prose-only)".into(),
            },
            derivation: Derivation::Verified {
                method: ToolInvocation {
                    command: "rule:NoStructuredTolerance".into(),
                    tool_version: format!(
                        "typed-trust-synth {}",
                        env!("CARGO_PKG_VERSION")
                    ),
                    env: vec![],
                },
                ran_by: synth.clone(),
                reruns: vec![],
            },
            at: at.clone(),
        };
    };

    let latest = latest_observation_for(criterion_id, evidence);

    let (value, rule) = match latest {
        Some((rerun_at, observed)) => {
            let passes = apply_op(tol.op, observed, tol.value);
            let cr = if passes {
                CriterionResult::Pass
            } else {
                CriterionResult::Fail
            };
            let rule = format!(
                "rule:{:?}(observed={}, tolerance={}) at {}",
                tol.op, observed, tol.value, rerun_at
            );
            (cr, rule)
        }
        None => (
            CriterionResult::NotAssessed {
                reason: "no observation in evidence for this criterion".into(),
            },
            "rule:NoObservation".into(),
        ),
    };

    Attested {
        value,
        derivation: Derivation::Verified {
            method: ToolInvocation {
                command: rule,
                tool_version: format!(
                    "typed-trust-synth {}",
                    env!("CARGO_PKG_VERSION")
                ),
                env: vec![],
            },
            ran_by: synth.clone(),
            reruns: vec![],
        },
        at: at.clone(),
    }
}

/// Return `(rerun.at, observation.value)` for the most recent rerun
/// across all evidence whose observation matches this criterion.
fn latest_observation_for<'a>(
    criterion_id: &CriterionId,
    evidence: &'a [Evidence],
) -> Option<(&'a str, f64)> {
    let mut best: Option<(&str, f64)> = None;
    for ev in evidence {
        let reruns: &[Rerun] = match &ev.extraction {
            Derivation::Verified { reruns, .. } => reruns,
            _ => continue,
        };
        for r in reruns {
            for obs in &r.observed {
                if &obs.criterion == criterion_id {
                    match best {
                        Some((cur_at, _)) if cur_at >= r.at.as_str() => {}
                        _ => best = Some((r.at.as_str(), obs.value)),
                    }
                }
            }
        }
    }
    best
}

fn apply_op(op: ComparisonOp, observed: f64, threshold: f64) -> bool {
    match op {
        ComparisonOp::Lt => observed < threshold,
        ComparisonOp::LtEq => observed <= threshold,
        ComparisonOp::GtEq => observed >= threshold,
        ComparisonOp::Gt => observed > threshold,
        ComparisonOp::Eq => observed == threshold,
    }
}

fn compute_render_status(
    claim_id: &ClaimId,
    criteria: &[Criterion],
    events: &[ReviewEvent],
    backing_reports: &[TrustReport],
) -> RenderStatus {
    // Supersede first.
    if events.iter().any(|e| {
        matches!(&e.kind, ReviewKind::Supersede { .. })
            && target_touches_report(&e.target, claim_id, criteria)
    }) {
        return RenderStatus::Superseded;
    }

    // Then substantive challenges. The challenge moves render status if:
    // - the category is procedural (closed list); or
    // - backed_by points at a backing report that synthesizes to Current
    //   (i.e., the backing claim's criteria pass on their own merits).
    // A backed_by claim id with no matching backing report, or a backing
    // report with status != Current, does NOT sustain the challenge.
    let has_sustained_challenge = events.iter().any(|e| match &e.kind {
        ReviewKind::Challenge {
            category,
            backed_by,
        } => {
            let proc_can_move = is_procedural_category(category);
            let backed_can_move = backed_by
                .as_ref()
                .map(|bid| backing_report_sustains(bid, backing_reports))
                .unwrap_or(false);
            (proc_can_move || backed_can_move)
                && target_touches_report(&e.target, claim_id, criteria)
        }
        _ => false,
    });
    if has_sustained_challenge {
        RenderStatus::Contested
    } else {
        RenderStatus::Current
    }
}

/// A backing claim sustains a challenge iff its TrustReport
/// synthesizes to a `Current` status — meaning the backing claim's
/// own criteria pass on their own merits and no challenges against IT
/// were sustained. `Contested` or `Superseded` backing reports do not
/// sustain.
///
/// Per design §8, the rule is that the backing claim's TrustReport
/// "synthesizes to a passing-criteria result." That is strictly
/// stronger than `status == Current`: a claim with no challenges but
/// `Fail`/`NotAssessed` criteria is also `Current`, but its criteria
/// did not pass. A sustain check therefore requires BOTH:
/// - `status == Current` (not contested/superseded), AND
/// - at least one criterion exists, AND
/// - every criterion's result is `Pass`.
///
/// An empty-criteria backing report ("no evaluable proposition") does
/// not sustain.
pub(crate) fn backing_report_sustains(
    backing_id: &ClaimId,
    backing_reports: &[TrustReport],
) -> bool {
    backing_reports
        .iter()
        .find(|r| &r.claim == backing_id)
        .is_some_and(|r| {
            r.status == RenderStatus::Current
                && !r.criteria.is_empty()
                && r.criteria
                    .iter()
                    .all(|c| matches!(c.result.value, CriterionResult::Pass))
        })
}

/// Closed list of procedural categories that may move render status
/// without a backing claim (per §6 in the design doc).
pub(crate) fn is_procedural_category(cat: &ChallengeCategory) -> bool {
    matches!(
        cat,
        ChallengeCategory::ArtifactUnavailable
            | ChallengeCategory::HashMismatch
            | ChallengeCategory::CommandFailure
            | ChallengeCategory::ConflictOfInterest
            | ChallengeCategory::PeerReviewUnverifiable
    )
}

/// Whether a [`Target`] points at this report (its Claim, a Criterion
/// in it, or a CriterionResult).
fn target_touches_report(target: &Target, claim_id: &ClaimId, criteria: &[Criterion]) -> bool {
    match target {
        Target::Claim(c) => c == claim_id,
        Target::Criterion(cid) => criteria.iter().any(|c| &c.id == cid),
        Target::CriterionResult { criterion, .. } => {
            criteria.iter().any(|c| &c.id == criterion)
        }
        // TrustReport-targeted events cannot be matched until TrustReport
        // carries its own ReportId. Returning `true` here was a bug —
        // when callers pass shared review-event slices while synthesizing
        // multiple reports, every report would falsely consider any
        // Target::TrustReport(_) event as targeting it. Conservative
        // fallback: don't match. The full fix is to add an `id: ReportId`
        // field to TrustReport so reports can disambiguate.
        Target::TrustReport(_) => false,
        _ => false,
    }
}

fn name_for_translated_criterion(tc: &TranslatedCriterion) -> String {
    match tc.tolerance.as_ref() {
        Some(tol) => name_for_tolerance(tol),
        None => {
            // Prose-only — surface the first line of the prose so the
            // criterion has a usable label even without a structured
            // threshold.
            let first_line = tc.prose.lines().next().unwrap_or("(prose-only)");
            format!("(prose-only) {first_line}")
        }
    }
}

fn name_for_tolerance(tol: &Tolerance) -> String {
    let op_str = match tol.op {
        ComparisonOp::Lt => "<",
        ComparisonOp::LtEq => "<=",
        ComparisonOp::GtEq => ">=",
        ComparisonOp::Gt => ">",
        ComparisonOp::Eq => "==",
    };
    let mut s = format!("{} {} {}", tol.metric, op_str, tol.value);
    if let Some(output) = &tol.output {
        s.push_str(" on ");
        s.push_str(output);
    }
    if let Some(against) = &tol.against {
        s.push_str(" vs ");
        s.push_str(against);
    }
    s
}

fn synthesizer_identity() -> Identity {
    Identity {
        kind: IdentityKind::Automated,
        name: "evident-synthesizer".into(),
        details: vec![],
    }
}

// ---------- Recursive backing-report synthesis ----------

/// The inputs synthesize() needs for one backing claim.
#[derive(Debug, Clone)]
pub struct BackingClaimInputs {
    pub criteria: Vec<TranslatedCriterion>,
    pub evidence: Vec<Evidence>,
    pub review_events: Vec<ReviewEvent>,
}

/// Source of backing-claim inputs. A caller provides this — could be
/// a HashMap, a manifest reader, a network lookup. The trait keeps the
/// recursion logic independent of how claims are persisted.
pub trait ClaimLookup {
    fn lookup(&self, claim_id: &ClaimId) -> Option<BackingClaimInputs>;
}

/// Walk Challenge events for `backed_by: Some(...)`, look up each
/// backing claim via `lookup`, recursively synthesize it, and collect
/// the resulting TrustReports.
///
/// Cycle handling: a first pass via [`detect_cycle_members`] identifies
/// claim ids that lie on a cycle in the challenge-backing graph. During
/// synthesis, those claims' TrustReports get `status: Contested` per
/// design §8 ("Contested if the graph reachable from it contains a
/// cycle in challenge edges") — the cycle is surfaced as Contested
/// rather than silently dropping out. `max_depth` still bounds the
/// recursion for pathologically long non-cyclic chains.
pub fn compute_backing_reports(
    initial_events: &[ReviewEvent],
    lookup: &dyn ClaimLookup,
    at: &str,
    max_depth: usize,
) -> Vec<TrustReport> {
    // First pass: identify all claim ids that lie on a cycle.
    let cycled = detect_cycle_members(initial_events, lookup);

    // Second pass: actually synthesize, marking cycled claims Contested.
    let mut backing = Vec::new();
    let mut visited: HashSet<ClaimId> = HashSet::new();
    for event in initial_events {
        if let ReviewKind::Challenge {
            backed_by: Some(cid),
            ..
        } = &event.kind
        {
            walk_backing(
                cid, lookup, &cycled, &mut visited, &mut backing, at, 0, max_depth,
            );
        }
    }
    backing
}

/// Pre-pass cycle detection on the challenge-backing graph. Returns the
/// set of claim ids that participate in any cycle. Uses recursion-stack
/// DFS — a back edge to a claim on the current stack means every claim
/// from that point on the stack lies in a cycle.
fn detect_cycle_members(
    initial_events: &[ReviewEvent],
    lookup: &dyn ClaimLookup,
) -> HashSet<ClaimId> {
    let mut cycled: HashSet<ClaimId> = HashSet::new();
    let mut visited: HashSet<ClaimId> = HashSet::new();
    let mut stack: Vec<ClaimId> = Vec::new();

    for event in initial_events {
        if let ReviewKind::Challenge {
            backed_by: Some(cid),
            ..
        } = &event.kind
        {
            cycle_dfs(cid, lookup, &mut visited, &mut stack, &mut cycled);
        }
    }
    cycled
}

fn cycle_dfs(
    claim_id: &ClaimId,
    lookup: &dyn ClaimLookup,
    visited: &mut HashSet<ClaimId>,
    stack: &mut Vec<ClaimId>,
    cycled: &mut HashSet<ClaimId>,
) {
    // Back edge to current stack → cycle. Mark every claim from the
    // back-edge target to the current top of the stack.
    if let Some(idx) = stack.iter().position(|c| c == claim_id) {
        for c in &stack[idx..] {
            cycled.insert(c.clone());
        }
        return;
    }
    if visited.contains(claim_id) {
        return;
    }
    visited.insert(claim_id.clone());
    stack.push(claim_id.clone());

    if let Some(inputs) = lookup.lookup(claim_id) {
        for event in &inputs.review_events {
            if let ReviewKind::Challenge {
                backed_by: Some(b),
                ..
            } = &event.kind
            {
                cycle_dfs(b, lookup, visited, stack, cycled);
            }
        }
    }

    stack.pop();
}

#[allow(clippy::too_many_arguments)]
fn walk_backing(
    claim_id: &ClaimId,
    lookup: &dyn ClaimLookup,
    cycled: &HashSet<ClaimId>,
    visited: &mut HashSet<ClaimId>,
    backing: &mut Vec<TrustReport>,
    at: &str,
    depth: usize,
    max_depth: usize,
) {
    if depth >= max_depth || visited.contains(claim_id) {
        return;
    }
    visited.insert(claim_id.clone());

    let Some(inputs) = lookup.lookup(claim_id) else {
        return;
    };

    let events = inputs.review_events.clone();

    // Recurse first (depth-first) so a backing claim's nested backings
    // are known before its TrustReport is synthesized.
    let backing_start = backing.len();
    for event in &events {
        if let ReviewKind::Challenge {
            backed_by: Some(b),
            ..
        } = &event.kind
        {
            walk_backing(
                b,
                lookup,
                cycled,
                visited,
                backing,
                at,
                depth + 1,
                max_depth,
            );
        }
    }
    let nested_backing: Vec<TrustReport> = backing[backing_start..].to_vec();

    let mut report = synthesize(
        claim_id.clone(),
        inputs.criteria,
        &inputs.evidence,
        &events,
        &nested_backing,
        at.to_string(),
    );

    // Apply the cycle rule: any claim that lies on a cycle in the
    // challenge-backing graph is surfaced as Contested, overriding the
    // pure §8 sustain rollup. Cycles cannot be resolved deterministically
    // and the reader needs to know.
    if cycled.contains(claim_id) {
        report.status = RenderStatus::Contested;
    }

    backing.push(report);
}
