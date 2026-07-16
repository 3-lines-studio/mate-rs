use serde_json;

pub fn format_tokens(n: i32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{}", n)
    }
}

pub fn tool_color(name: &str) -> &'static str {
    match name {
        "read_file" | "grep" | "glob" => "#99FFE4",
        "write_file" | "edit_file" | "bash" => "#FFCB8B",
        _ => "#FFC799",
    }
}

pub fn format_tool_label(cwd: &str, name: &str, args: &str) -> String {
    if args.is_empty() {
        return name.to_string();
    }
    match name {
        "read_file" => {
            if let Ok(p) = serde_json::from_str::<serde_json::Value>(args) {
                let path = p["path"].as_str().unwrap_or("");
                let offset = p["offset"].as_i64().unwrap_or(0);
                let limit = p["limit"].as_i64().unwrap_or(0);
                if !path.is_empty() {
                    let label = rel_path(cwd, path);
                    if offset > 0 || limit > 0 {
                        let end = if limit > 0 {
                            format!("-{}", offset + limit - 1)
                        } else {
                            String::new()
                        };
                        return format!("{}({} L{}{})", name, label, offset, end);
                    }
                    return format!("{}({})", name, label);
                }
            }
        }
        "write_file" => {
            if let Ok(p) = serde_json::from_str::<serde_json::Value>(args) {
                let path = p["path"].as_str().unwrap_or("");
                let content = p["content"].as_str().unwrap_or("");
                if !path.is_empty() {
                    return format!("{}({}, {}B)", name, rel_path(cwd, path), content.len());
                }
            }
        }
        "edit_file" => {
            if let Ok(p) = serde_json::from_str::<serde_json::Value>(args) {
                let path = p["path"].as_str().unwrap_or("");
                let edits = p["edits"].as_array().map(|a| a.len()).unwrap_or(0);
                if !path.is_empty() {
                    return format!("{}({}, {} edits)", name, rel_path(cwd, path), edits);
                }
            }
        }
        "bash" => {
            if let Ok(p) = serde_json::from_str::<serde_json::Value>(args) {
                let cmd = p["command"].as_str().unwrap_or("");
                if !cmd.is_empty() {
                    return format!("{}({})", name, cmd);
                }
            }
        }
        "delegate" => {
            if let Ok(p) = serde_json::from_str::<serde_json::Value>(args) {
                let subagent = p["subagent"].as_str().unwrap_or("");
                let task = p["task"].as_str().unwrap_or("");
                if !subagent.is_empty() {
                    let mut label = subagent.to_string();
                    if !task.is_empty() {
                        let t = if task.len() > 80 {
                            format!("{}…", &task[..80])
                        } else {
                            task.to_string()
                        };
                        label = format!("{}: {}", label, t);
                    }
                    return format!("{}({})", name, label);
                }
            }
        }
        _ => {}
    }
    format!("{}({})", name, args)
}

fn rel_path(cwd: &str, path: &str) -> String {
    if cwd.is_empty() {
        return path.to_string();
    }
    let cwd_path = std::path::Path::new(cwd);
    let p = std::path::Path::new(path);
    match p.strip_prefix(cwd_path) {
        Ok(rel) => {
            let s = rel.to_string_lossy();
            if s.is_empty() {
                ".".to_string()
            } else {
                s.to_string()
            }
        }
        Err(_) => path.to_string(),
    }
}

pub fn result_lang(tool_name: &str, args: &str) -> String {
    match tool_name {
        "bash" => "bash".to_string(),
        "read_file" => {
            if let Ok(p) = serde_json::from_str::<serde_json::Value>(args) {
                let path = p["path"].as_str().unwrap_or("");
                if !path.is_empty() {
                    let ext = std::path::Path::new(path)
                        .extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_default();
                    return ext;
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

pub fn strip_box_drawing(s: &str) -> String {
    s.chars()
        .filter(|c| !('\u{2500}'..='\u{257F}').contains(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
    }

    #[test]
    fn test_format_tool_label_read_file() {
        let label = format_tool_label(
            "/home/user/project",
            "read_file",
            r#"{"path":"/home/user/project/src/main.rs","offset":10,"limit":5}"#,
        );
        assert!(label.contains("src/main.rs"));
        assert!(label.contains("L10-14"));
    }

    #[test]
    fn test_format_tool_label_bash() {
        let label = format_tool_label("/home/user", "bash", r#"{"command":"ls -la"}"#);
        assert!(label.contains("ls -la"));
    }
}
