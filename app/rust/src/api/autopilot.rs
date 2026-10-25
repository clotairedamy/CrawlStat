//! UX Testing: the autopilot audit, its findings workflow, retests and
//! exports.
//!
//! The audit model is large and nested, so it crosses the bridge as JSON
//! rather than as dozens of mirror structs; the Dart side reads it with
//! light wrappers. The reasoning lives in `crawlstat_engine::autopilot`.

use crawlstat_engine::autopilot::{
    self, store, Audit, Category, Depth, Device, EvidenceStatus, Goal, PageType, Setup, Severity, Status, WorkStatus,
};
use crawlstat_engine::error::EngineError;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::runtime;

fn err(e: impl ToString) -> String {
    e.to_string()
}

fn options<T: Copy>(all: &[T], id: impl Fn(T) -> &'static str, label: impl Fn(T) -> &'static str) -> Value {
    Value::Array(all.iter().map(|v| json!({ "id": id(*v), "label": label(*v) })).collect())
}

/// The choices the setup form offers, and the whole test library.
#[flutter_rust_bridge::frb(sync)]
pub fn autopilot_catalog() -> String {
    let questions: serde_json::Map<String, Value> =
        PageType::ALL.iter().map(|t| (t.as_str().to_string(), json!(t.questions()))).collect();
    json!({
        "page_types": options(PageType::ALL, PageType::as_str, PageType::label),
        "goals": options(Goal::ALL, Goal::as_str, Goal::label),
        "devices": options(Device::ALL, Device::as_str, Device::label),
        "depths": options(Depth::ALL, Depth::as_str, Depth::label),
        "categories": options(Category::ALL, Category::as_str, Category::label),
        "statuses": options(Status::ALL, Status::as_str, Status::label),
        "severities": options(Severity::ALL, Severity::as_str, Severity::label),
        "evidence": options(EvidenceStatus::ALL, EvidenceStatus::as_str, EvidenceStatus::label),
        "work": options(WorkStatus::ALL, WorkStatus::as_str, WorkStatus::label),
        "questions": questions,
        "library": autopilot::library(),
    })
    .to_string()
}

/// Everything the screen shows for one audit.
fn view(conn: &Connection, audit: &Audit) -> Result<Value, EngineError> {
    let states = store::states(conn, audit.id)?;
    let activity = store::activity(conn, audit.id)?;
    let rows = store::list(conn)?;
    // A baseline is compared with its latest retest; a retest with its baseline.
    let (comparison, retest_id) = match audit.parent_id {
        Some(parent) => match store::load(conn, parent)? {
            Some(before) => (Some(autopilot::compare(&before, audit)), None),
            None => (None, None),
        },
        None => match rows.iter().filter(|r| r.parent_id == Some(audit.id)).max_by_key(|r| (r.tested_at, r.id)) {
            Some(row) => match store::load(conn, row.id)? {
                Some(after) => (Some(autopilot::compare(audit, &after)), Some(row.id)),
                None => (None, None),
            },
            None => (None, None),
        },
    };
    Ok(json!({
        "audit": audit,
        "summary": audit.summary(),
        "states": states,
        "activity": activity,
        "comparison": comparison,
        "retest_id": retest_id,
    }))
}

fn setup_from(setup_json: &str) -> Result<Setup, String> {
    let given: Value = serde_json::from_str(setup_json).map_err(err)?;
    let url = given.get("url").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    if url.is_empty() {
        return Err("Enter the URL of the page to test.".to_string());
    }
    // Start from the engine's defaults so the form can leave fields out.
    let mut base = serde_json::to_value(Setup::new(url)).map_err(err)?;
    if let (Some(base), Some(given)) = (base.as_object_mut(), given.as_object()) {
        for (key, value) in given {
            if key == "audience" {
                if let (Some(audience), Some(given)) = (base.get_mut("audience").and_then(Value::as_object_mut), value.as_object()) {
                    for (k, v) in given {
                        audience.insert(k.clone(), v.clone());
                    }
                }
            } else if !(value.is_string() && value.as_str() == Some("") && matches!(key.as_str(), "page_type" | "goal")) {
                base.insert(key.clone(), value.clone());
            }
        }
    }
    serde_json::from_value(base).map_err(err)
}

