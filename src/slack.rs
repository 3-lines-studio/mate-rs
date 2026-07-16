use crate::core::session_manager::SessionManager;
use crate::core::{Deps, Interface, Notifier};
use crate::markdown::{slack::markdown_to_slack, split_text};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

const SLACK_TEXT_LIMIT: usize = 3500;
const FLUSH_INTERVAL: Duration = Duration::from_millis(800);
const TRUNCATION_NOTE: &str = "\n\n> _...message continues..._";
const PROMPT_CANCEL_TIMEOUT: Duration = Duration::from_secs(5);

fn api_url(method: &str) -> String {
    format!("https://www.slack.com/api/{}", method)
}

#[derive(Deserialize)]
struct SlackApiResp {
    ok: bool,
    error: Option<String>,
}

#[derive(Deserialize)]
struct ConnectionsOpenResp {
    ok: bool,
    url: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct AuthTestResp {
    ok: bool,
    user_id: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct PostMessageResp {
    ok: bool,
    ts: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    env_type: String,
    envelope_id: Option<String>,
    payload: Option<serde_json::Value>,
}

struct SlackEvent {
    text: String,
    ts: String,
    channel: String,
    thread_ts: String,
}

async fn auth_test(client: &reqwest::Client, bot_token: &str) -> Result<String, String> {
    let resp: AuthTestResp = client
        .post(api_url("auth.test"))
        .header("Authorization", format!("Bearer {}", bot_token))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "auth.test failed".into()));
    }
    Ok(resp.user_id.unwrap_or_default())
}

async fn connections_open(client: &reqwest::Client, app_token: &str) -> Result<String, String> {
    let resp: ConnectionsOpenResp = client
        .post(api_url("apps.connections.open"))
        .header("Authorization", format!("Bearer {}", app_token))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok {
        return Err(resp
            .error
            .unwrap_or_else(|| "connections.open failed".into()));
    }
    resp.url.ok_or_else(|| "no url in connections.open".into())
}

async fn post_message(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    text: &str,
    thread_ts: Option<&str>,
) -> Result<String, String> {
    let mut body = serde_json::json!({"channel": channel, "text": text});
    if let Some(ts) = thread_ts {
        if !ts.is_empty() {
            body["thread_ts"] = serde_json::Value::String(ts.to_string());
        }
    }
    let resp: PostMessageResp = client
        .post(api_url("chat.postMessage"))
        .header("Authorization", format!("Bearer {}", bot_token))
        .json(&body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "postMessage failed".into()));
    }
    resp.ts.ok_or_else(|| "no ts in postMessage".into())
}

async fn update_message(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    ts: &str,
    text: &str,
) -> Result<(), String> {
    let body = serde_json::json!({"channel": channel, "ts": ts, "text": text});
    let resp: SlackApiResp = client
        .post(api_url("chat.update"))
        .header("Authorization", format!("Bearer {}", bot_token))
        .json(&body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "chat.update failed".into()));
    }
    Ok(())
}

fn flush_display(text: &str) -> String {
    let mrkdwn = markdown_to_slack(text);
    let chunks = split_text(&mrkdwn, SLACK_TEXT_LIMIT);
    if chunks.is_empty() {
        return String::new();
    }
    let display = &chunks[0];
    let needs_truncation = chunks.len() > 1 || display.len() < mrkdwn.len();
    let mut display = display.to_string();
    if needs_truncation {
        display.push_str(TRUNCATION_NOTE);
    }
    if display.len() > SLACK_TEXT_LIMIT {
        display.truncate(SLACK_TEXT_LIMIT);
    }
    display
}

async fn do_flush(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    msg_ts: &str,
    text: &str,
) {
    let display = flush_display(text);
    if display.is_empty() {
        return;
    }
    if let Err(e) = update_message(client, bot_token, channel, msg_ts, &display).await {
        log::error!("slack update message: {}", e);
    }
}

