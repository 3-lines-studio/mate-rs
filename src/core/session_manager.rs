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
        if let Some(ref session_id) = session_id
            && let Ok(loaded_sess) = self.store.load(session_id)
        {
            let sess = (self.factory)(loaded_sess);
            let sess = Arc::new(std::sync::Mutex::new(sess));
            self.cache.put(key, sess.clone());
            return Ok(sess);
        }

        let stored = self.store.create()?;
        self.key_store.set(key, &stored.id)?;

        let sess = (self.factory)(stored);
        let sess = Arc::new(std::sync::Mutex::new(sess));
        self.cache.put(key, sess.clone());
        Ok(sess)
    }

    pub fn reload(
        &mut self,
        key: &str,
    ) -> Result<Option<Session>, Box<dyn std::error::Error + Send + Sync>> {
        let Some(session_id) = self.key_store.get(key).cloned() else {
            return Ok(None);
        };
        match self.store.load(&session_id) {
            Ok(sess) => Ok(Some(sess)),
            Err(_) => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager(dir: &str) -> SessionManager {
        let store = Store::new(dir).unwrap();
        let cache = Cache::new(8);
        let ks = KeyStore::new(&format!("{}/keys.json", dir)).unwrap();
        let factory: Box<dyn Fn(Session) -> AgentSession + Send + Sync> =
            Box::new(|_| panic!("factory must not be invoked by reload"));
        SessionManager::new(store, cache, ks, factory)
    }

    #[test]
    fn reload_returns_none_for_unknown_key() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut mgr = make_manager(&dir.path().to_string_lossy());
        assert!(mgr.reload("nope").unwrap().is_none());
    }

    #[test]
    fn reload_reads_advanced_current_turn_from_disk() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_s = dir.path().to_string_lossy().to_string();

        let mut store = Store::new(&dir_s).unwrap();
        let mut sess = store.create().unwrap();
        sess.current_turn = "t1".to_string();
        sess.turn_count = 1;
        store.save_meta(&sess).unwrap();

        let mut mgr = make_manager(&dir_s);
        mgr.key_store.set("thread-1", &sess.id).unwrap();

        let fresh = mgr.reload("thread-1").unwrap().unwrap();
        assert_eq!(fresh.current_turn, "t1");
        assert_eq!(fresh.turn_count, 1);
    }
}
