use crate::tools::define_tool;
use crate::tools::Tool;
use futures_util::StreamExt;
use htmd::HtmlToMarkdown;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::RwLock;
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize)]
pub struct WebFetchParams {
    pub url: String,
    #[serde(default)]
    pub max_size: i32,
}

const DNS_CACHE_TTL: Duration = Duration::from_secs(300);
const DEFAULT_MAX_SIZE: i32 = 100_000;
const MAX_MAX_SIZE: i32 = 200_000;

struct DnsCacheEntry {
    addrs: Vec<IpAddr>,
    expires: Instant,
}

static DNS_CACHE: once_cell::sync::Lazy<RwLock<HashMap<String, DnsCacheEntry>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

pub fn tool() -> Tool {
    let mut params = HashMap::new();
    params.insert("type".to_string(), serde_json::json!("object"));
    let mut properties: HashMap<String, serde_json::Value> = HashMap::new();
    properties.insert(
        "url".to_string(),
        serde_json::json!({"type": "string", "description": "URL to fetch (https:// or http://)"}),
    );
    properties.insert(
        "max_size".to_string(),
        serde_json::json!({"type": "integer", "description": "Maximum bytes to return (default: 100000, max: 200000)"}),
    );
    params.insert("properties".to_string(), serde_json::json!(properties));
    params.insert("required".to_string(), serde_json::json!(["url"]));

    define_tool(
        "web_fetch",
        "Fetch a URL and return its content as Markdown. Automatically uses a headless browser if the page appears to be a JavaScript SPA.",
        params,
        |p: WebFetchParams| async move { execute_webfetch(p).await },
    )
}

async fn execute_webfetch(mut p: WebFetchParams) -> Result<String, String> {
    validate_and_clamp_url(&p.url, &mut p.max_size).await?;
    let result = fetch_http(&p.url, p.max_size).await?;

    if looks_like_spa_shell(&result) {
        return Ok(result);
    }

    let is_html = result
        .lines()
        .find(|l| l.starts_with("Content-Type:"))
        .map(|l| l.contains("text/html"))
        .unwrap_or(false);

    if is_html {
        if let Some((header, body)) = result.split_once("\n\n") {
            match HtmlToMarkdown::builder()
                .skip_tags(vec!["script", "style"])
                .build()
                .convert(body)
            {
                Ok(md) => return Ok(format!("{}\n\n{}", header, md)),
                Err(_) => return Ok(result),
            }
        }
    }

    Ok(result)
}

pub fn init() {
    // Register tool in catalog
    // This is called automatically at startup
}

async fn validate_and_clamp_url(url: &str, max_size: &mut i32) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("invalid URL: must start with http:// or https://".to_string());
    }
    if *max_size <= 0 {
        *max_size = DEFAULT_MAX_SIZE;
    }
    if *max_size > MAX_MAX_SIZE {
        *max_size = MAX_MAX_SIZE;
    }
    let host = host_from_url(url).ok_or_else(|| "parse URL".to_string())?;
    check_public_host(&host).await?;
    Ok(())
}

fn host_from_url(raw: &str) -> Option<String> {
    let after = raw.split("://").nth(1)?;
    let rest = after.split('/').next()?;

    // Handle IPv6 bracket notation
    if rest.starts_with('[') {
        let end = rest.find(']')?;
        return Some(rest[1..end].to_string());
    }

    // Strip port
    let host = rest.split(':').next()?;
    Some(host.to_string())
}

async fn check_public_host(host: &str) -> Result<(), String> {
    // Check if it's an IP
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_addr(&ip) {
            return Err(format!("blocked: IP {} is private/loopback/link-local", ip));
        }
        return Ok(());
    }

    // Check DNS cache
    {
        let cache = DNS_CACHE.read().unwrap();
        if let Some(entry) = cache.get(host) {
            if Instant::now() < entry.expires {
                for ip in &entry.addrs {
                    if is_private_addr(ip) {
                        return Err(format!("blocked: {} resolves to private IP {}", host, ip));
                    }
                }
                return Ok(());
            }
        }
    }

    // DNS lookup
    let addrs: Vec<IpAddr> = tokio::net::lookup_host(format!("{}:0", host))
        .await
        .map_err(|e| format!("DNS lookup failed for {}: {}", host, e))?
        .map(|sa| sa.ip())
        .collect();

    if addrs.is_empty() {
        return Err(format!("no addresses found for {}", host));
    }

    for ip in &addrs {
        if is_private_addr(ip) {
            return Err(format!("blocked: {} resolves to private IP {}", host, ip));
        }
    }

    let mut cache = DNS_CACHE.write().unwrap();
    cache.insert(
        host.to_string(),
        DnsCacheEntry {
            addrs,
            expires: Instant::now() + DNS_CACHE_TTL,
        },
    );

    Ok(())
}

