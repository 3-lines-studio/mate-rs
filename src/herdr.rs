use serde_json::json;
use std::env;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SOURCE: &str = "custom:mate";
const AGENT: &str = "mate";
const TIMEOUT: Duration = Duration::from_millis(500);

static SEQ: OnceLock<AtomicU64> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Working,
    Blocked,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Working => "working",
            State::Blocked => "blocked",
        }
    }
}

struct Ctx {
    socket_path: String,
    pane_id: String,
}

fn ctx() -> Option<Ctx> {
    if env::var("HERDR_ENV").ok().as_deref() != Some("1") {
        return None;
    }
    let socket_path = env::var("HERDR_SOCKET_PATH").ok()?;
    let pane_id = env::var("HERDR_PANE_ID").ok()?;
    if socket_path.is_empty() || pane_id.is_empty() {
        return None;
    }
    Some(Ctx {
        socket_path,
        pane_id,
    })
}

fn next_seq() -> u64 {
    let seq = SEQ.get_or_init(|| {
        let base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
            .saturating_mul(1000);
        AtomicU64::new(base)
    });
    seq.fetch_add(1, Ordering::Relaxed) + 1
}

fn send(method: &str, mut params: serde_json::Value) {
    let Some(ctx) = ctx() else {
        return;
    };
    let seq = next_seq();
    if let Some(obj) = params.as_object_mut() {
        obj.entry("seq").or_insert(json!(seq));
        obj.entry("pane_id").or_insert(json!(ctx.pane_id.clone()));
        obj.entry("source").or_insert(json!(SOURCE));
        obj.entry("agent").or_insert(json!(AGENT));
    }
    let req = json!({
        "id": format!("mate:{}:{}", method, seq),
        "method": method,
        "params": params,
    });
    let Ok(mut stream) = UnixStream::connect(&ctx.socket_path) else {
        return;
    };
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    let line = match serde_json::to_string(&req) {
        Ok(s) => s + "\n",
        Err(_) => return,
    };
    let _ = stream.write_all(line.as_bytes());
    let _ = stream.flush();
}

pub fn report(state: State) {
    send("pane.report_agent", json!({ "state": state.as_str() }));
}

pub fn release() {
    send("pane.release_agent", json!({}));
}

/// Reports `working` on create and `idle` on drop. No-op when inactive or not under Herdr.
pub struct WorkingGuard {
    active: bool,
}

impl WorkingGuard {
    pub fn enter(active: bool) -> Self {
        if active {
            report(State::Working);
        }
        Self { active }
    }
}

impl Drop for WorkingGuard {
    fn drop(&mut self) {
        if self.active {
            report(State::Idle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_noop_without_env() {
        unsafe {
            env::remove_var("HERDR_ENV");
            env::remove_var("HERDR_SOCKET_PATH");
            env::remove_var("HERDR_PANE_ID");
        }
        report(State::Working);
        release();
        let _ = WorkingGuard::enter(true);
    }
}
