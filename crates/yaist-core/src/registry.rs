use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::{ActionSpec, Args, Context, Output};

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type Handler = Arc<dyn Fn(Arc<Context>, Args) -> BoxFuture<Result<Output>> + Send + Sync>;

/// A module: one entry in the sidebar, one panel.
#[derive(Clone, Copy, Debug)]
pub struct ModuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub tagline: &'static str,
    /// The read action whose output the panel shows.
    pub overview: &'static str,
    /// Actions offered on each row of the overview table; parameters are
    /// filled from the row's columns of the same name.
    pub row_actions: &'static [&'static str],
}

#[derive(Clone)]
pub struct Action {
    pub spec: ActionSpec,
    handler: Handler,
}

#[derive(Default, Clone)]
pub struct Registry {
    pub modules: Vec<ModuleSpec>,
    actions: BTreeMap<&'static str, Action>,
    order: Vec<&'static str>,
}

impl Registry {
    pub fn module(&mut self, spec: ModuleSpec) {
        self.modules.push(spec);
    }

    pub fn action<F, Fut>(&mut self, spec: ActionSpec, f: F)
    where
        F: Fn(Arc<Context>, Args) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Output>> + Send + 'static,
    {
        let handler: Handler = Arc::new(move |ctx, args| Box::pin(f(ctx, args)));
        self.order.push(spec.id);
        self.actions.insert(spec.id, Action { spec, handler });
    }

    pub fn get(&self, id: &str) -> Option<&ActionSpec> {
        self.actions.get(id).map(|a| &a.spec)
    }

    /// All actions, in registration order.
    pub fn specs(&self) -> impl Iterator<Item = &ActionSpec> {
        self.order.iter().map(|id| &self.actions[id].spec)
    }

    pub fn module_actions(&self, module: &str) -> Vec<ActionSpec> {
        self.specs().filter(|s| s.module() == module).copied().collect()
    }

    pub fn find_module(&self, id: &str) -> Option<&ModuleSpec> {
        self.modules.iter().find(|m| m.id == id)
    }

    /// Runs an action with already validated arguments.
    pub async fn run(&self, ctx: Arc<Context>, id: &str, args: Args) -> Result<Output> {
        let action = self.actions.get(id).ok_or_else(|| anyhow!("unknown action {id}"))?;
        (action.handler)(ctx, args).await
    }
}
