//! Render-aux — JSON-to-JSON post-process that adds the consumer
//! conveniences described in `concepts/typed-trust-json-shape.md`:
//!
//! - `criteria[*].result.observed_value` — joined from the latest
//!   [`MetricObservation`](crate::report::MetricObservation) matching
//!   the criterion id, across all evidence reruns.
//! - `criteria[*].result.criterion_status` — per-criterion
//!   `Current | Superseded | Contested`, computed by the §8 rule.
//! - `criteria[*].result.contested_by` — EventIds of Challenge events
//!   targeting this criterion.
//! - `_graph.review_events` — inline the related ReviewEvents so a
//!   reader doesn't have to fetch them separately.
//! - `_graph.backing_reports` — inline any precomputed
//!   backing-claim TrustReports (the recursive synthesis result).
//!
//! These fields are NOT normative — the v0.7 type definitions don't
//! carry them. They live in the JSON output as renderer convenience.
//! Consumers that only want the normative graph can ignore them.

use serde_json::{json, Map, Value};

use crate::derivation::{Derivation, Rerun};
use crate::evidence::Evidence;
use crate::ids::CriterionId;
use crate::report::{RenderStatus, TrustReport};
use crate::review::{ReviewEvent, ReviewKind, Target};
use crate::synthesize::is_procedural_category;

/// Inputs to the render-aux layer. Borrows so a caller that already
/// has the report + evidence + events doesn't have to clone.
pub struct RenderInput<'a> {
    pub report: &'a TrustReport,
    /// Evidence the report's criteria reference. Used to look up
    /// observed values for `observed_value`.
    pub evidence: &'a [Evidence],
    /// ReviewEvents that target this report or its criteria. Used to
    /// compute `criterion_status`, `contested_by`, and `_graph.review_events`.
    pub related_events: &'a [ReviewEvent],
    /// Precomputed TrustReports for any backing claims of substantive
    /// challenges. Inlined into `_graph.backing_reports`.
    pub backing_reports: &'a [TrustReport],
}

/// Produce the augmented JSON. The normative report is serialized first;
/// renderer-aux fields are added in-place.
pub fn render_augmented(input: &RenderInput) -> Value {
    let mut json = serde_json::to_value(input.report).expect("serialize TrustReport");

    if let Some(criteria) = json.get_mut("criteria").and_then(Value::as_array_mut) {
        for crit_json in criteria.iter_mut() {
            augment_criterion(crit_json, input);
        }
    }

    if let Some(obj) = json.as_object_mut() {
        if let Some(graph) = build_graph_aux(input) {
            obj.insert("_graph".into(), graph);
        }
    }

    json
}

fn augment_criterion(crit_json: &mut Value, input: &RenderInput) {
    let Some(crit_id_str) = crit_json.get("id").and_then(Value::as_str) else {
        return;
    };
    let crit_id = CriterionId::new(crit_id_str);

    let observed = latest_observation_for(&crit_id, input.evidence);
    let crit_status =
        compute_criterion_status(&crit_id, input.related_events, input.backing_reports);
    let contested_by: Vec<String> = input
        .related_events
        .iter()
        .filter(|e| matches!(&e.kind, ReviewKind::Challenge { .. }))
        .filter(|e| event_targets_criterion(&e.target, &crit_id))
        .map(|e| e.id.as_str().to_string())
        .collect();

    let Some(result) = crit_json.get_mut("result").and_then(Value::as_object_mut) else {
        return;
    };

    if let Some(v) = observed {
        if let Some(n) = serde_json::Number::from_f64(v) {
            result.insert("observed_value".into(), Value::Number(n));
        }
    }

    result.insert(
        "criterion_status".into(),
        Value::String(status_label(&crit_status).into()),
    );

    if !contested_by.is_empty() {
        result.insert("contested_by".into(), json!(contested_by));
    }
}

fn latest_observation_for(criterion_id: &CriterionId, evidence: &[Evidence]) -> Option<f64> {
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
    best.map(|(_, v)| v)
}

fn compute_criterion_status(
    criterion_id: &CriterionId,
    events: &[ReviewEvent],
    backing_reports: &[crate::report::TrustReport],
) -> RenderStatus {
    if events.iter().any(|e| {
        matches!(&e.kind, ReviewKind::Supersede { .. })
            && event_targets_criterion(&e.target, criterion_id)
    }) {
        return RenderStatus::Superseded;
    }
    // Same §8 sustain rule as synthesize::compute_render_status: a
    // backing claim id only sustains if the matching backing report
    // synthesizes to Current.
    if events.iter().any(|e| match &e.kind {
        ReviewKind::Challenge {
            category,
            backed_by,
        } => {
            let proc_can_move = is_procedural_category(category);
            let backed_can_move = backed_by.as_ref().is_some_and(|bid| {
                backing_reports
                    .iter()
                    .find(|r| &r.claim == bid)
                    .is_some_and(|r| r.status == RenderStatus::Current)
            });
            (proc_can_move || backed_can_move)
                && event_targets_criterion(&e.target, criterion_id)
        }
        _ => false,
    }) {
        return RenderStatus::Contested;
    }
    RenderStatus::Current
}

fn event_targets_criterion(target: &Target, criterion_id: &CriterionId) -> bool {
    match target {
        Target::Criterion(c) => c == criterion_id,
        Target::CriterionResult { criterion, .. } => criterion == criterion_id,
        _ => false,
    }
}

fn build_graph_aux(input: &RenderInput) -> Option<Value> {
    let mut graph = Map::new();
    if !input.related_events.is_empty() {
        let events: Vec<Value> = input
            .related_events
            .iter()
            .map(|e| serde_json::to_value(e).expect("serialize ReviewEvent"))
            .collect();
        graph.insert("review_events".into(), Value::Array(events));
    }
    if !input.backing_reports.is_empty() {
        let reports: Vec<Value> = input
            .backing_reports
            .iter()
            .map(|r| serde_json::to_value(r).expect("serialize TrustReport"))
            .collect();
        graph.insert("backing_reports".into(), Value::Array(reports));
    }
    if graph.is_empty() {
        None
    } else {
        Some(Value::Object(graph))
    }
}

fn status_label(s: &RenderStatus) -> &'static str {
    match s {
        RenderStatus::Current => "current",
        RenderStatus::Superseded => "superseded",
        RenderStatus::Contested => "contested",
    }
}