fn is_private_addr(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            let octets = v6.octets();
            // fc00::/7 — unique local addresses (ULA)
            if octets[0] & 0xfe == 0xfc {
                return true;
            }
            // fe80::/10 — link-local unicast
            if octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80 {
                return true;
            }
            // ff00::/8 — multicast
            if octets[0] == 0xff {
                return true;
            }
            false
        }
    }
}

fn is_text_content(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("javascript")
}

async fn fetch_http(url: &str, max_size: i32) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("too many redirects");
            }
            let new_url = attempt.url().clone();
            let new_host = new_url.host_str().unwrap_or("");
            if let Some(first) = attempt.previous().first() {
                if first.host_str() == Some(new_host) {
                    return attempt.follow();
                }
            }
            let port = new_url.port_or_known_default().unwrap_or(80);
            let addr_str = format!("{}:{}", new_host, port);
            match addr_str.to_socket_addrs() {
                Ok(addrs) => {
                    for sa in addrs {
                        if is_private_addr(&sa.ip()) {
                            return attempt.error("redirect to private IP blocked");
                        }
                    }
                    attempt.follow()
                }
                Err(_) => attempt.error("DNS resolution failed for redirect target"),
            }
        }))
        .build()
        .map_err(|e| format!("create client: {}", e))?;

    let resp = client
        .get(url)
        .header("User-Agent", "mate/1.0")
        .send()
        .await
        .map_err(|e| format!("fetch {}: {}", url, e))?;

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let status = resp.status();
    let mut body = Vec::with_capacity(max_size as usize);
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("read response: {}", e))?;
        let remaining = (max_size as usize) - body.len();
        if remaining == 0 {
            break;
        }
        let take = chunk.len().min(remaining);
        body.extend_from_slice(&chunk[..take]);
        if take < chunk.len() {
            break;
        }
    }
    let truncated = &body;

    if !is_text_content(&content_type) {
        return Err(format!(
            "non-text content type: {} (status: {})",
            content_type,
            status.as_u16()
        ));
    }

    let body = String::from_utf8_lossy(truncated);

    let result = format!(
        "HTTP {} {}\nContent-Type: {}\n\n{}",
        status.as_u16(),
        status.canonical_reason().unwrap_or(""),
        content_type,
        body
    );

    Ok(result)
}

const SPA_MOUNT_POINTS: &[&str] = &[
    r#"<div id="root""#,
    r#"<div id="app""#,
    r#"<div id="__next""#,
    r#"<div id="__nuxt""#,
];

pub fn looks_like_spa_shell(fetch_result: &str) -> bool {
    let after = match fetch_result.split_once("\n\n") {
        Some((_, after)) => after,
        None => return false,
    };

    for mp in SPA_MOUNT_POINTS {
        if after.contains(mp) {
            let text = strip_tags_for_detection(after);
            return text.len() < 500;
        }
    }

    false
}

pub fn strip_tags_for_detection(html: &str) -> String {
    let html = remove_blocks_for_detection(html, "script");
    let html = remove_blocks_for_detection(&html, "style");

    let mut result = String::new();
    let mut in_tag = false;

    for c in html.chars() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if c == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag {
            result.push(c);
        }
    }

    result.trim().to_string()
}

