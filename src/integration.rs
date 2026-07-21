use crate::core::session_manager::SessionManager;
use crate::markdown::split_text;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

const TRUNCATION_NOTE: &str = "\n\n> _...message continues..._";
const PROMPT_CANCEL_TIMEOUT: Duration = Duration::from_secs(5);

pub struct ActivePrompt {
    pub cancel: oneshot::Sender<()>,
    pub done: oneshot::Receiver<()>,
}

#[async_trait]
pub trait StreamingBackend: Send + Sync {
    type MsgId: Clone + Send + Sync;

    fn flush_interval(&self) -> Duration;
    fn text_limit(&self) -> usize;
    fn markdown(&self, text: &str) -> String;

    async fn send_thinking(&self) -> Result<Self::MsgId, String>;
    async fn edit_message(&self, msg_id: &Self::MsgId, text: &str) -> Result<(), String>;
    async fn post_message(&self, text: &str) -> Result<Self::MsgId, String>;

    fn on_event(&self, _event: &crate::agent::Event) {}
    async fn after_finalize(&self) {}
}

fn flush_display<B: StreamingBackend + ?Sized>(backend: &B, text: &str) -> String {
    let mrkdwn = backend.markdown(text);
    let chunks = split_text(&mrkdwn, backend.text_limit());
    if chunks.is_empty() {
        return String::new();
    }
    let display = &chunks[0];
    let needs_truncation = chunks.len() > 1 || display.len() < mrkdwn.len();
    let mut display = display.to_string();
    if needs_truncation {
        display.push_str(TRUNCATION_NOTE);
    }
    if display.len() > backend.text_limit() {
        display.truncate(backend.text_limit());
    }
    display
}

async fn do_flush<B: StreamingBackend + ?Sized>(backend: &B, msg_id: &B::MsgId, text: &str) {
    let display = flush_display(backend, text);
    if display.is_empty() {
        return;
    }
    if let Err(e) = backend.edit_message(msg_id, &display).await {
        log::error!("streaming flush: {}", e);
    }
}

async fn do_finalize<B: StreamingBackend + ?Sized>(backend: &B, msg_id: &B::MsgId, text: &str) {
    let mrkdwn = backend.markdown(text);
    let limit = backend.text_limit();
    let chunks = split_text(&mrkdwn, limit);
    for (i, chunk) in chunks.iter().enumerate() {
        let mut chunk = chunk.clone();
        if chunk.len() > limit {
            chunk.truncate(limit);
        }
        let result = if i == 0 {
            backend.edit_message(msg_id, &chunk).await
        } else {
            backend.post_message(&chunk).await.map(|_| ())
        };
        if let Err(e) = result {
            log::error!("streaming finalize: {}", e);
        }
    }
}

pub async fn process_prompt<B: StreamingBackend + ?Sized, K>(
    backend: &B,
    manager: &Arc<Mutex<SessionManager>>,
    active_prompts: &Mutex<HashMap<K, ActivePrompt>>,
    prompt_key: K,
    session_key: &str,
    text: &str,
) where
    K: Eq + std::hash::Hash + Clone + Send,
{
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let (done_tx, done_rx) = oneshot::channel::<()>();
    tokio::pin!(cancel_rx);

    let prior = active_prompts.lock().unwrap().remove(&prompt_key);
    if let Some(prior) = prior {
        let _ = prior.cancel.send(());
        let _ = tokio::time::timeout(PROMPT_CANCEL_TIMEOUT, prior.done).await;
    }

    active_prompts.lock().unwrap().insert(
        prompt_key.clone(),
        ActivePrompt {
            cancel: cancel_tx,
            done: done_rx,
        },
    );

    let msg_id = match backend.send_thinking().await {
        Ok(id) => id,
        Err(e) => {
            log::error!("streaming post thinking: {}", e);
            active_prompts.lock().unwrap().remove(&prompt_key);
            let _ = done_tx.send(());
            return;
        }
    };

    let sess_arc = {
        let result = {
            let mut mgr = manager.lock().unwrap();
            mgr.get_or_create(session_key)
        };
        match result {
            Ok(s) => s,
            Err(e) => {
                log::error!("streaming session: {}", e);
                do_finalize(backend, &msg_id, "Error interno creando sesi\u{f3}n.").await;
                active_prompts.lock().unwrap().remove(&prompt_key);
                let _ = done_tx.send(());
                return;
            }
        }
    };

    let mut events = {
        let mut sess = sess_arc.lock().unwrap();
        sess.prompt(text)
    };

    let mut full_text = String::new();
    let mut last_flush = Instant::now();
    let flush_interval = backend.flush_interval();

    loop {
        tokio::select! {
            ev = events.recv() => {
                match ev {
                    Some(ev) => {
                        backend.on_event(&ev);
                        match ev.kind {
                            crate::agent::EventKind::TextDelta(delta) => full_text.push_str(&delta),
                            crate::agent::EventKind::Error(msg) => {
                                full_text.push_str("\n\nError: ");
                                full_text.push_str(&msg);
                            }
                            _ => {}
                        }
                    }
                    None => break,
                }
                if !full_text.is_empty() && last_flush.elapsed() >= flush_interval {
                    do_flush(backend, &msg_id, &full_text).await;
                    last_flush = Instant::now();
                }
            }
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(last_flush) + flush_interval), if !full_text.is_empty() => {
                do_flush(backend, &msg_id, &full_text).await;
                last_flush = Instant::now();
            }
            _ = &mut cancel_rx => {
                break;
            }
        }
    }

    if !full_text.is_empty() {
        do_finalize(backend, &msg_id, &full_text).await;
    }

    backend.after_finalize().await;

    {
        let fresh = {
            let mut mgr = manager.lock().unwrap();
            mgr.reload(session_key)
        };
        if let Ok(Some(fresh)) = fresh {
            let mut sess = sess_arc.lock().unwrap();
            sess.reload_from(fresh);
        }
    }

    active_prompts.lock().unwrap().remove(&prompt_key);
    let _ = done_tx.send(());
}
