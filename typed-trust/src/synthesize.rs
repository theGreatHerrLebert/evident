//! Synthesis — see §4/§8 of `concepts/typed-trust.md`.
//!
//! Takes a claim, its translated criteria, its evidence, and any
//! ReviewEvents targeting the report, and produces a [`TrustReport`].
//! Deterministic: same inputs → same output. Per invariant 2,
//! synthesis introduces no new judgment by anyone — it compares
//! observed values against tolerances, applies the §8 rule for
//! render status, and assembles the report.

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
pub fn synthesize(
    claim: ClaimId,
    criteria: Vec<TranslatedCriterion>,
    evidence: &[Evidence],
    review_events: &[ReviewEvent],
    at: Timestamp,
) -> TrustReport {
    let synth = synthesizer_identity();
    let result_criteria: Vec<Criterion> = criteria
        .into_iter()
        .map(|tc| build_criterion(tc, evidence, &synth, &at))
        .collect();

    let status = compute_render_status(&claim, &result_criteria, review_events);

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
    let name = name_for_tolerance(&tc.tolerance);
    let result = synthesize_result(&tc.id, &tc.tolerance, evidence, synth, at);
    Criterion {
        id: tc.id,
        name,
        tolerance: Some(tc.tolerance),
        result,
    }
}

fn synthesize_result(
    criterion_id: &CriterionId,
    tol: &Tolerance,
    evidence: &[Evidence],
    synth: &Identity,
    at: &Timestamp,
) -> Attested<CriterionResult> {
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
) -> RenderStatus {
    // Supersede first.
    if events.iter().any(|e| {
        matches!(&e.kind, ReviewKind::Supersede { .. })
            && target_touches_report(&e.target, claim_id, criteria)
    }) {
        return RenderStatus::Superseded;
    }

    // Then substantive challenges.
    let has_substantive_challenge = events.iter().any(|e| match &e.kind {
        ReviewKind::Challenge {
            category,
            backed_by,
        } => {
            let can_move = is_procedural_category(category) || backed_by.is_some();
            can_move && target_touches_report(&e.target, claim_id, criteria)
        }
        _ => false,
    });
    if has_substantive_challenge {
        RenderStatus::Contested
    } else {
        RenderStatus::Current
    }
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
        Target::TrustReport(_) => true,
        _ => false,
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
