use crate::agent::AgentSession;
use crate::provider::Client;
use crate::session::{Cache, KeyStore, Session};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

use super::session_manager::SessionManager;
use super::Deps;

impl Deps {
    pub fn new_session(&self, sess: Session) -> AgentSession {
        self.new_session_with_client(sess, &self.client)
    }

    pub fn new_session_with_client(&self, sess: Session, client: &Client) -> AgentSession {
        let mut asession = AgentSession::new(
            Arc::new(TokioMutex::new(self.store.clone())),
            sess,
            Arc::new(client.clone()),
            self.registry.clone(),
            self.system_prompt.clone(),
            self.max_rounds,
            self.cwd.clone(),
        );
        asession.set_subagents(self.subagents.clone());
        if let Some(ref cc) = self.compaction_client {
            asession.set_compaction_client(Arc::new(cc.clone()));
        }
        asession
    }

    pub fn new_session_manager(
        &self,
        key_store_path: &str,
    ) -> Result<SessionManager, Box<dyn std::error::Error + Send + Sync>> {
        let ks = KeyStore::new(key_store_path)?;
        let cache = Cache::new(50);
        let store = self.store.clone();
        let client = self.client.clone();
        let registry = self.registry.clone();
        let system_prompt = self.system_prompt.clone();
        let max_rounds = self.max_rounds;
        let cwd = self.cwd.clone();
        let compaction_client = self.compaction_client.clone();
        let subagents = self.subagents.clone();
        let factory = Box::new(move |sess: Session| {
            let mut asession = AgentSession::new(
                Arc::new(TokioMutex::new(store.clone())),
                sess,
                Arc::new(client.clone()),
                registry.clone(),
                system_prompt.clone(),
                max_rounds,
                cwd.clone(),
            );
            asession.set_subagents(subagents.clone());
            if let Some(ref cc) = compaction_client {
                asession.set_compaction_client(Arc::new(cc.clone()));
            }
            asession
        });
        Ok(SessionManager::new(self.store.clone(), cache, ks, factory))
    }
}
