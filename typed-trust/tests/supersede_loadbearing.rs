//! Phase 2d load-bearing first test (codex review-2 recommended).
//!
//! Single property: a Challenge contests a claim's report; a
//! Supersede event in the same claim slice targeting that Challenge
//! removes it from `active_verdicts`, clears `contested_by`, AND
//! makes the synthesized + rendered status `Current`.
//!
//! If this test passes, the projection-fed-into-synthesize
//! architecture works end-to-end. Everything else in Phase 2d
//! (counter naming, audit ordering, cross-claim semantics) is polish
//! on top of this foundation.

use std::collections::HashSet;

use typed_trust::derivation::{Attested, Derivation, ToolInvocation};
use typed_trust::identity::{Identity, IdentityDetail, IdentityKind};
use typed_trust::ids::{ClaimId, CriterionId, EventId};
use typed_trust::report::{
    ComparisonOp, Criterion, CriterionResult, RenderStatus, Tolerance, TrustReport,
};
use typed_trust::review::{ChallengeCategory, ReviewEvent, ReviewKind, Target};
use typed_trust::{render_augmented, render_markdown, synthesize, RenderInput};
use typed_trust::translate::TranslatedCriterion;
use typed_trust::evidence::Evidence;

const CLAIM_ID: &str = "ball-electrostatic-ci";

fn synth_identity() -> Identity {
    Identity {
        kind: IdentityKind::Automated,
        name: "typed-trust-synth".into(),
        details: vec![],
    }
}

fn model_author(name: &str, version: &str) -> Identity {
    Identity {
        kind: IdentityKind::Model,
        name: name.into(),
        details: vec![IdentityDetail {
            key: "version".into(),
            value: version.into(),
        }],
    }
}

fn criterion_id() -> CriterionId {
    CriterionId::new(format!("{CLAIM_ID}-criterion-0"))
}

fn challenge_event(eid: &str, claim_backing: Option<&str>) -> ReviewEvent {
    // Use a PROCEDURAL category (CommandFailure) so the Challenge
    // moves status without requiring a backing claim. The
    // load-bearing property — Supersede correctly removes the
    // Challenge from active_verdicts + clears contested_by — is
    // independent of category; we use procedural here to keep the
    // test focused on the projection wiring rather than the
    // backing-claim sustain rule.
    ReviewEvent {
        id: EventId::new(eid),
        target: Target::Claim(ClaimId::new(CLAIM_ID)),
        by: model_author("claude-opus-4-7", "20250101"),
        protocol: None,
        rationale: "Cited command failed; reproducibility blocked.".into(),
        at: "2026-06-02T10:00:00Z".into(),
        kind: ReviewKind::Challenge {
            category: ChallengeCategory::CommandFailure,
            backed_by: claim_backing.map(|c| ClaimId::new(c)),
        },
    }
}

fn supersede_event(eid: &str, target_event: &str) -> ReviewEvent {
    ReviewEvent {
        id: EventId::new(eid),
        target: Target::ReviewEvent(EventId::new(target_event)),
        by: model_author("claude-opus-4-7", "20260601"),
        protocol: None,
        rationale: "Re-reviewed the digest; the cited value is a known artifact. Withdraw the prior Challenge.".into(),
        at: "2026-06-15T09:00:00Z".into(),
        kind: ReviewKind::Supersede {
            successor: typed_trust::ids::AttestedId::new("att-replacement"),
        },
    }
}

fn passing_criterion() -> TranslatedCriterion {
    TranslatedCriterion {
        id: criterion_id(),
        tolerance: Some(Tolerance {
            metric: "electrostatic_error".into(),
            op: ComparisonOp::Lt,
            value: 0.02,
            output: None,
            against: None,
            prose: "stay under 2 percent relative error".into(),
        }),
        prose: "stay under 2 percent relative error".into(),
    }
}

/// THE load-bearing test. Without Phase 2d this fails:
/// `synthesize` reads the raw event slice, sees the Challenge,
/// flips status to Contested. With Phase 2d, `synthesize` (and
/// `render_augmented`) read the projection's `active_verdicts`
/// and exclude superseded Challenges.
#[test]
fn supersede_on_challenge_flips_status_back_to_current() {
    let now = "2026-06-15T10:00:00Z".to_string();
    let events = vec![
        challenge_event("evt-original-challenge", None),
        supersede_event("evt-supersede", "evt-original-challenge"),
    ];

    let report = synthesize(
        ClaimId::new(CLAIM_ID),
        vec![passing_criterion()],
        &[],
        &events,
        &[],
        &HashSet::new(),
        now.clone(),
    );

    // 1. Synthesizer status: target falls back to Current.
    assert_eq!(
        report.status,
        RenderStatus::Current,
        "expected Current after Supersede; got {:?} — synthesize did not consume active_verdicts",
        report.status,
    );

    // 2. Render-aux: contested_by on the criterion is empty.
    let augmented = render_augmented(&RenderInput {
        report: &report,
        evidence: &[],
        related_events: &events,
        backing_reports: &[],
        cycle_contested: &HashSet::new(),
        metadata: None,
        concordance: None,
        concordance_result: None,
    });
    let crit = &augmented["criteria"][0];
    let contested_by = crit["result"].get("contested_by");
    let empty = match contested_by {
        Some(arr) => arr.as_array().map(|a| a.is_empty()).unwrap_or(true),
        None => true,
    };
    assert!(
        empty,
        "contested_by should be empty after the Challenge is superseded; got {contested_by:?}",
    );

    // 3. Rendered markdown: status reads Current, not Contested.
    let md = render_markdown(&augmented);
    assert!(
        md.contains("Current"),
        "rendered markdown should report Current; got:\n{md}",
    );
    assert!(
        !md.contains("Contested"),
        "rendered markdown should NOT report Contested after Supersede; got:\n{md}",
    );

    // 4. Sanity: a control run without the Supersede event flips
    //    the report to Contested. This confirms the test would
    //    have flagged a regression if we didn't apply Supersede.
    let control_report = synthesize(
        ClaimId::new(CLAIM_ID),
        vec![passing_criterion()],
        &[],
        &[challenge_event("evt-original-challenge", None)],
        &[],
        &HashSet::new(),
        now,
    );
    assert_eq!(
        control_report.status,
        RenderStatus::Contested,
        "control: without Supersede the Challenge must still flip status to Contested",
    );
}

// Silence dead_code warnings on the helpers if a future refactor
// drops one. The four helpers are all referenced by the test today.
#[allow(dead_code)]
fn _helpers_used(_: Identity, _: Attested<CriterionResult>) {
    let _ = synth_identity;
}