async fn do_finalize(
    client: &reqwest::Client,
    bot_token: &str,
    channel: &str,
    msg_ts: &str,
    thread_ts: &str,
    text: &str,
) {
    let mrkdwn = markdown_to_slack(text);
    let chunks = split_text(&mrkdwn, SLACK_TEXT_LIMIT);
    for (i, chunk) in chunks.iter().enumerate() {
        let mut chunk = chunk.clone();
        if chunk.len() > SLACK_TEXT_LIMIT {
            chunk.truncate(SLACK_TEXT_LIMIT);
        }
        let result = if i == 0 {
            update_message(client, bot_token, channel, msg_ts, &chunk).await
        } else {
            post_message(client, bot_token, channel, &chunk, Some(thread_ts))
                .await
                .map(|_| ())
        };
        if let Err(e) = result {
            log::error!("slack finalize: {}", e);
        }
    }
}

struct ActivePrompt {
    cancel: oneshot::Sender<()>,
    done: oneshot::Receiver<()>,
}

struct BotInner {
    client: reqwest::Client,
    bot_token: String,
    app_token: String,
    bot_user_id: String,
    manager: Arc<Mutex<SessionManager>>,
    active_prompts: Mutex<HashMap<String, ActivePrompt>>,
}

impl BotInner {
    async fn run_loop(self: Arc<Self>) {
        loop {
            let url = match connections_open(&self.client, &self.app_token).await {
                Ok(u) => u,
                Err(e) => {
                    log::error!("slack connections.open: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            let ws = match tokio_tungstenite::connect_async(&url).await {
                Ok((ws, _)) => ws,
                Err(e) => {
                    log::error!("slack websocket connect: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            let mut ws = ws;

            loop {
                let msg = match ws.next().await {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        log::error!("slack websocket: {}", e);
                        break;
                    }
                    None => break,
                };

                if !msg.is_text() {
                    continue;
                }

                let text = msg.into_text().unwrap_or_default();
                let env: Envelope = match serde_json::from_str(&text) {
                    Ok(e) => e,
                    Err(e) => {
                        log::warn!("slack envelope parse: {}", e);
                        continue;
                    }
                };

                match env.env_type.as_str() {
                    "hello" => {
                        log::info!("slack connected");
                    }
                    "events_api" => {
                        if let Some(ref eid) = env.envelope_id {
                            let ack = serde_json::json!({"envelope_id": eid}).to_string();
                            let _ = ws.send(Message::Text(ack.into())).await;
                        }
                        if let Some(payload) = env.payload {
                            if let Some(event) = parse_event(&payload, &self.bot_user_id) {
                                let bot = self.clone();
                                tokio::spawn(async move {
                                    bot.handle_event(event).await;
                                });
                            }
                        }
                    }
                    "disconnect" => {
                        log::info!("slack disconnect, reconnecting");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    async fn handle_event(&self, event: SlackEvent) {
        let effective = if event.thread_ts.is_empty() {
            event.ts.clone()
        } else {
            event.thread_ts.clone()
        };
        self.process_prompt(event.channel, effective.clone(), effective, event.text)
            .await;
    }

    async fn process_prompt(
        &self,
        channel: String,
        session_key: String,
        thread_ts: String,
        text: String,
    ) {
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        let (done_tx, done_rx) = oneshot::channel::<()>();
        tokio::pin!(cancel_rx);

        let prior = self.active_prompts.lock().unwrap().remove(&session_key);
        if let Some(prior) = prior {
            let _ = prior.cancel.send(());
            let _ = tokio::time::timeout(PROMPT_CANCEL_TIMEOUT, prior.done).await;
        }

        self.active_prompts.lock().unwrap().insert(
            session_key.clone(),
            ActivePrompt {
                cancel: cancel_tx,
                done: done_rx,
            },
        );

        let msg_ts = match post_message(
            &self.client,
            &self.bot_token,
            &channel,
            "...",
            Some(&thread_ts),
        )
        .await
        {
            Ok(ts) => ts,
            Err(e) => {
                log::error!("slack post thinking: {}", e);
                self.active_prompts.lock().unwrap().remove(&session_key);
                let _ = done_tx.send(());
                return;
            }
        };

        let sess_arc = {
            let result = {
                let mut mgr = self.manager.lock().unwrap();
                mgr.get_or_create(&session_key)
            };
            match result {
                Ok(s) => s,
                Err(e) => {
                    log::error!("slack session: {}", e);
                    let _ = do_finalize(
                        &self.client,
                        &self.bot_token,
                        &channel,
                        &msg_ts,
                        &thread_ts,
                        "Error interno creando sesi\u{f3}n.",
                    )
                    .await;
                    self.active_prompts.lock().unwrap().remove(&session_key);
                    let _ = done_tx.send(());
                    return;
                }
            }
        };

        let mut events = {
            let mut sess = sess_arc.lock().unwrap();
            sess.prompt(&text)
        };

        let mut full_text = String::new();
        let mut last_flush = Instant::now();

        loop {
            tokio::select! {
                ev = events.recv() => {
                    match ev {
                        Some(ev) => match ev.event_type.as_str() {
                            "text_delta" => full_text.push_str(&ev.delta),
                            "error" => {
                                full_text.push_str("\n\nError: ");
                                full_text.push_str(&ev.error);
                            }
                            _ => {}
                        },
                        None => break,
                    }
                    if !full_text.is_empty() && last_flush.elapsed() >= FLUSH_INTERVAL {
                        do_flush(&self.client, &self.bot_token, &channel, &msg_ts, &full_text).await;
                        last_flush = Instant::now();
                    }
                }
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(last_flush) + FLUSH_INTERVAL), if !full_text.is_empty() => {
                    do_flush(&self.client, &self.bot_token, &channel, &msg_ts, &full_text).await;
                    last_flush = Instant::now();
                }
                _ = &mut cancel_rx => {
                    break;
                }
            }
        }

        if !full_text.is_empty() {
            do_finalize(
                &self.client,
                &self.bot_token,
                &channel,
                &msg_ts,
                &thread_ts,
                &full_text,
            )
            .await;
        }

        {
            let sess = sess_arc.lock().unwrap();
            let mut mgr = self.manager.lock().unwrap();
            let _ = mgr.save(&session_key, &sess);
        }

        self.active_prompts.lock().unwrap().remove(&session_key);
        let _ = done_tx.send(());
    }
}

fn parse_event(payload: &serde_json::Value, bot_user_id: &str) -> Option<SlackEvent> {
    let event = payload.get("event")?;
    let event_type = event.get("type")?.as_str()?;

    match event_type {
        "app_mention" => {
            let text = event.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let mention = format!("<@{}>", bot_user_id);
            let text = text.replace(&mention, "").trim().to_string();
            if text.is_empty() {
                return None;
            }
            let ts = event
                .get("ts")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let thread_ts = event
                .get("thread_ts")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let channel = event
                .get("channel")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(SlackEvent {
                text,
                ts,
                channel,
                thread_ts,
            })
        }
        "message" => {
            let channel_type = event
                .get("channel_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if channel_type != "im" {
                return None;
            }
            let subtype = event.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
            if !subtype.is_empty() {
                return None;
            }
            let text = event
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if text.is_empty() {
                return None;
            }
            let ts = event
                .get("ts")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let channel = event
                .get("channel")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(SlackEvent {
                text,
                ts,
                channel,
                thread_ts: String::new(),
            })
        }
        _ => None,
    }
}

pub struct BotAdapter;

impl Interface for BotAdapter {
    fn name(&self) -> &str {
        "slack"
    }

    fn run(&self, deps: Deps) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let bot_token = deps.config.slack.bot_token.clone();
        let app_token = deps.config.slack.app_token.clone();
        if bot_token.is_empty() || app_token.is_empty() {
            return Err("slack bot_token and app_token are required".into());
        }

        let http = reqwest::Client::new();

        let auth = {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(auth_test(&http, &bot_token))
        };
        let bot_user_id = match auth {
            Ok(id) => id,
            Err(e) => {
                log::error!("slack auth test: {}", e);
                String::new()
            }
        };

        let key_store_path = format!("{}/thread_sessions.json", deps.store.dir());
        let manager = deps.new_session_manager(&key_store_path)?;

        let bot = Arc::new(BotInner {
            client: http,
            bot_token,
            app_token,
            bot_user_id,
            manager: Arc::new(Mutex::new(manager)),
            active_prompts: Mutex::new(HashMap::new()),
        });

        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(bot.run_loop());
        Ok(())
    }

    fn notifier(&self, deps: &Deps) -> Option<Arc<dyn Notifier + Send + Sync>> {
        let bot_token = deps.config.slack.bot_token.clone();
        if bot_token.is_empty() {
            return None;
        }
        Some(Arc::new(SlackNotifier {
            client: reqwest::Client::new(),
            bot_token,
        }))
    }
}

struct SlackNotifier {
    client: reqwest::Client,
    bot_token: String,
}

impl Notifier for SlackNotifier {
    fn schedule_notify(
        &self,
        channel: &str,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            post_message(&self.client, &self.bot_token, channel, message, None)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_app_mention() {
        let payload = serde_json::json!({
            "event": {
                "type": "app_mention",
                "text": "<@U12345> hello world",
                "ts": "123.456",
                "channel": "Cabc",
                "thread_ts": "100.000"
            }
        });
        let ev = parse_event(&payload, "U12345").unwrap();
        assert_eq!(ev.text, "hello world");
        assert_eq!(ev.ts, "123.456");
        assert_eq!(ev.channel, "Cabc");
        assert_eq!(ev.thread_ts, "100.000");
    }

    #[test]
    fn test_parse_app_mention_no_thread() {
        let payload = serde_json::json!({
            "event": {
                "type": "app_mention",
                "text": "<@U12345> hi",
                "ts": "123.456",
                "channel": "Cabc"
            }
        });
        let ev = parse_event(&payload, "U12345").unwrap();
        assert_eq!(ev.text, "hi");
        assert!(ev.thread_ts.is_empty());
    }

    #[test]
    fn test_parse_app_mention_empty_text() {
        let payload = serde_json::json!({
            "event": {
                "type": "app_mention",
                "text": "<@U12345>",
                "ts": "123.456",
                "channel": "Cabc"
            }
        });
        assert!(parse_event(&payload, "U12345").is_none());
    }

    #[test]
    fn test_parse_dm_message() {
        let payload = serde_json::json!({
            "event": {
                "type": "message",
                "text": "hello",
                "ts": "123.456",
                "channel": "Dabc",
                "channel_type": "im"
            }
        });
        let ev = parse_event(&payload, "U12345").unwrap();
        assert_eq!(ev.text, "hello");
        assert_eq!(ev.ts, "123.456");
        assert_eq!(ev.channel, "Dabc");
        assert!(ev.thread_ts.is_empty());
    }

    #[test]
    fn test_parse_dm_message_wrong_type() {
        let payload = serde_json::json!({
            "event": {
                "type": "message",
                "text": "hello",
                "ts": "123.456",
                "channel": "Cabc",
                "channel_type": "channel"
            }
        });
        assert!(parse_event(&payload, "U12345").is_none());
    }

    #[test]
    fn test_parse_dm_message_with_subtype() {
        let payload = serde_json::json!({
            "event": {
                "type": "message",
                "text": "hello",
                "ts": "123.456",
                "channel": "Dabc",
                "channel_type": "im",
                "subtype": "message_changed"
            }
        });
        assert!(parse_event(&payload, "U12345").is_none());
    }

    #[test]
    fn test_parse_unknown_event() {
        let payload = serde_json::json!({
            "event": {
                "type": "reaction_added",
                "user": "U123"
            }
        });
        assert!(parse_event(&payload, "U12345").is_none());
    }

    #[test]
    fn test_flush_display_short() {
        let display = flush_display("Hello **world**");
        assert!(!display.is_empty());
        assert!(!display.contains(TRUNCATION_NOTE));
    }

    #[test]
    fn test_flush_display_truncation_note() {
        let long_text = format!("{}\n\n{}", "x".repeat(2500), "y".repeat(2500));
        let display = flush_display(&long_text);
        assert!(display.contains(TRUNCATION_NOTE));
    }

    #[test]
    fn test_flush_display_empty() {
        let display = flush_display("");
        assert!(display.is_empty());
    }

    #[test]
    fn test_session_key_from_mention() {
        let event = SlackEvent {
            text: "hello".into(),
            ts: "123.456".into(),
            channel: "Cabc".into(),
            thread_ts: "100.000".into(),
        };
        let key = if event.thread_ts.is_empty() {
            event.ts.clone()
        } else {
            event.thread_ts.clone()
        };
        assert_eq!(key, "100.000");
    }

    #[test]
    fn test_session_key_from_dm() {
        let event = SlackEvent {
            text: "hello".into(),
            ts: "123.456".into(),
            channel: "Dabc".into(),
            thread_ts: String::new(),
        };
        let key = if event.thread_ts.is_empty() {
            event.ts.clone()
        } else {
            event.thread_ts.clone()
        };
        assert_eq!(key, "123.456");
    }
}
