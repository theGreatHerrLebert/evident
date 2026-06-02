//! Phase 2c: tests for the `_graph.panel_summary` aux projection
//! and its markdown rendering.

use std::collections::HashSet;

use typed_trust::derivation::{Attested, Derivation, ToolInvocation};
use typed_trust::identity::{Identity, IdentityDetail, IdentityKind};
use typed_trust::ids::{ClaimId, CriterionId, EventId};
use typed_trust::report::{
    ComparisonOp, Criterion, CriterionResult, RenderStatus, Tolerance, TrustReport,
};
use typed_trust::review::{ChallengeCategory, ReviewEvent, ReviewKind, Target};
use typed_trust::{render_augmented, render_markdown, RenderInput};

fn iso(s: &str) -> String {
    s.to_string()
}

fn synth_identity() -> Identity {
    Identity {
        kind: IdentityKind::Automated,
        name: "typed-trust-synth".into(),
        details: vec![],
    }
}

/// Minimal in-memory TrustReport for rendering tests. Real synthesis
/// is exercised elsewhere; here we just need a valid report shell so
/// render_augmented can do its projection over related_events.
fn minimal_report(claim_id: &str) -> TrustReport {
    let synth = synth_identity();
    let at = iso("2026-06-02T00:00:00Z");
    let criterion_id = CriterionId::new(format!("{claim_id}-criterion-0"));
    let tolerance = Tolerance {
        metric: "relative_error".into(),
        op: ComparisonOp::Lt,
        value: 0.02,
        output: None,
        against: None,
        prose: "test".into(),
    };
    let result = Attested {
        value: CriterionResult::Pass,
        derivation: Derivation::Verified {
            method: ToolInvocation {
                command: "synth".into(),
                tool_version: "test".into(),
                env: vec![],
            },
            ran_by: synth.clone(),
            reruns: vec![],
        },
        at: at.clone(),
    };
    TrustReport {
        claim: ClaimId::new(claim_id),
        status: RenderStatus::Current,
        criteria: vec![Criterion {
            id: criterion_id,
            name: "relative_error < 0.02".into(),
            tolerance: Some(tolerance),
            result,
        }],
        challenges: vec![],
        gaps: vec![],
        aggregate: None,
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

fn human_author(name: &str) -> Identity {
    Identity {
        kind: IdentityKind::Human,
        name: name.into(),
        details: vec![],
    }
}

fn event(claim_id: &str, eid: &str, by: Identity, kind: ReviewKind, at: &str) -> ReviewEvent {
    ReviewEvent {
        id: EventId::new(eid),
        target: Target::Claim(ClaimId::new(claim_id)),
        by,
        protocol: None,
        rationale: "rationale that's plenty long for the validator to accept it.".into(),
        at: iso(at),
        kind,
    }
}

fn render(report: &TrustReport, events: &[ReviewEvent]) -> serde_json::Value {
    render_augmented(&RenderInput {
        report,
        evidence: &[],
        related_events: events,
        backing_reports: &[],
        cycle_contested: &HashSet::new(),
    })
}

// ---------- panel_summary shape ----------

#[test]
fn panel_summary_counts_events_and_distinct_reviewers() {
    let claim = "ball-electrostatic-ci";
    let report = minimal_report(claim);
    let events = vec![
        event(
            claim,
            "evt-1",
            model_author("claude-opus-4-7", "20250101"),
            ReviewKind::Endorse,
            "2026-06-02T10:00:00Z",
        ),
        event(
            claim,
            "evt-2",
            model_author("claude-opus-4-7", "20250101"),
            ReviewKind::Dissent,
            "2026-06-02T10:05:00Z",
        ),
        event(
            claim,
            "evt-3",
            model_author("claude-haiku-4-5", "20251001"),
            ReviewKind::Endorse,
            "2026-06-02T10:10:00Z",
        ),
    ];
    let augmented = render(&report, &events);
    let panel = &augmented["_graph"]["panel_summary"];
    // 3 events but only 2 distinct reviewers — codex F-2C-4.
    assert_eq!(panel["n_events"].as_u64(), Some(3));
    assert_eq!(panel["n_reviewers"].as_u64(), Some(2));
    assert_eq!(panel["n_endorse"].as_u64(), Some(2));
    assert_eq!(panel["n_dissent"].as_u64(), Some(1));
    assert_eq!(panel["n_challenge"].as_u64(), Some(0));
}

#[test]
fn panel_summary_breaks_down_by_identity_kind() {
    let claim = "ball-electrostatic-ci";
    let report = minimal_report(claim);
    let events = vec![
        event(
            claim,
            "evt-1",
            model_author("claude-opus-4-7", "20250101"),
            ReviewKind::Endorse,
            "2026-06-02T10:00:00Z",
        ),
        event(
            claim,
            "evt-2",
            human_author("Jane Doe"),
            ReviewKind::Endorse,
            "2026-06-02T10:05:00Z",
        ),
    ];
    let augmented = render(&report, &events);
    let panel = &augmented["_graph"]["panel_summary"];
    // F-2C-3: humans and models share the same panel; by_kind tallies both.
    assert_eq!(panel["by_kind"]["model"].as_u64(), Some(1));
    assert_eq!(panel["by_kind"]["human"].as_u64(), Some(1));
    assert_eq!(panel["by_kind"]["automated"].as_u64(), Some(0));
}

#[test]
fn panel_summary_same_name_different_version_are_distinct_reviewers() {
    let claim = "ball-electrostatic-ci";
    let report = minimal_report(claim);
    let events = vec![
        event(
            claim,
            "evt-1",
            model_author("claude-opus-4-7", "20250101"),
            ReviewKind::Endorse,
            "2026-06-02T10:00:00Z",
        ),
        event(
            claim,
            "evt-2",
            model_author("claude-opus-4-7", "20260601"),
            ReviewKind::Dissent,
            "2026-06-02T10:05:00Z",
        ),
    ];
    let augmented = render(&report, &events);
    let panel = &augmented["_graph"]["panel_summary"];
    // F-2C-7: same name, different version → distinct reviewers.
    assert_eq!(panel["n_reviewers"].as_u64(), Some(2));
}

#[test]
fn panel_summary_same_name_different_orcid_are_distinct_reviewers_codex_2c_cr2() {
    // Codex F-CR2C-2 regression: two humans named "Jane Doe" with
    // different orcids must be counted as distinct reviewers. The
    // prior (kind, name, version) key collapsed them — no version
    // field meant both keyed to ("human", "Jane Doe", "") and the
    // panel undercounted n_reviewers.
    let claim = "x";
    let report = minimal_report(claim);
    let jane_a = Identity {
        kind: IdentityKind::Human,
        name: "Jane Doe".into(),
        details: vec![IdentityDetail {
            key: "orcid".into(),
            value: "0000-0001".into(),
        }],
    };
    let jane_b = Identity {
        kind: IdentityKind::Human,
        name: "Jane Doe".into(),
        details: vec![IdentityDetail {
            key: "orcid".into(),
            value: "0000-0002".into(),
        }],
    };
    let events = vec![
        event(
            claim,
            "evt-1",
            jane_a,
            ReviewKind::Endorse,
            "2026-06-02T10:00:00Z",
        ),
        event(
            claim,
            "evt-2",
            jane_b,
            ReviewKind::Dissent,
            "2026-06-02T10:05:00Z",
        ),
    ];
    let augmented = render(&report, &events);
    let panel = &augmented["_graph"]["panel_summary"];
    assert_eq!(panel["n_reviewers"].as_u64(), Some(2));
    assert_eq!(panel["n_events"].as_u64(), Some(2));
    assert_eq!(panel["by_kind"]["human"].as_u64(), Some(2));
    // Panel section must therefore appear in the rendered markdown.
    let md = render_markdown(&augmented);
    assert!(md.contains("## Reviewer Panel"), "panel section missing");
}

#[test]
fn panel_summary_same_identity_repeated_is_one_reviewer() {
    // Symmetric case: two events from the same author identity (no
    // distinguishing details) ARE the same reviewer. n_events=2 but
    // n_reviewers=1.
    let claim = "x";
    let report = minimal_report(claim);
    let same = Identity {
        kind: IdentityKind::Human,
        name: "Jane Doe".into(),
        details: vec![],
    };
    let events = vec![
        event(
            claim,
            "evt-1",
            same.clone(),
            ReviewKind::Endorse,
            "2026-06-02T10:00:00Z",
        ),
        event(
            claim,
            "evt-2",
            same,
            ReviewKind::Dissent,
            "2026-06-02T10:05:00Z",
        ),
    ];
    let augmented = render(&report, &events);
    let panel = &augmented["_graph"]["panel_summary"];
    assert_eq!(panel["n_events"].as_u64(), Some(2));
    assert_eq!(panel["n_reviewers"].as_u64(), Some(1));
}

#[test]
fn panel_summary_verdicts_are_sorted_deterministically() {
    let claim = "ball-electrostatic-ci";
    let report = minimal_report(claim);
    // Append in reverse-sort order; assert the projection orders them
    // by (kind, name, version, timestamp, event_id).
    let events = vec![
        event(
            claim,
            "evt-z",
            model_author("zoo", "v1"),
            ReviewKind::Endorse,
            "2026-06-02T10:05:00Z",
        ),
        event(
            claim,
            "evt-a",
            model_author("aardvark", "v1"),
            ReviewKind::Endorse,
            "2026-06-02T10:00:00Z",
        ),
    ];
    let augmented = render(&report, &events);
    let rows = augmented["_graph"]["panel_summary"]["verdicts_by_reviewer"]
        .as_array()
        .unwrap();
    assert_eq!(rows[0]["author"]["name"].as_str(), Some("aardvark"));
    assert_eq!(rows[1]["author"]["name"].as_str(), Some("zoo"));
}

#[test]
fn panel_summary_challenge_rows_carry_backing_metadata() {
    let claim = "ball-electrostatic-ci";
    let report = minimal_report(claim);
    let events = vec![event(
        claim,
        "evt-1",
        model_author("claude-opus-4-7", "20250101"),
        ReviewKind::Challenge {
            category: ChallengeCategory::WeakStatistics,
            backed_by: Some(ClaimId::new("ball-electrostatic-ci-counter-abcd1234")),
        },
        "2026-06-02T10:00:00Z",
    )];
    let augmented = render(&report, &events);
    let row = &augmented["_graph"]["panel_summary"]["verdicts_by_reviewer"][0];
    assert_eq!(row["kind"].as_str(), Some("challenge"));
    assert_eq!(row["has_backing"].as_bool(), Some(true));
    assert_eq!(
        row["backed_by"].as_str(),
        Some("ball-electrostatic-ci-counter-abcd1234")
    );
}

#[test]
fn panel_summary_row_has_event_id_timestamp_structured_author() {
    let claim = "x";
    let report = minimal_report(claim);
    let events = vec![event(
        claim,
        "evt-stable-id",
        model_author("m", "v"),
        ReviewKind::Endorse,
        "2026-06-02T10:00:00Z",
    )];
    let augmented = render(&report, &events);
    let row = &augmented["_graph"]["panel_summary"]["verdicts_by_reviewer"][0];
    assert_eq!(row["event_id"].as_str(), Some("evt-stable-id"));
    assert_eq!(row["timestamp"].as_str(), Some("2026-06-02T10:00:00Z"));
    assert_eq!(row["author"]["kind"].as_str(), Some("model"));
    assert_eq!(row["author"]["name"].as_str(), Some("m"));
    assert_eq!(row["author"]["version"].as_str(), Some("v"));
}

// ---------- markdown rendering ----------

#[test]
fn markdown_panel_section_omitted_for_single_reviewer() {
    let claim = "x";
    let report = minimal_report(claim);
    let events = vec![event(
        claim,
        "evt-1",
        model_author("m", "v"),
        ReviewKind::Endorse,
        "2026-06-02T10:00:00Z",
    )];
    let augmented = render(&report, &events);
    let md = render_markdown(&augmented);
    assert!(
        !md.contains("## Reviewer Panel"),
        "single-reviewer report must not include the panel section; got:\n{md}"
    );
}

#[test]
fn markdown_panel_section_says_all_endorsed_on_consensus() {
    let claim = "x";
    let report = minimal_report(claim);
    let events = vec![
        event(
            claim,
            "evt-1",
            model_author("a", "v"),
            ReviewKind::Endorse,
            "2026-06-02T10:00:00Z",
        ),
        event(
            claim,
            "evt-2",
            model_author("b", "v"),
            ReviewKind::Endorse,
            "2026-06-02T10:05:00Z",
        ),
    ];
    let augmented = render(&report, &events);
    let md = render_markdown(&augmented);
    assert!(md.contains("## Reviewer Panel"), "panel section missing");
    assert!(
        md.contains("all endorsed"),
        "consensus phrasing missing; got:\n{md}"
    );
}

#[test]
fn markdown_panel_section_says_divergent_on_disagreement() {
    let claim = "x";
    let report = minimal_report(claim);
    let events = vec![
        event(
            claim,
            "evt-1",
            model_author("a", "v"),
            ReviewKind::Endorse,
            "2026-06-02T10:00:00Z",
        ),
        event(
            claim,
            "evt-2",
            model_author("b", "v"),
            ReviewKind::Dissent,
            "2026-06-02T10:05:00Z",
        ),
    ];
    let augmented = render(&report, &events);
    let md = render_markdown(&augmented);
    assert!(md.contains("Panel divergent"), "divergent phrasing missing; got:\n{md}");
}

#[test]
fn markdown_panel_footnote_when_supersede_present() {
    let claim = "x";
    let report = minimal_report(claim);
    let events = vec![
        event(
            claim,
            "evt-1",
            model_author("a", "v"),
            ReviewKind::Endorse,
            "2026-06-02T10:00:00Z",
        ),
        event(
            claim,
            "evt-2",
            model_author("b", "v"),
            ReviewKind::Supersede {
                successor: typed_trust::ids::AttestedId::new("att-1"),
            },
            "2026-06-02T10:05:00Z",
        ),
    ];
    let augmented = render(&report, &events);
    let md = render_markdown(&augmented);
    assert!(
        md.contains("Panel reflects raw attestation log"),
        "supersede footnote missing; got:\n{md}"
    );
}
