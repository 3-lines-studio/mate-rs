use serde_json::json;
use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SOURCE: &str = "custom:mate";
const AGENT: &str = "mate";
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const RETRY_TIMEOUT: Duration = Duration::from_millis(1500);
const HEARTBEAT: Duration = Duration::from_secs(15);
const SHUTDOWN_WAIT: Duration = Duration::from_millis(800);

static SEQ: OnceLock<AtomicU64> = OnceLock::new();
static TX: OnceLock<Sender<Cmd>> = OnceLock::new();
static JOIN: OnceLock<Mutex<Option<JoinHandle<()>>>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

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

#[derive(Debug)]
enum Cmd {
    Report(State),
    Release,
    Shutdown,
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

fn ensure_worker() -> Option<&'static Sender<Cmd>> {
    ctx()?;
    Some(TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let handle = thread::Builder::new()
            .name("mate-herdr".into())
            .spawn(move || worker_loop(rx))
            .expect("herdr worker");
        JOIN.get_or_init(|| Mutex::new(Some(handle)));
        STARTED.store(true, Ordering::SeqCst);
        tx
    }))
}

fn worker_loop(rx: mpsc::Receiver<Cmd>) {
    let mut pending: Option<State> = None;
    let mut want_release = false;
    let mut shutting_down = false;
    let mut last_sent: Option<State> = None;
    let mut last_ok_at = Instant::now() - HEARTBEAT;
    let mut fail_backoff = Duration::from_millis(100);

    loop {
        let timeout = if pending.is_some() || want_release {
            Duration::from_millis(0)
        } else if last_sent.is_some() {
            HEARTBEAT
                .checked_sub(last_ok_at.elapsed())
                .unwrap_or(Duration::ZERO)
                .max(Duration::from_millis(50))
        } else {
            Duration::from_secs(60)
        };

        match rx.recv_timeout(timeout) {
            Ok(Cmd::Report(state)) if !shutting_down => {
                pending = Some(state);
                want_release = false;
            }
            Ok(Cmd::Report(_)) => {}
            Ok(Cmd::Release) => {
                want_release = true;
            }
            Ok(Cmd::Shutdown) => {
                shutting_down = true;
                want_release = true;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                want_release = true;
                shutting_down = true;
            }
        }

        // Drain any burst so we only send the latest state.
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Cmd::Report(state) if !shutting_down => {
                    pending = Some(state);
                    want_release = false;
                }
                Cmd::Report(_) => {}
                Cmd::Release => want_release = true,
                Cmd::Shutdown => {
                    shutting_down = true;
                    want_release = true;
                }
            }
        }

        if shutting_down {
            // Quit path ends authority via release; drop queued state updates.
            pending = None;
        } else if let Some(state) = pending.take() {
            let force = last_ok_at.elapsed() >= HEARTBEAT;
            if force || last_sent != Some(state) {
                if send_agent(state) {
                    last_sent = Some(state);
                    last_ok_at = Instant::now();
                    fail_backoff = Duration::from_millis(100);
                } else {
                    pending = Some(state);
                    thread::sleep(fail_backoff);
                    fail_backoff = (fail_backoff * 2).min(Duration::from_secs(5));
                    continue;
                }
            }
        } else if last_sent.is_some()
            && last_ok_at.elapsed() >= HEARTBEAT
            && let Some(state) = last_sent
        {
            if send_agent(state) {
                last_ok_at = Instant::now();
                fail_backoff = Duration::from_millis(100);
            } else {
                thread::sleep(fail_backoff);
                fail_backoff = (fail_backoff * 2).min(Duration::from_secs(5));
            }
        }

        if want_release {
            if send_release() {
                want_release = false;
                last_sent = None;
                fail_backoff = Duration::from_millis(100);
            } else {
                thread::sleep(fail_backoff);
                fail_backoff = (fail_backoff * 2).min(Duration::from_secs(5));
                continue;
            }
        }

        if shutting_down && pending.is_none() && !want_release {
            break;
        }
    }
}

fn build_request(method: &str, mut params: serde_json::Value, ctx: &Ctx) -> serde_json::Value {
    let seq = next_seq();
    if let Some(obj) = params.as_object_mut() {
        obj.insert("seq".into(), json!(seq));
        obj.insert("pane_id".into(), json!(ctx.pane_id.clone()));
        obj.insert("source".into(), json!(SOURCE));
        obj.insert("agent".into(), json!(AGENT));
    }
    json!({
        "id": format!("mate:{}:{}", method, seq),
        "method": method,
        "params": params,
    })
}

fn send_agent(state: State) -> bool {
    let Some(ctx) = ctx() else {
        return true;
    };
    let req = build_request(
        "pane.report_agent",
        json!({ "state": state.as_str() }),
        &ctx,
    );
    deliver(&ctx.socket_path, &req)
}

