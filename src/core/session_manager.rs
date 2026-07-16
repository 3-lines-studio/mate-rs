use crate::agent::AgentSession;
use crate::session::store::Store;
use crate::session::{Cache, KeyStore, Session};
use std::sync::Arc;

pub struct SessionManager {
    store: Store,
    cache: Cache<Arc<std::sync::Mutex<AgentSession>>>,
    key_store: KeyStore,
    factory: Box<dyn Fn(Session) -> AgentSession + Send + Sync>,
}

impl SessionManager {
    pub fn new(
        store: Store,
        cache: Cache<Arc<std::sync::Mutex<AgentSession>>>,
        key_store: KeyStore,
        factory: Box<dyn Fn(Session) -> AgentSession + Send + Sync>,
    ) -> Self {
        Self {
            store,
            cache,
            key_store,
            factory,
        }
    }

    pub fn get_or_create(
        &mut self,
        key: &str,
    ) -> Result<Arc<std::sync::Mutex<AgentSession>>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(sess) = self.cache.get(key) {
            return Ok(sess.clone());
        }

        let session_id = self.key_store.get(key).cloned();
        if let Some(ref session_id) = session_id {
            if let Ok(loaded_sess) = self.store.load(session_id) {
                let sess = (self.factory)(loaded_sess);
                let sess = Arc::new(std::sync::Mutex::new(sess));
                self.cache.put(key, sess.clone());
                return Ok(sess);
            }
        }

        let stored = self.store.create()?;
        self.key_store.set(key, &stored.id)?;

        let sess = (self.factory)(stored);
        let sess = Arc::new(std::sync::Mutex::new(sess));
        self.cache.put(key, sess.clone());
        Ok(sess)
    }

    pub fn save(
        &mut self,
        key: &str,
        sess: &AgentSession,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.key_store.get(key).is_none() {
            return Ok(());
        }
        self.store.save_meta(&sess.sess())?;
        Ok(())
    }
}
