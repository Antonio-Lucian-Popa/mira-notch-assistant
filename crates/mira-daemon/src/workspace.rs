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
    receiver: Mutex<Option<mira_core::bridge::Receiver>>,
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
    async fn phone_start(&self) -> zbus::fdo::Result<String> {
        let mut r = self.receiver.lock().await;
        if r.is_none() {
            let (config, dir) =
                mira_core::bridge::paths().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
            *r = Some(
                mira_core::bridge::Receiver::start(config, dir, "0.0.0.0:45271")
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?,
            );
        }
        Ok(r.as_ref().unwrap().snapshot().to_string())
    }
    async fn phone_stop(&self) {
        self.receiver.lock().await.take();
    }
    async fn phone_revoke(&self) -> zbus::fdo::Result<()> {
        self.receiver.lock().await.take();
        let (config, _) =
            mira_core::bridge::paths().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        if config.exists() {
            mira_core::bridge::revoke_pairing(&config)
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        }
        Ok(())
    }
    async fn phone_state(&self) -> String {
        self.receiver
            .lock()
            .await
            .as_ref()
            .map(|r| r.snapshot().to_string())
            .unwrap_or("{\"running\":false}".into())
    }
    async fn phone_actions(&self) -> String {
        self.receiver
            .lock()
            .await
            .as_ref()
            .map(|r| serde_json::to_string(&r.drain()).unwrap_or_default())
            .unwrap_or("[]".into())
    }
    async fn phone_add_action(&self, name: String, keys: String) -> zbus::fdo::Result<()> {
        let r = self.receiver.lock().await;
        r.as_ref()
            .ok_or_else(|| zbus::fdo::Error::Failed("Pornește primirea mai întâi.".into()))?
            .add_action(&name, &keys)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn phone_set_shake(&self, first: String, second: String) -> zbus::fdo::Result<()> {
        let r = self.receiver.lock().await;
        r.as_ref()
            .ok_or_else(|| zbus::fdo::Error::Failed("Pornește primirea mai întâi.".into()))?
            .set_shake(&first, &second)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn phone_remove_action(&self, id: String) -> zbus::fdo::Result<()> {
        let r = self.receiver.lock().await;
        if let Some(r) = r.as_ref() {
            r.remove_action(&id)
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        }
        Ok(())
    }
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
