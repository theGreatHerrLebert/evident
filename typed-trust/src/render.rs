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

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use crate::claim::MetadataDeclaration;
use crate::derivation::{Derivation, Rerun};
use crate::evidence::Evidence;
use crate::ids::{ClaimId, CriterionId, EventId};
use crate::report::{RenderStatus, TrustReport};
use crate::review::{ReviewEvent, ReviewKind, Target};
use crate::synthesize::{backing_report_sustains, is_procedural_category};

// ============================================================
// Phase 2d: SupersedeProjection
// ============================================================
//
// Single computation over a per-claim event slice that returns
// "what's active right now" plus the audit material for what
// isn't. Used by panel_summary, by the per-kind rendered
// sections, by `contested_by`, AND by synthesize's render-status
// computation. Single source of truth so panel and status can
// never disagree on which Challenges are active (codex review).
//
// Phase 2d-i scope decisions (see EVIDENT_AGENT_PHASE2D_DRAFT.md):
// - Locality: Supersede applies only when both events live in
//   this slice. Cross-claim Supersedes go to
//   `unresolved_supersedes`.
// - One-pass semantics: a Supersede whose target is itself a
//   Supersede is `invalid`; we do not reactivate the underlying
//   verdict.
// - Cycles (A↔B, self-target) go to `invalid_supersedes`.
// - Same author submitting Endorse + Dissent without a Supersede
//   linking them: both stay active (typed-trust records every
//   speech act).

/// The four buckets a per-claim event slice is partitioned into
/// once Supersede semantics are applied. Borrowed references to
/// the underlying events so the projection is cheap to build and
/// the caller's event vec keeps ownership.
pub struct SupersedeProjection<'a> {
    /// Endorse / Dissent / Challenge events that have NOT been
    /// superseded by a well-formed local Supersede. Excludes
    /// Supersede events themselves and events that are
    /// superseded targets.
    pub active_verdicts: Vec<&'a ReviewEvent>,
    /// Supersede event paired with the event it supersedes.
    /// Audit material; renderers display these in the
    /// `## Superseded Events` section's "valid pairs" subsection.
    pub superseded_pairs: Vec<(&'a ReviewEvent, &'a ReviewEvent)>,
    /// Supersede events whose target is NOT in this slice
    /// (cross-claim, pruned, missing). Active set is unaffected;
    /// renderers display these in the "unresolved" subsection.
    pub unresolved_supersedes: Vec<&'a ReviewEvent>,
    /// Supersede events whose target IS in this slice but is
    /// itself a Supersede event (meta-supersede), or whose
    /// target id is part of a cycle, or that self-target. Phase
    /// 2d-i does NOT reactivate the underlying verdict; renderers
    /// display these in the "invalid" subsection.
    pub invalid_supersedes: Vec<&'a ReviewEvent>,
}

impl<'a> SupersedeProjection<'a> {
    /// Returns true iff the given event id is in `active_verdicts`.
    /// Used by synthesize + render-aux to decide whether a
    /// Challenge contests the report.
    pub fn is_active(&self, event_id: &EventId) -> bool {
        self.active_verdicts.iter().any(|e| &e.id == event_id)
    }
}

