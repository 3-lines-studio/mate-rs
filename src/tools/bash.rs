use crate::tools::define_tool;
use crate::tools::Tool;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Stdio;

#[cfg(unix)]
use libc;

#[derive(Debug, Deserialize)]
pub struct BashParams {
    pub command: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub description: String,
    #[serde(default)]
    pub max_lines: i32,
}

pub fn tool() -> Tool {
    let mut params = HashMap::new();
    params.insert("type".to_string(), serde_json::json!("object"));
    let mut properties: HashMap<String, serde_json::Value> = HashMap::new();
    properties.insert(
        "command".to_string(),
        serde_json::json!({"type": "string", "description": "The shell command to execute"}),
    );
    properties.insert(
        "description".to_string(),
        serde_json::json!({"type": "string", "description": "Brief description of what this command does"}),
    );
    properties.insert(
        "max_lines".to_string(),
        serde_json::json!({"type": "integer", "description": "Maximum lines to return from the end of output (default: 500)"}),
    );
    params.insert("properties".to_string(), serde_json::json!(properties));
    params.insert(
        "required".to_string(),
        serde_json::json!(["command"]),
    );

    define_tool(
        "bash",
        "Execute a shell command in the current working directory. Returns combined stdout and stderr. Output is truncated to last max_lines lines (default: 500).",
        params,
        |p: BashParams| async move {
            execute_bash(p).await
        },
    )
}

async fn execute_bash(p: BashParams) -> Result<String, String> {
    let script = format!("trap 'pkill -TERM -P $$ 2>/dev/null' EXIT\n{}", p.command);

    let mut script_file = tempfile::Builder::new()
        .prefix("mate-bash-")
        .suffix(".sh")
        .tempfile()
        .map_err(|e| format!("creating temp script: {}", e))?;

    std::io::Write::write_all(&mut script_file, script.as_bytes())
        .map_err(|e| format!("writing temp script: {}", e))?;

    let script_path = script_file.path().to_path_buf();
    let max_lines = if p.max_lines <= 0 { 500 } else { p.max_lines };

    let mut cmd = tokio::process::Command::new("bash");
    cmd.arg(script_path.to_str().unwrap_or(""))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .process_group(0);

    let mut child = cmd.spawn().map_err(|e| format!("spawn bash: {}", e))?;
    let pid = child.id();

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "stdout not piped".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "stderr not piped".to_string())?;

    let timeout = tokio::time::Duration::from_secs(120);
    let merged = tokio::time::timeout(timeout, async {
        use tokio::io::AsyncReadExt;
        let mut merged: Vec<u8> = Vec::new();
        let mut obuf = [0u8; 8192];
        let mut ebuf = [0u8; 8192];
        let mut out_done = false;
        let mut err_done = false;
        loop {
            tokio::select! {
                res = stdout.read(&mut obuf), if !out_done => match res {
                    Ok(0) | Err(_) => out_done = true,
                    Ok(n) => merged.extend_from_slice(&obuf[..n]),
                },
                res = stderr.read(&mut ebuf), if !err_done => match res {
                    Ok(0) | Err(_) => err_done = true,
                    Ok(n) => merged.extend_from_slice(&ebuf[..n]),
                },
            }
            if out_done && err_done {
                break;
            }
        }
        merged
    })
    .await;

    #[cfg(unix)]
    if let Some(pid) = pid {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
            std::thread::sleep(std::time::Duration::from_millis(20));
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = std::fs::remove_file(&script_path);

    match merged {
        Ok(bytes) => {
            let status = child.wait().await.map_err(|e| format!("bash error: {}", e))?;
            let s = String::from_utf8_lossy(&bytes).to_string();
            let result_str = if !status.success() {
                let trimmed = s.trim().to_string();
                if trimmed.is_empty() {
                    status.to_string()
                } else {
                    trimmed
                }
            } else {
                s.trim().to_string()
            };
            if result_str.is_empty() {
                return Ok(result_str);
            }
            let lines: Vec<&str> = result_str.lines().collect();
            if lines.len() as i32 <= max_lines {
                return Ok(result_str);
            }
            let truncated = &lines[(lines.len() as i32 - max_lines) as usize..];
            Ok(truncated.join("\n"))
        }
        Err(_) => {
            let _ = child.wait().await;
            Err("bash timed out after 120s".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run_bash(params: BashParams) -> Result<String, String> {
        execute_bash(params).await
    }

    #[tokio::test]
    async fn test_bash_basic() {
        let result = run_bash(BashParams {
            command: "echo hello".to_string(),
            description: String::new(),
            max_lines: 0,
        })
        .await
        .unwrap();
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn test_bash_max_lines_default_large() {
        let result = run_bash(BashParams {
            command: "for i in $(seq 1 600); do echo x; done".to_string(),
            description: String::new(),
            max_lines: 5,
        })
        .await
        .unwrap();
        let count = result.lines().count() as i32;
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_bash_max_lines_zero_defaults() {
        let result = run_bash(BashParams {
            command: "for i in $(seq 1 600); do echo x; done".to_string(),
            description: String::new(),
            max_lines: 0,
        })
        .await
        .unwrap();
        let count = result.lines().count() as i32;
        assert_eq!(count, 500);
    }

    #[tokio::test]
    async fn test_bash_max_lines_custom() {
        let result = run_bash(BashParams {
            command: "for i in $(seq 1 20); do echo $i; done".to_string(),
            description: String::new(),
            max_lines: 3,
        })
        .await
        .unwrap();
        let count = result.lines().count() as i32;
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_bash_max_lines_negative() {
        let result = run_bash(BashParams {
            command: "for i in $(seq 1 10); do echo x; done".to_string(),
            description: String::new(),
            max_lines: -1,
        })
        .await
        .unwrap();
        let count = result.lines().count() as i32;
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn test_bash_empty_output() {
        let result = run_bash(BashParams {
            command: "true".to_string(),
            description: String::new(),
            max_lines: 0,
        })
        .await
        .unwrap();
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn test_bash_stderr_output() {
        let result = run_bash(BashParams {
            command: "echo error >&2".to_string(),
            description: String::new(),
            max_lines: 0,
        })
        .await
        .unwrap();
        assert!(result.contains("error"));
    }

    #[tokio::test]
    async fn test_bash_failing_command() {
        let result = run_bash(BashParams {
            command: "exit 1".to_string(),
            description: String::new(),
            max_lines: 0,
        })
        .await
        .unwrap();
        assert!(result.contains("exit status"));
    }

    #[tokio::test]
    async fn test_bash_failing_command_with_output() {
        let result = run_bash(BashParams {
            command: "echo some message; exit 1".to_string(),
            description: String::new(),
            max_lines: 0,
        })
        .await
        .unwrap();
        assert!(result.contains("some message"));
    }

    #[tokio::test]
    async fn test_bash_background_process_does_not_hang() {
        let result = run_bash(BashParams {
            command: "sleep 1 & echo started; echo done".to_string(),
            description: String::new(),
            max_lines: 0,
        })
        .await
        .unwrap();
        assert!(result.contains("started"));
        assert!(result.contains("done"));
    }
}