fn remove_blocks_for_detection(s: &str, tag: &str) -> String {
    let lower = s.to_lowercase();
    let open_prefix = format!("<{}", tag);
    let close_tag = format!("</{}>", tag);

    let mut s = s.to_string();
    let mut lower = lower;

    while let Some(start) = lower.find(&open_prefix) {
        let end = match lower[start..].find(&close_tag) {
            Some(pos) => pos + close_tag.len(),
            None => break,
        };
        let end = start + end;

        s = format!("{}{}", &s[..start], &s[end..]);
        lower = format!("{}{}", &lower[..start], &lower[end..]);
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_validate_url_valid() {
        let mut max_size = 5000;
        let result = validate_and_clamp_url("https://example.com", &mut max_size).await;
        assert!(result.is_ok());
        assert_eq!(max_size, 5000);
    }

    #[tokio::test]
    async fn test_validate_url_http() {
        let mut max_size = 1000;
        assert!(validate_and_clamp_url("http://example.com", &mut max_size)
            .await
            .is_ok());
    }

    #[test]
    fn test_validate_url_no_protocol() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut max_size = 100;
        let result = rt.block_on(validate_and_clamp_url("example.com", &mut max_size));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_url_ftp_not_allowed() {
        let mut max_size = 100;
        assert!(validate_and_clamp_url("ftp://files.com", &mut max_size)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_validate_url_default_max_size() {
        let mut max_size = 0;
        assert!(validate_and_clamp_url("https://x.com", &mut max_size)
            .await
            .is_ok());
        assert_eq!(max_size, 100000);
    }

    #[tokio::test]
    async fn test_validate_url_negative_max_size() {
        let mut max_size = -5;
        assert!(validate_and_clamp_url("https://x.com", &mut max_size)
            .await
            .is_ok());
        assert_eq!(max_size, 100000);
    }

    #[tokio::test]
    async fn test_validate_url_clamped() {
        let mut max_size = 999999;
        assert!(validate_and_clamp_url("https://x.com", &mut max_size)
            .await
            .is_ok());
        assert_eq!(max_size, 200000);
    }

    #[test]
    fn test_is_text_content() {
        for ct in &[
            "text/html",
            "text/plain; charset=utf-8",
            "application/json",
            "application/xml",
            "text/javascript",
        ] {
            assert!(is_text_content(ct), "{} should be text", ct);
        }
        assert!(!is_text_content("image/png"));
        assert!(!is_text_content("application/octet-stream"));
    }

    #[test]
    fn test_looks_like_spa_react() {
        let result = "HTTP 200 OK\nContent-Type: text/html\n\n<html><div id=\"root\"></div></html>";
        assert!(looks_like_spa_shell(result));
    }

    #[test]
    fn test_looks_like_spa_nextjs() {
        let result = "HTTP 200 OK\n\n<div id=\"__next\"><!-- empty --></div>";
        assert!(looks_like_spa_shell(result));
    }

    #[test]
    fn test_looks_like_spa_nuxt() {
        let result = "HTTP 200 OK\n\n<div id=\"__nuxt\"></div>";
        assert!(looks_like_spa_shell(result));
    }

    #[test]
    fn test_looks_like_spa_plain_html_not_spa() {
        let result = "HTTP 200 OK\nContent-Type: text/html\n\n<html><body><p>Hello world, this is a lot of text. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. More text here to exceed the 500 character threshold for SPA detection. Padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding.</p></body></html>";
        assert!(!looks_like_spa_shell(result));
    }

    #[test]
    fn test_looks_like_spa_json_not_spa() {
        let result = "HTTP 200 OK\nContent-Type: application/json\n\n{\"key\":\"value\"}";
        assert!(!looks_like_spa_shell(result));
    }

    #[test]
    fn test_looks_like_spa_no_separator() {
        let result = "just a single line";
        assert!(!looks_like_spa_shell(result));
    }

    #[test]
    fn test_strip_tags_removes_tags() {
        let html = "<html><body><p>hello</p></body></html>";
        assert_eq!(strip_tags_for_detection(html), "hello");
    }

    #[test]
    fn test_strip_tags_removes_script_block() {
        let html = "<div>before</div><script>console.log('x')</script><div>after</div>";
        assert_eq!(strip_tags_for_detection(html), "beforeafter");
    }

    #[test]
    fn test_strip_tags_removes_style_block() {
        let html = "<div>text</div><style>.x{color:red}</style><div>more</div>";
        assert_eq!(strip_tags_for_detection(html), "textmore");
    }

    #[test]
    fn test_remove_blocks() {
        assert_eq!(
            remove_blocks_for_detection("a<script>code</script>b", "script"),
            "ab"
        );
    }

    #[test]
    fn test_remove_blocks_no_closing_tag() {
        assert_eq!(
            remove_blocks_for_detection("a<script>no close", "script"),
            "a<script>no close"
        );
    }

    #[test]
    fn test_remove_blocks_case_insensitive() {
        assert_eq!(
            remove_blocks_for_detection("a<SCRIPT>code</SCRIPT>b", "script"),
            "ab"
        );
    }

    #[tokio::test]
    async fn test_fetch_http_text_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("hello world")
                    .insert_header("Content-Type", "text/plain"),
            )
            .mount(&mock_server)
            .await;

        let result = fetch_http(&format!("{}/page", mock_server.uri()), 1000)
            .await
            .unwrap();
        assert!(result.contains("HTTP 200"));
        assert!(result.contains("hello world"));
    }

    #[tokio::test]
    async fn test_fetch_http_non_text_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/image"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "image/png")
                    .set_body_bytes("fake-image".as_bytes().to_vec()),
            )
            .mount(&mock_server)
            .await;

        let result = fetch_http(&format!("{}/image", mock_server.uri()), 1000).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_http_max_size_truncation() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/big"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("x".repeat(200))
                    .insert_header("Content-Type", "text/plain"),
            )
            .mount(&mock_server)
            .await;

        let result = fetch_http(&format!("{}/big", mock_server.uri()), 50)
            .await
            .unwrap();
        // Body should be max 50 chars
        let body = result.split("\n\n").nth(1).unwrap();
        assert_eq!(body.len(), 50);
    }

    #[tokio::test]
    async fn test_validate_url_blocks_loopback() {
        let mut max_size = 100;
        assert!(
            validate_and_clamp_url("http://127.0.0.1/secrets", &mut max_size)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_url_blocks_private_ip() {
        let mut max_size = 100;
        assert!(
            validate_and_clamp_url("http://192.168.1.1/secrets", &mut max_size)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_url_blocks_link_local() {
        let mut max_size = 100;
        assert!(
            validate_and_clamp_url("http://169.254.169.254/latest/meta-data/", &mut max_size)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_fetch_http_blocks_redirect_to_private_ip() {
        // wiremock binds to 127.0.0.1, so the redirect target must use a
        // *different* host to exercise the cross-host SSRF check. Redirecting
        // to 127.0.0.1 (same host as the mock) would hit the same-host
        // fast-path in the redirect policy and follow instead of blocking.
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/redirect"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("Location", "http://10.0.0.1/blocked"),
            )
            .mount(&mock_server)
            .await;

        let result = fetch_http(&format!("{}/redirect", mock_server.uri()), 1000).await;
        let err = result.expect_err("cross-host redirect to private IP should be blocked");
        // A policy-level rejection surfaces as a redirect error ("error
        // following redirect ..."), distinct from a transport-level
        // connection error ("error sending request ..."). This proves the
        // block fired rather than a spurious connection failure.
        assert!(
            err.contains("redirect"),
            "expected redirect-policy error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_host_from_url() {
        assert_eq!(
            host_from_url("https://example.com/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            host_from_url("http://example.com:8080/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            host_from_url("https://[::1]:8080/path"),
            Some("::1".to_string())
        );
    }

    #[test]
    fn test_html_to_markdown_conversion() {
        let result = "HTTP 200 OK\nContent-Type: text/html; charset=utf-8\n\n<html><head><script>console.log('x')</script><style>.a{}</style></head><body><h1>Hello</h1><p>World <strong>bold</strong>.</p></body></html>";
        let (header, html) = result.split_once("\n\n").unwrap();
        let md = HtmlToMarkdown::builder()
            .skip_tags(vec!["script", "style"])
            .build()
            .convert(html)
            .unwrap();
        let body = format!("{}\n\n{}", header, md);
        assert!(body.starts_with("HTTP 200 OK\nContent-Type: text/html; charset=utf-8\n\n"));
        let body_content = body.split_once("\n\n").unwrap().1;
        assert!(
            body_content.contains("# Hello"),
            "expected heading, got: {}",
            body_content
        );
        assert!(
            body_content.contains("**bold**"),
            "expected bold, got: {}",
            body_content
        );
        assert!(
            !body_content.contains("console.log"),
            "script not skipped: {}",
            body_content
        );
        assert!(
            !body_content.contains(".a{}"),
            "style not skipped: {}",
            body_content
        );
    }

    #[test]
    fn test_non_html_not_converted() {
        let result = "HTTP 200 OK\nContent-Type: application/json\n\n{\"key\":\"value\"}";
        let is_html = result
            .lines()
            .find(|l| l.starts_with("Content-Type:"))
            .map(|l| l.contains("text/html"))
            .unwrap_or(false);
        assert!(!is_html, "application/json should not be detected as HTML");
    }
}