/// Compute the `SupersedeProjection` for a per-claim event slice.
///
/// Pure function. O(N) over the slice; uses two `HashSet`s to
/// track ids that are superseded and ids that are themselves
/// Supersede events.
pub fn supersede_projection<'a>(events: &'a [ReviewEvent]) -> SupersedeProjection<'a> {
    use std::collections::HashMap;

    // Index by event_id so we can resolve Supersede targets in O(1).
    // Slice membership is by id; cross-claim Supersedes (target id
    // not in this map) go to `unresolved_supersedes`.
    let by_id: HashMap<&EventId, &ReviewEvent> =
        events.iter().map(|e| (&e.id, e)).collect();

    // First pass: bucket each Supersede event by validity.
    // - Target not in slice → unresolved.
    // - Target IS a Supersede event → invalid (meta-supersede).
    // - Target == this Supersede's own id → invalid (self-target).
    // Cycles (A targets B, B targets A) fall out: B's target A is
    // a Supersede event itself, so B is invalid; symmetrically A.
    let mut superseded_ids: HashSet<&EventId> = HashSet::new();
    let mut superseded_pairs: Vec<(&ReviewEvent, &ReviewEvent)> = Vec::new();
    let mut unresolved_supersedes: Vec<&ReviewEvent> = Vec::new();
    let mut invalid_supersedes: Vec<&ReviewEvent> = Vec::new();

    for e in events.iter() {
        if !matches!(e.kind, ReviewKind::Supersede { .. }) {
            continue;
        }
        // Target must be a ReviewEvent to constitute a verdict
        // supersede. Phase 2d-i: a Supersede whose target is NOT
        // a review event (e.g., Target::Criterion) is left in the
        // raw event log; the synthesize layer's existing per-
        // criterion Supersede handling covers that case
        // independently. Those Supersedes are also not "verdict
        // supersedes" — they don't go to any of our four buckets
        // and so don't affect the active_verdicts set.
        let target_id = match &e.target {
            Target::ReviewEvent(eid) => eid,
            _ => continue,
        };
        // Self-target = invalid.
        if target_id == &e.id {
            invalid_supersedes.push(e);
            continue;
        }
        // Target absent from slice = unresolved.
        let Some(target_event) = by_id.get(target_id).copied() else {
            unresolved_supersedes.push(e);
            continue;
        };
        // Target is itself a Supersede event = invalid (meta-supersede).
        if matches!(target_event.kind, ReviewKind::Supersede { .. }) {
            invalid_supersedes.push(e);
            continue;
        }
        // Valid pair.
        superseded_ids.insert(target_id);
        superseded_pairs.push((target_event, e));
    }

    // Active verdicts: Endorse / Dissent / Challenge events whose
    // id is NOT in the superseded set. Supersede events themselves
    // are not "verdicts" — they're meta — so they're excluded too.
    let active_verdicts: Vec<&ReviewEvent> = events
        .iter()
        .filter(|e| !matches!(e.kind, ReviewKind::Supersede { .. }))
        .filter(|e| !superseded_ids.contains(&e.id))
        .collect();

    SupersedeProjection {
        active_verdicts,
        superseded_pairs,
        unresolved_supersedes,
        invalid_supersedes,
    }
}

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
    /// Claim ids whose challenge-backing graph reaches a cycle. Same
    /// set used by [`synthesize`](crate::synthesize), kept in sync so
    /// the per-criterion render status agrees with the synthesized
    /// report status.
    pub cycle_contested: &'a HashSet<ClaimId>,
    /// PR5c: the typed Claim's metadata declaration when this is a
    /// `MetadataCompatibility` claim. The TrustReport carries only
    /// `claim: ClaimId`, so renderers can't reach the declaration
    /// through the report — RenderInput is the only delivery path.
    /// Inlined into the augmented JSON as a top-level
    /// `metadata_declaration` block; consumed by `human_render` and
    /// `html_render`.
    pub metadata: Option<&'a MetadataDeclaration>,
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
        // PR5c: surface the typed metadata declaration when the
        // claim is a MetadataCompatibility kind. Inlined at the top
        // level (not inside `_graph` or `criteria`) so the human and
        // HTML renderers can pick it up without traversing criteria
        // — metadata claims have no criteria to traverse.
        if let Some(md) = input.metadata {
            obj.insert(
                "metadata_declaration".into(),
                serde_json::to_value(md).expect("serialize MetadataDeclaration"),
            );
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
    let crit_status = compute_criterion_status(
        &crit_id,
        input.related_events,
        input.backing_reports,
        input.cycle_contested,
    );
    // Phase 2d-i: contested_by must reflect ACTIVE Challenges only.
    // A Challenge that was superseded by a ReviewEvent-targeted
    // Supersede shouldn't appear here — otherwise the criterion's
    // result.contested_by would disagree with criterion_status
    // and with the synthesized report status.
    let projection = supersede_projection(input.related_events);
    let contested_by: Vec<String> = projection
        .active_verdicts
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
    cycle_contested: &HashSet<ClaimId>,
) -> RenderStatus {
    if events.iter().any(|e| {
        matches!(&e.kind, ReviewKind::Supersede { .. })
            && event_targets_criterion(&e.target, criterion_id)
    }) {
        return RenderStatus::Superseded;
    }
    // Phase 2d-i: filter to active verdicts before checking which
    // Challenges contest this criterion. Without this, a Challenge
    // that was superseded by a ReviewEvent-targeted Supersede would
    // still flip the criterion to Contested even though the report
    // status reads Current. Single projection, used everywhere.
    let projection = supersede_projection(events);

    // Same §8 sustain + cycle-propagation rule as
    // synthesize::compute_render_status. Without the cycle check
    // here, a criterion-targeted challenge backed by a cycled claim
    // would leave the criterion `current` while the report itself
    // renders `contested` — render would contradict synthesize.
    if projection.active_verdicts.iter().any(|e| match &e.kind {
        ReviewKind::Challenge {
            category,
            backed_by,
        } => {
            let proc_can_move = is_procedural_category(category);
            let backed_can_move = backed_by.as_ref().is_some_and(|bid| {
                backing_report_sustains(bid, backing_reports)
                    || cycle_contested.contains(bid)
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
        // Target::CriterionResult is snapshot-bound (tied to a specific
        // ReportId) and TrustReport doesn't yet carry its own id, so
        // matching by criterion alone would cross-contaminate reports
        // when callers batch events through a shared slice. The
        // synthesize side returns false for the same reason; this
        // mirrors that to keep render and synthesize consistent — a
        // contested criterion should always coincide with a contested
        // report.
        Target::CriterionResult { .. } => false,
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

        // Phase 2c: panel_summary is a per-claim projection over
        // related_events that surfaces the reviewer panel as a single
        // queryable block. Author-symmetric (humans and models in
        // one list, broken down by identity kind for accounting),
        // distinguishes events from distinct reviewers, deterministic
        // sort. Operates on raw related_events; supersede semantics
        // are deferred to Phase 2d and called out via a footnote
        // marker when n_supersede > 0.
        graph.insert(
            "panel_summary".into(),
            build_panel_summary(input.related_events),
        );
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

/// Build the Phase 2c panel_summary aux block from a slice of
/// ReviewEvents. The projection is pure and side-effect-free so the
/// markdown / HTML renderers can read its fields without traversing
/// the event slice themselves.
fn build_panel_summary(events: &[ReviewEvent]) -> Value {
    // Phase 2d-i: build the supersede projection once and let
    // panel_summary reflect ACTIVE verdicts only. The counters,
    // by_kind tally, and verdicts_by_reviewer all read the active
    // set so the rendered panel agrees with the synthesized status.
    let projection = supersede_projection(events);
    let active_verdicts = &projection.active_verdicts;

    // Distinct authors keyed by the full canonical identity (codex
    // F-CR2C-2). Tally on the ACTIVE set so two reviewers whose
    // only verdicts were superseded don't inflate n_reviewers.
    let mut seen_reviewers: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut by_kind: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for k in ["human", "model", "automated", "organization", "anonymous"] {
        by_kind.insert(k, 0);
    }
    let mut n_endorse = 0usize;
    let mut n_dissent = 0usize;
    let mut n_challenge = 0usize;

    for e in active_verdicts.iter() {
        let kind_str = identity_kind_label(&e.by.kind);
        let key = canonical_identity_key(&e.by);
        let is_new_reviewer = seen_reviewers.insert(key);
        if is_new_reviewer {
            *by_kind.entry(kind_str).or_default() += 1;
        }
        match &e.kind {
            ReviewKind::Endorse => n_endorse += 1,
            ReviewKind::Dissent => n_dissent += 1,
            ReviewKind::Challenge { .. } => n_challenge += 1,
            // Supersede events are never in active_verdicts;
            // this match arm is unreachable but kept exhaustive
            // for compiler safety against future ReviewKind
            // additions.
            ReviewKind::Supersede { .. } => {}
            // Phase 5 PR3: PromoteFromExtracted is a lifecycle
            // transition, not a verdict; render keeps it visually
            // separate from Endorse/Dissent/Challenge counts.
            ReviewKind::PromoteFromExtracted { .. } => {}
        }
    }
    let n_reviewers = seen_reviewers.len();
    let n_events_active = active_verdicts.len();
    let n_events_raw = events.len();
    // n_supersede_raw counts EVERY Supersede event in the slice
    // — telemetry about how much re-judgment has happened, not a
    // statement about active state.
    let n_supersede_raw = events
        .iter()
        .filter(|e| matches!(e.kind, ReviewKind::Supersede { .. }))
        .count();
    let n_unresolved_supersede = projection.unresolved_supersedes.len();
    let n_invalid_supersede = projection.invalid_supersedes.len();

    // verdicts_by_reviewer rows: ACTIVE verdicts only. Stable sort
    // by (kind, name, version, timestamp, event_id) — codex F-2C-13.
    let mut rows: Vec<(String, String, String, String, String, Value)> = active_verdicts
        .iter()
        .map(|e| {
            let kind_str = identity_kind_label(&e.by.kind).to_string();
            let name = e.by.name.clone();
            let version = identity_version(&e.by).unwrap_or_default();
            let timestamp = e.at.to_string();
            let event_id = e.id.as_str().to_string();
            let row = build_panel_row(e, &kind_str);
            (kind_str, name, version, timestamp, event_id, row)
        })
        .collect();
    rows.sort_by(|a, b| {
        (&a.0, &a.1, &a.2, &a.3, &a.4).cmp(&(&b.0, &b.1, &b.2, &b.3, &b.4))
    });
    let verdicts_by_reviewer: Vec<Value> = rows.into_iter().map(|(_, _, _, _, _, v)| v).collect();

    // Audit material: the three Supersede subsections rendered
    // under `## Superseded Events`. Each sorted by (timestamp,
    // event_id) of the relevant Supersede event for determinism
    // (codex review-2 audit ordering).
    let superseded_pairs_json = build_superseded_pairs_aux(&projection.superseded_pairs);
    let unresolved_json = build_supersede_list_aux(&projection.unresolved_supersedes);
    let invalid_json = build_supersede_list_aux(&projection.invalid_supersedes);

    let mut summary = Map::new();
    // New explicit counter names (codex F-2D-7).
    summary.insert("n_events_raw".into(), Value::Number(n_events_raw.into()));
    summary.insert("n_events_active".into(), Value::Number(n_events_active.into()));
    summary.insert("n_supersede_raw".into(), Value::Number(n_supersede_raw.into()));
    summary.insert(
        "n_unresolved_supersede".into(),
        Value::Number(n_unresolved_supersede.into()),
    );
    summary.insert(
        "n_invalid_supersede".into(),
        Value::Number(n_invalid_supersede.into()),
    );
    // Compat aliases for one release (codex review-2 + plan open
    // decision 1): old consumers that read n_events / n_supersede
    // continue to work. Both names point at the same numbers so
    // there's no semantic drift.
    summary.insert("n_events".into(), Value::Number(n_events_raw.into()));
    summary.insert("n_supersede".into(), Value::Number(n_supersede_raw.into()));

    summary.insert("n_reviewers".into(), Value::Number(n_reviewers.into()));
    summary.insert("n_endorse".into(), Value::Number(n_endorse.into()));
    summary.insert("n_dissent".into(), Value::Number(n_dissent.into()));
    summary.insert("n_challenge".into(), Value::Number(n_challenge.into()));
    let mut by_kind_obj = Map::new();
    for (k, v) in by_kind {
        by_kind_obj.insert(k.into(), Value::Number(v.into()));
    }
    summary.insert("by_kind".into(), Value::Object(by_kind_obj));
    summary.insert(
        "verdicts_by_reviewer".into(),
        Value::Array(verdicts_by_reviewer),
    );
    summary.insert("superseded_pairs".into(), superseded_pairs_json);
    summary.insert("unresolved_supersedes".into(), unresolved_json);
    summary.insert("invalid_supersedes".into(), invalid_json);
    Value::Object(summary)
}

/// Aux JSON for the "valid superseded pairs" subsection of the
/// rendered `## Superseded Events`. Each pair carries the original
/// event and the Supersede event with the full audit context the
/// renderer needs (event ids, author identities, timestamps,
/// successor id, supersede rationale).
fn build_superseded_pairs_aux(
    pairs: &[(&ReviewEvent, &ReviewEvent)],
) -> Value {
    let mut rows: Vec<(String, String, Value)> = pairs
        .iter()
        .map(|(original, supersede)| {
            let mut obj = Map::new();
            obj.insert(
                "original".into(),
                serde_json::to_value(*original).expect("serialize original"),
            );
            obj.insert(
                "supersede".into(),
                serde_json::to_value(*supersede).expect("serialize supersede"),
            );
            (supersede.at.to_string(), supersede.id.as_str().to_string(), Value::Object(obj))
        })
        .collect();
    rows.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    Value::Array(rows.into_iter().map(|(_, _, v)| v).collect())
}

/// Aux JSON for either of the two single-list Supersede subsections
/// (unresolved or invalid). Just the Supersede event itself; the
/// renderer adds the "(target not in slice)" / "(invalid chain)"
/// framing.
fn build_supersede_list_aux(events: &[&ReviewEvent]) -> Value {
    let mut rows: Vec<(String, String, Value)> = events
        .iter()
        .map(|e| {
            let v = serde_json::to_value(*e).expect("serialize supersede");
            (e.at.to_string(), e.id.as_str().to_string(), v)
        })
        .collect();
    rows.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    Value::Array(rows.into_iter().map(|(_, _, v)| v).collect())
}

fn identity_kind_label(kind: &crate::identity::IdentityKind) -> &'static str {
    use crate::identity::IdentityKind;
    match kind {
        IdentityKind::Human => "human",
        IdentityKind::Model => "model",
        IdentityKind::Automated => "automated",
        IdentityKind::Organization => "organization",
        IdentityKind::Anonymous => "anonymous",
    }
}

fn identity_version(identity: &crate::identity::Identity) -> Option<String> {
    identity
        .details
        .iter()
        .find(|d| d.key == "version")
        .map(|d| d.value.clone())
}

/// Canonical identity key for the panel reviewer-dedup set.
///
/// Codex F-CR2C-2: two authors who share kind + name but differ in
/// any structured detail (version, orcid, affiliation, context, …)
/// are distinct reviewers. Collapsing them undercounts
/// `n_reviewers` and can hide the Reviewer Panel section entirely.
///
/// The projection is canonical-JSON over kind + name + a sorted
/// list of (key, value) details. Same projection shape used by
/// `canonical_event_id`'s author block.
fn canonical_identity_key(identity: &crate::identity::Identity) -> String {
    let kind = identity_kind_label(&identity.kind);
    let mut details: Vec<(&str, &str)> = identity
        .details
        .iter()
        .map(|d| (d.key.as_str(), d.value.as_str()))
        .collect();
    details.sort();
    // Build a deterministic string. Separators chosen so that no
    // collision can arise between e.g. {name: "a", details: "b"}
    // and {name: "ab"}.
    let detail_str: String = details
        .iter()
        .map(|(k, v)| format!("|{k}\u{1f}{v}"))
        .collect();
    format!("{kind}\u{1f}{}{detail_str}", identity.name)
}

fn build_panel_row(event: &ReviewEvent, kind_str: &str) -> Value {
    let mut author = Map::new();
    author.insert("kind".into(), Value::String(kind_str.to_string()));
    author.insert("name".into(), Value::String(event.by.name.clone()));
    if let Some(v) = identity_version(&event.by) {
        author.insert("version".into(), Value::String(v));
    }

    let (kind_label, has_backing, backed_by) = match &event.kind {
        ReviewKind::Endorse => ("endorse", false, None),
        ReviewKind::Dissent => ("dissent", false, None),
        ReviewKind::Challenge { backed_by, .. } => {
            let has = backed_by.is_some();
            let bb = backed_by.as_ref().map(|c| c.as_str().to_string());
            ("challenge", has, bb)
        }
        ReviewKind::Supersede { .. } => ("supersede", false, None),
        // Phase 5 PR3: lifecycle transition, rendered as its own row.
        ReviewKind::PromoteFromExtracted { .. } => ("promote_from_extracted", false, None),
    };

    let mut row = Map::new();
    row.insert("author".into(), Value::Object(author));
    row.insert("kind".into(), Value::String(kind_label.into()));
    row.insert("event_id".into(), Value::String(event.id.as_str().into()));
    row.insert("timestamp".into(), Value::String(event.at.to_string()));
    row.insert("has_backing".into(), Value::Bool(has_backing));
    row.insert(
        "backed_by".into(),
        backed_by.map(Value::String).unwrap_or(Value::Null),
    );
    Value::Object(row)
}

fn status_label(s: &RenderStatus) -> &'static str {
    match s {
        RenderStatus::Current => "current",
        RenderStatus::Superseded => "superseded",
        RenderStatus::Contested => "contested",
    }
}
