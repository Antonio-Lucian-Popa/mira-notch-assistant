use mira_core::workspace::{self, Workspace};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct WorkspaceIface {
    state: Arc<Mutex<Workspace>>,
    generation: Arc<AtomicU64>,
}
impl WorkspaceIface {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(Workspace::new())),
            ..Self::default()
        }
    }
}
#[zbus::interface(name = "ro.novabytecode.Mira.Workspace")]
impl WorkspaceIface {
    async fn snapshot(&self) -> String {
        serde_json::to_string(&*self.state.lock().await).unwrap_or_default()
    }
    async fn add_paths(&self, paths: Vec<String>) -> zbus::fdo::Result<String> {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let mut w = state.blocking_lock();
            w.add_paths(&paths);
            serde_json::to_string(&*w).unwrap_or_default()
        })
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn note(&self, text: String) {
        self.state.lock().await.set_note(&text);
    }
    async fn capture(&self, enabled: bool) {
        self.state.lock().await.capture = enabled;
    }
    async fn push_clip(&self, text: String) {
        self.state.lock().await.push_clip(&text);
    }
    async fn clear(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.state.lock().await.clear();
    }
    async fn remove(&self, path: String) {
        self.state.lock().await.links.retain(|l| l.path != path);
        self.state.lock().await.roots.retain(|r| r != &path);
    }
    async fn cancel_search(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
    async fn search(&self, query: String) -> zbus::fdo::Result<String> {
        let roots = self.state.lock().await.roots.clone();
        let generation = self.generation.clone();
        let request = generation.fetch_add(1, Ordering::Relaxed) + 1;
        tokio::task::spawn_blocking(move || {
            serde_json::to_string(&workspace::search(&roots, &query, &generation, request))
                .unwrap_or_default()
        })
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
}