/// Run a new audit and save it. Returns the view JSON.
pub fn autopilot_run(db_path: String, setup_json: String) -> Result<String, String> {
    let setup = setup_from(&setup_json)?;
    runtime::with_store(&db_path, |s| {
        let conn = s.connection();
        store::ensure(conn)?;
        Ok(match autopilot::audit(conn, &setup) {
            Ok(mut audit) => {
                store::save(conn, &mut audit)?;
                Ok(view(conn, &audit)?.to_string())
            }
            Err(message) => Err(message),
        })
    })
    .map_err(err)?
}

/// Saved audits, newest first.
pub fn autopilot_list(db_path: String) -> Result<String, String> {
    runtime::with_store(&db_path, |s| Ok(serde_json::to_string(&store::list(s.connection())?).unwrap_or_default()))
        .map_err(err)
}

pub fn autopilot_open(db_path: String, id: i64) -> Result<String, String> {
    runtime::with_store(&db_path, |s| {
        let conn = s.connection();
        Ok(match store::load(conn, id)? {
            Some(audit) => Ok(view(conn, &audit)?.to_string()),
            None => Err("That audit no longer exists.".to_string()),
        })
    })
    .map_err(err)?
}

/// Save what a person did with one finding. Returns the refreshed view.
pub fn autopilot_set_state(db_path: String, audit_id: i64, state_json: String) -> Result<String, String> {
    let state: store::FindingState = serde_json::from_str(&state_json).map_err(err)?;
    runtime::with_store(&db_path, |s| {
        let conn = s.connection();
        store::set_state(conn, audit_id, &state)?;
        Ok(match store::load(conn, audit_id)? {
            Some(audit) => Ok(view(conn, &audit)?.to_string()),
            None => Err("That audit no longer exists.".to_string()),
        })
    })
    .map_err(err)?
}

/// Run the same setup again, compare, and write the results back onto the
/// baseline's findings. Returns the baseline's view, now with the comparison.
pub fn autopilot_retest(db_path: String, id: i64) -> Result<String, String> {
    runtime::with_store(&db_path, |s| {
        let conn = s.connection();
        let Some(chosen) = store::load(conn, id)? else {
            return Ok(Err("That audit no longer exists.".to_string()));
        };
        let baseline = match chosen.parent_id {
            Some(parent) => store::load(conn, parent)?.unwrap_or(chosen),
            None => chosen,
        };
        let mut after = match autopilot::audit(conn, &baseline.setup) {
            Ok(after) => after,
            Err(message) => return Ok(Err(message)),
        };
        after.kind = "retest".to_string();
        after.parent_id = Some(baseline.id);
        store::save(conn, &mut after)?;
        store::record_retest(conn, &baseline, &after)?;
        Ok(Ok(view(conn, &baseline)?.to_string()))
    })
    .map_err(err)?
}

pub fn autopilot_delete(db_path: String, id: i64) -> Result<(), String> {
    runtime::with_store(&db_path, |s| store::delete(s.connection(), id)).map_err(err)
}

fn export(db_path: &str, id: i64, write: impl Fn(&Audit, &[store::FindingState], &Connection) -> Result<String, EngineError>) -> Result<String, String> {
    runtime::with_store(db_path, |s| {
        let conn = s.connection();
        Ok(match store::load(conn, id)? {
            Some(audit) => {
                let states = store::states(conn, id)?;
                Ok(write(&audit, &states, conn)?)
            }
            None => Err("That audit no longer exists.".to_string()),
        })
    })
    .map_err(err)?
}

/// The client report, as Markdown.
pub fn autopilot_client_report(db_path: String, id: i64) -> Result<String, String> {
    export(&db_path, id, |audit, states, conn| {
        let comparison = view(conn, audit)?.get("comparison").cloned().and_then(|c| serde_json::from_value(c).ok());
        Ok(autopilot::report::client_markdown(audit, states, comparison.as_ref()))
    })
}

/// One row per finding, for a tracker.
pub fn autopilot_developer_csv(db_path: String, id: i64) -> Result<String, String> {
    export(&db_path, id, |audit, states, _| Ok(autopilot::report::developer_csv(audit, states)))
}
