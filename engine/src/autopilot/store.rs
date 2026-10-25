//! Audits and the work that follows them, kept in the crawl file.
//!
//! The audit itself is stored whole as JSON and never edited: it is the
//! record of what was tested. Everything people do afterwards — assigning,
//! scheduling, overriding a severity, flagging a finding as wrong, recording
//! a retest — lives beside it in `ux_finding_state`, and every change is
//! logged in `ux_activity`.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{compare, now, Audit, Status, WorkStatus};
use crate::error::Result;

pub fn ensure(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ux_audits (
             id INTEGER PRIMARY KEY,
             url TEXT NOT NULL,
             kind TEXT NOT NULL,
             parent_id INTEGER,
             tested_at INTEGER NOT NULL,
             page_type TEXT NOT NULL,
             json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS ux_finding_state (
             audit_id INTEGER NOT NULL,
             check_id TEXT NOT NULL,
             work_status TEXT NOT NULL DEFAULT 'open',
             owner TEXT NOT NULL DEFAULT '',
             due TEXT NOT NULL DEFAULT '',
             note TEXT NOT NULL DEFAULT '',
             flagged INTEGER NOT NULL DEFAULT 0,
             flag_reason TEXT NOT NULL DEFAULT '',
             severity_override TEXT,
             override_reason TEXT NOT NULL DEFAULT '',
             retest TEXT,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (audit_id, check_id)
         );
         CREATE TABLE IF NOT EXISTS ux_activity (
             id INTEGER PRIMARY KEY,
             audit_id INTEGER NOT NULL,
             check_id TEXT NOT NULL,
             at INTEGER NOT NULL,
             what TEXT NOT NULL
         );",
    )?;
    Ok(())
}

/// What people have done with one finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingState {
    pub check_id: String,
    pub work_status: WorkStatus,
    pub owner: String,
    pub due: String,
    pub note: String,
    pub flagged: bool,
    pub flag_reason: String,
    pub severity_override: Option<String>,
    pub override_reason: String,
    /// The latest retest result for this check: pass, fail, needs_review …
    pub retest: Option<String>,
    pub updated_at: i64,
}

