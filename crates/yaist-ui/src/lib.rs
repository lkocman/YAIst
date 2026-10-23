//! The client model shared by the yAIst TUI and GUI: same modules, same
//! panels, same plan cards, same conversation. A feature added here shows up
//! in both.

pub mod palette;
mod session;
mod state;

pub use session::{Cmd, Session};
pub use state::{Entry, LlmStatus, ModuleView, Panel, Plan, PlanStatus, State};

use yaist_core::{ActionSpec, Output, Table};

/// What a status cell means, so both clients colour it the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Warn,
    Bad,
    Faint,
    Plain,
}

pub fn tone_of(value: &str) -> Tone {
    match value {
        "active" | "running" | "enabled" | "yes" | "on" | "installed" | "enforcing" => Tone::Good,
        "failed" | "masked" => Tone::Bad,
        "activating" | "deactivating" | "reloading" | "permissive" => Tone::Warn,
        "inactive" | "dead" | "disabled" | "no" | "off" | "static" | "-" | "available" | "exited" | "indirect" => Tone::Faint,
        _ => Tone::Plain,
    }
}

/// Columns whose values are states and get a badge.
pub fn is_status_column(key: &str) -> bool {
    matches!(key, "active" | "sub" | "enabled" | "is_admin" | "setting" | "status")
}

/// Raw arguments for a row action, taken from the row's matching columns.
pub fn row_args(spec: &ActionSpec, table: &Table, row: &[String]) -> serde_json::Map<String, serde_json::Value> {
    let mut raw = serde_json::Map::new();
    for p in spec.params {
        if let Some(v) = table.cell(row, p.name) {
            raw.insert(p.name.to_owned(), serde_json::Value::String(v.to_owned()));
        }
    }
    raw
}

/// Whether a row action can be proposed straight from the row: every
/// required parameter comes from a column. Otherwise a form opens first.
pub fn row_action_is_direct(spec: &ActionSpec, table: &Table) -> bool {
    spec.params.iter().filter(|p| p.required).all(|p| table.columns.iter().any(|c| c.key == p.name))
}

pub fn output_rows(out: &Output) -> usize {
    match out {
        Output::Table(t) => t.rows.len(),
        Output::Facts(f) => f.len(),
        _ => 1,
    }
}

/// A form generated from an action's parameters; both clients edit `values`
/// with their own widgets.
#[derive(Clone, Debug)]
pub struct Form {
    pub spec: ActionSpec,
    /// One per parameter, as text; booleans are "true"/"false".
    pub values: Vec<String>,
    /// The plan this form edits, cancelled when the form is submitted.
    pub replaces: Option<u64>,
}

impl Form {
    pub fn new(spec: ActionSpec, prefill: &serde_json::Map<String, serde_json::Value>) -> Self {
        let values = spec
            .params
            .iter()
            .map(|p| match prefill.get(p.name) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => match p.kind {
                    yaist_core::ParamKind::Choice(opts) => p.default.unwrap_or(opts[0]).to_owned(),
                    _ => p.default.unwrap_or("").to_owned(),
                },
            })
            .map(|v| match v.as_str() {
                "on" | "yes" => "true".to_owned(),
                "off" | "no" => "false".to_owned(),
                _ => v,
            })
            .collect();
        Self { spec, values, replaces: None }
    }

    /// Editing an existing plan: start from its arguments.
    pub fn edit(plan: &Plan) -> Self {
        Self { replaces: Some(plan.id), ..Self::new(plan.spec, &plan.args.0) }
    }

    pub fn toggle(&mut self, i: usize) {
        let v = &mut self.values[i];
        *v = if v == "true" { "false".into() } else { "true".into() };
    }

    /// Steps a choice parameter forwards or backwards.
    pub fn cycle(&mut self, i: usize, forward: bool) {
        if let yaist_core::ParamKind::Choice(opts) = self.spec.params[i].kind {
            let cur = opts.iter().position(|o| *o == self.values[i]).unwrap_or(0);
            let next = if forward { (cur + 1) % opts.len() } else { (cur + opts.len() - 1) % opts.len() };
            self.values[i] = opts[next].to_owned();
        }
    }

    pub fn raw(&self) -> serde_json::Map<String, serde_json::Value> {
        self.spec
            .params
            .iter()
            .zip(&self.values)
            .filter(|(p, v)| p.required || !v.trim().is_empty())
            .map(|(p, v)| (p.name.to_owned(), serde_json::Value::String(v.clone())))
            .collect()
    }

    /// Sends the form: cancels the plan it edits and proposes the new one.
    pub fn submit(&self, session: &Session) {
        if let Some(old) = self.replaces {
            session.send(Cmd::Cancel(old));
        }
        session.send(Cmd::Propose { action: self.spec.id, raw: self.raw() });
    }
}