fn send_release() -> bool {
    let Some(ctx) = ctx() else {
        return true;
    };
    let req = build_request("pane.release_agent", json!({}), &ctx);
    deliver(&ctx.socket_path, &req)
}

/// Write one JSON line and wait for any response byte. Retry once on failure.
fn deliver(socket_path: &str, req: &serde_json::Value) -> bool {
    if deliver_once(socket_path, req, CONNECT_TIMEOUT) {
        return true;
    }
    deliver_once(socket_path, req, RETRY_TIMEOUT)
}

fn deliver_once(socket_path: &str, req: &serde_json::Value, timeout: Duration) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket_path) else {
        return false;
    };
    let _ = stream.set_write_timeout(Some(timeout));
    let _ = stream.set_read_timeout(Some(timeout));
    let Ok(body) = serde_json::to_string(req) else {
        return false;
    };
    let line = body + "\n";
    if stream.write_all(line.as_bytes()).is_err() {
        return false;
    }
    if stream.flush().is_err() {
        return false;
    }
    // Match Herdr's official integrations: only an ACK body (or clean EOF) counts.
    // Timeout/error → caller retries once with a longer budget.
    let mut buf = [0u8; 64];
    match stream.read(&mut buf) {
        Ok(0) => true,
        Ok(_) => true,
        Err(_) => false,
    }
}

fn send_cmd(cmd: Cmd) {
    let Some(tx) = ensure_worker() else {
        return;
    };
    let _ = tx.send(cmd);
}

pub fn report(state: State) {
    send_cmd(Cmd::Report(state));
}

pub fn release() {
    send_cmd(Cmd::Release);
}

/// Flush a release and stop the worker. Best-effort; bounded wait.
pub fn shutdown() {
    if !STARTED.load(Ordering::SeqCst) {
        let _ = send_release();
        return;
    }
    if let Some(tx) = TX.get() {
        let _ = tx.send(Cmd::Shutdown);
    }
    if let Some(cell) = JOIN.get()
        && let Ok(mut guard) = cell.lock()
        && let Some(handle) = guard.take()
    {
        let start = Instant::now();
        loop {
            if handle.is_finished() {
                let _ = handle.join();
                break;
            }
            if start.elapsed() >= SHUTDOWN_WAIT {
                // Detach: worker dies with process.
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
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
    use std::os::unix::net::UnixListener;
    use std::sync::{Arc, Mutex as StdMutex};

    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn clear_env() {
        unsafe {
            env::remove_var("HERDR_ENV");
            env::remove_var("HERDR_SOCKET_PATH");
            env::remove_var("HERDR_PANE_ID");
        }
    }

    #[test]
    fn report_noop_without_env() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_env();
        assert!(send_agent(State::Working));
        assert!(send_release());
    }

    #[test]
    fn deliver_writes_json_line_and_accepts_ack() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_env();

        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("herdr.sock");
        let sock_s = sock.to_str().unwrap().to_string();
        let listener = UnixListener::bind(&sock).unwrap();
        listener.set_nonblocking(true).unwrap();

        let got = Arc::new(StdMutex::new(None::<String>));
        let got2 = got.clone();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if Instant::now() > deadline {
                    return;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = vec![0u8; 4096];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        *got2.lock().unwrap() =
                            Some(String::from_utf8_lossy(&buf[..n]).to_string());
                        let _ = stream.write_all(br#"{"ok":true}"#);
                        let _ = stream.flush();
                        return;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => return,
                }
            }
        });

        let ctxs = Ctx {
            socket_path: sock_s.clone(),
            pane_id: "w1:p1".into(),
        };
        let req = build_request("pane.report_agent", json!({ "state": "working" }), &ctxs);
        assert!(deliver(&sock_s, &req));
        server.join().unwrap();

        let msg = got.lock().unwrap().clone().expect("request received");
        let v: serde_json::Value = serde_json::from_str(msg.trim()).unwrap();
        assert_eq!(v["method"], "pane.report_agent");
        assert_eq!(v["params"]["agent"], "mate");
        assert_eq!(v["params"]["source"], "custom:mate");
        assert_eq!(v["params"]["pane_id"], "w1:p1");
        assert_eq!(v["params"]["state"], "working");
        assert!(v["params"]["seq"].as_u64().unwrap() > 0);
    }

    #[test]
    fn deliver_fails_fast_on_missing_socket() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let req = json!({"id":"t","method":"ping","params":{}});
        let start = Instant::now();
        assert!(!deliver("/tmp/mate-herdr-does-not-exist.sock", &req));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