impl FindingState {
    pub fn new(check_id: impl Into<String>) -> Self {
        Self {
            check_id: check_id.into(),
            work_status: WorkStatus::Open,
            owner: String::new(),
            due: String::new(),
            note: String::new(),
            flagged: false,
            flag_reason: String::new(),
            severity_override: None,
            override_reason: String::new(),
            retest: None,
            updated_at: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditRow {
    pub id: i64,
    pub url: String,
    pub kind: String,
    pub parent_id: Option<i64>,
    pub tested_at: i64,
    pub page_type: String,
    pub fail: u32,
    pub critical: u32,
    pub high: u32,
    pub coverage: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub check_id: String,
    pub at: i64,
    pub what: String,
}

pub fn save(conn: &Connection, audit: &mut Audit) -> Result<i64> {
    ensure(conn)?;
    let json = serde_json::to_string(audit).map_err(|e| crate::error::EngineError::Other(e.into()))?;
    conn.execute(
        "INSERT INTO ux_audits (url, kind, parent_id, tested_at, page_type, json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![audit.setup.url, audit.kind, audit.parent_id, audit.tested_at, audit.page_type.as_str(), json],
    )?;
    audit.id = conn.last_insert_rowid();
    Ok(audit.id)
}

pub fn load(conn: &Connection, id: i64) -> Result<Option<Audit>> {
    ensure(conn)?;
    let json: Option<String> =
        conn.query_row("SELECT json FROM ux_audits WHERE id = ?1", [id], |row| row.get(0)).optional()?;
    let Some(json) = json else { return Ok(None) };
    let mut audit: Audit = serde_json::from_str(&json).map_err(|e| crate::error::EngineError::Other(e.into()))?;
    audit.id = id;
    Ok(Some(audit))
}

pub fn list(conn: &Connection) -> Result<Vec<AuditRow>> {
    ensure(conn)?;
    let mut stmt = conn.prepare("SELECT id FROM ux_audits ORDER BY tested_at DESC, id DESC")?;
    let ids: Vec<i64> = stmt.query_map([], |row| row.get(0))?.collect::<std::result::Result<_, _>>()?;
    let mut rows = Vec::new();
    for id in ids {
        if let Some(audit) = load(conn, id)? {
            let summary = audit.summary();
            rows.push(AuditRow {
                id,
                url: audit.setup.url.clone(),
                kind: audit.kind.clone(),
                parent_id: audit.parent_id,
                tested_at: audit.tested_at,
                page_type: audit.page_type.label().to_string(),
                fail: summary.fail,
                critical: summary.critical,
                high: summary.high,
                coverage: summary.coverage,
            });
        }
    }
    Ok(rows)
}

pub fn states(conn: &Connection, audit_id: i64) -> Result<Vec<FindingState>> {
    ensure(conn)?;
    let mut stmt = conn.prepare(
        "SELECT check_id, work_status, owner, due, note, flagged, flag_reason, severity_override,
                override_reason, retest, updated_at
         FROM ux_finding_state WHERE audit_id = ?1",
    )?;
    let rows = stmt.query_map([audit_id], |row| {
        Ok(FindingState {
            check_id: row.get(0)?,
            work_status: WorkStatus::parse(&row.get::<_, String>(1)?).unwrap_or(WorkStatus::Open),
            owner: row.get(2)?,
            due: row.get(3)?,
            note: row.get(4)?,
            flagged: row.get::<_, i64>(5)? != 0,
            flag_reason: row.get(6)?,
            severity_override: row.get(7)?,
            override_reason: row.get(8)?,
            retest: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn state(conn: &Connection, audit_id: i64, check_id: &str) -> Result<Option<FindingState>> {
    Ok(states(conn, audit_id)?.into_iter().find(|s| s.check_id == check_id))
}

fn describe(old: &FindingState, new: &FindingState) -> Vec<String> {
    let mut changes = Vec::new();
    if old.work_status != new.work_status {
        changes.push(format!("Status → {}", new.work_status.label()));
    }
    if old.owner != new.owner {
        changes.push(if new.owner.is_empty() { "Unassigned".into() } else { format!("Assigned to {}", new.owner) });
    }
    if old.due != new.due {
        changes.push(if new.due.is_empty() { "Due date cleared".into() } else { format!("Due {}", new.due) });
    }
    if old.note != new.note {
        changes.push("Note edited".into());
    }
    if old.flagged != new.flagged {
        changes.push(if new.flagged { format!("Flagged as incorrect: {}", new.flag_reason) } else { "Flag removed".into() });
    }
    if old.severity_override != new.severity_override {
        changes.push(match &new.severity_override {
            Some(s) => format!("Severity overridden to {s}: {}", new.override_reason),
            None => "Severity override removed".into(),
        });
    }
    if old.retest != new.retest {
        if let Some(r) = &new.retest {
            changes.push(format!("Retest: {}", r.replace('_', " ")));
        }
    }
    changes
}

/// Save what someone did with a finding, and log it.
pub fn set_state(conn: &Connection, audit_id: i64, new: &FindingState) -> Result<()> {
    ensure(conn)?;
    let old = state(conn, audit_id, &new.check_id)?.unwrap_or_else(|| FindingState::new(&new.check_id));
    let at = now();
    conn.execute(
        "INSERT INTO ux_finding_state (audit_id, check_id, work_status, owner, due, note, flagged, flag_reason,
                                       severity_override, override_reason, retest, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(audit_id, check_id) DO UPDATE SET
             work_status = excluded.work_status, owner = excluded.owner, due = excluded.due,
             note = excluded.note, flagged = excluded.flagged, flag_reason = excluded.flag_reason,
             severity_override = excluded.severity_override, override_reason = excluded.override_reason,
             retest = excluded.retest, updated_at = excluded.updated_at",
        params![
            audit_id,
            new.check_id,
            new.work_status.as_str(),
            new.owner,
            new.due,
            new.note,
            new.flagged as i64,
            new.flag_reason,
            new.severity_override,
            new.override_reason,
            new.retest,
            at
        ],
    )?;
    for what in describe(&old, new) {
        conn.execute(
            "INSERT INTO ux_activity (audit_id, check_id, at, what) VALUES (?1, ?2, ?3, ?4)",
            params![audit_id, new.check_id, at, what],
        )?;
    }
    Ok(())
}

pub fn activity(conn: &Connection, audit_id: i64) -> Result<Vec<Activity>> {
    ensure(conn)?;
    let mut stmt =
        conn.prepare("SELECT check_id, at, what FROM ux_activity WHERE audit_id = ?1 ORDER BY at DESC, id DESC LIMIT 200")?;
    let rows = stmt.query_map([audit_id], |row| Ok(Activity { check_id: row.get(0)?, at: row.get(1)?, what: row.get(2)? }))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn delete(conn: &Connection, audit_id: i64) -> Result<()> {
    ensure(conn)?;
    conn.execute("DELETE FROM ux_activity WHERE audit_id = ?1", [audit_id])?;
    conn.execute("DELETE FROM ux_finding_state WHERE audit_id = ?1", [audit_id])?;
    conn.execute("DELETE FROM ux_audits WHERE id = ?1", [audit_id])?;
    Ok(())
}

/// Write a retest's results back onto the findings of the audit it retested:
/// each finding gets the new status, and one that now passes is resolved.
pub fn record_retest(conn: &Connection, before: &Audit, after: &Audit) -> Result<()> {
    let existing = states(conn, before.id)?;
    let comparison = compare(before, after);
    for (check, _) in before.findings() {
        let Some(row) = comparison.rows.iter().find(|r| r.id == check.id) else { continue };
        let result = match row.change.as_str() {
            "fixed" => Status::Pass.as_str(),
            _ => row.after.map_or("not_tested", |s| s.as_str()),
        };
        let mut state =
            existing.iter().find(|s| s.check_id == check.id).cloned().unwrap_or_else(|| FindingState::new(&check.id));
        state.retest = Some(result.to_string());
        if row.change == "fixed" {
            state.work_status = WorkStatus::Resolved;
        } else if state.work_status == WorkStatus::Resolved && row.change == "still_failing" {
            state.work_status = WorkStatus::Open;
        }
        set_state(conn, before.id, &state)?;
    }
    Ok(())
}
