use super::types::{ProviderError, StreamEvent, StreamToolCall, Usage};
use crate::message::ReasoningDetail;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::sync::mpsc;

#[derive(Debug, Deserialize)]
struct StreamError {
    code: Option<i32>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoiceDeltaReasoningDetail {
    #[serde(rename = "type")]
    rd_type: String,
    id: Option<String>,
    format: Option<String>,
    index: Option<i32>,
    text: Option<String>,
    signature: Option<String>,
    summary: Option<String>,
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoiceDeltaToolCallFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoiceDeltaToolCall {
    index: Option<i32>,
    id: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    tc_type: Option<String>,
    function: Option<StreamChunkChoiceDeltaToolCallFunction>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoiceDelta {
    content: Option<String>,
    reasoning: Option<String>,
    reasoning_content: Option<String>,
    reasoning_details: Option<Vec<StreamChunkChoiceDeltaReasoningDetail>>,
    tool_calls: Option<Vec<StreamChunkChoiceDeltaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoice {
    delta: Option<StreamChunkChoiceDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Option<Vec<StreamChunkChoice>>,
    usage: Option<Usage>,
    error: Option<StreamError>,
}

fn next_sse_lines(buffer: &mut String) -> Vec<String> {
    let mut lines = Vec::new();
    while let Some(pos) = buffer.find('\n') {
        let line = buffer[..pos].trim_end_matches('\r').to_string();
        *buffer = buffer[pos + 1..].to_string();
        lines.push(line);
    }
    lines
}

fn push_utf8_chunk(byte_buf: &mut Vec<u8>, text: &mut String, chunk: &[u8]) {
    byte_buf.extend_from_slice(chunk);
    loop {
        match std::str::from_utf8(byte_buf) {
            Ok(s) => {
                text.push_str(s);
                byte_buf.clear();
                return;
            }
            Err(e) => {
                let good = e.valid_up_to();
                if good > 0 {
                    let s = std::str::from_utf8(&byte_buf[..good]).unwrap_or("");
                    text.push_str(s);
                    byte_buf.drain(..good);
                }
                match e.error_len() {
                    None => return,
                    Some(n) => {
                        let n = n.max(1).min(byte_buf.len());
                        byte_buf.drain(..n);
                    }
                }
            }
        }
    }
}

fn merge_tool_call_deltas(
    tool_calls: &mut HashMap<i32, StreamToolCall>,
    tc_list: &[StreamChunkChoiceDeltaToolCall],
) {
    for tc in tc_list {
        let idx = tc.index.unwrap_or(0);
        let entry = tool_calls.entry(idx).or_insert_with(|| StreamToolCall {
            id: String::new(),
            name: String::new(),
            arguments: String::new(),
        });
        if let Some(ref id) = tc.id
            && !id.is_empty()
        {
            entry.id = id.clone();
        }
        if let Some(ref func) = tc.function {
            if let Some(ref name) = func.name {
                entry.name = name.clone();
            }
            if let Some(ref args) = func.arguments {
                entry.arguments.push_str(args);
            }
        }
    }
}

async fn merge_reasoning_details(
    reasoning_details: &mut HashMap<i32, ReasoningDetail>,
    reasoning_detail_order: &mut Vec<i32>,
    rd_list: &[StreamChunkChoiceDeltaReasoningDetail],
    tx: &mpsc::Sender<StreamEvent>,
) -> bool {
    let mut detail_delta = false;
    for rd in rd_list {
        let idx = if let Some(i) = rd.index {
            i
        } else {
            let mut i = 0;
            while reasoning_details.contains_key(&i) {
                i += 1;
            }
            i
        };

        let entry = reasoning_details.entry(idx).or_insert_with(|| {
            reasoning_detail_order.push(idx);
            ReasoningDetail {
                detail_type: rd.rd_type.clone(),
                id: String::new(),
                format: String::new(),
                text: String::new(),
                signature: String::new(),
                summary: String::new(),
                data: String::new(),
            }
        });

        if !rd.rd_type.is_empty() {
            entry.detail_type = rd.rd_type.clone();
        }
        if let Some(ref id) = rd.id {
            entry.id = id.clone();
        }
        if let Some(ref fmt) = rd.format {
            entry.format = fmt.clone();
        }
        if let Some(ref text) = rd.text {
            entry.text.push_str(text);
            let _ = tx
                .send(StreamEvent::ReasoningDelta {
                    delta: text.clone(),
                })
                .await;
            detail_delta = true;
        }
        if let Some(ref sig) = rd.signature {
            entry.signature = sig.clone();
        }
        if let Some(ref summary) = rd.summary {
            entry.summary.push_str(summary);
            let _ = tx
                .send(StreamEvent::ReasoningDelta {
                    delta: summary.clone(),
                })
                .await;
            detail_delta = true;
        }
        if let Some(ref d) = rd.data {
            entry.data = d.clone();
        }
    }
    detail_delta
}

async fn handle_sse_data(
    data: &str,
    tx: &mpsc::Sender<StreamEvent>,
    debug: bool,
    finish_reason: &mut String,
    tool_calls: &mut HashMap<i32, StreamToolCall>,
    reasoning_details: &mut HashMap<i32, ReasoningDetail>,
    reasoning_detail_order: &mut Vec<i32>,
) -> Result<bool, ()> {
    if data == "[DONE]" {
        if debug {
            eprintln!("stream done, pending_tool_calls: {}", tool_calls.len());
        }
        return Ok(true);
    }

    let chunk: StreamChunk = match serde_json::from_str(data) {
        Ok(c) => c,
        Err(e) => {
            if debug {
                eprintln!("stream unmarshal error: {} data: {}", e, data);
            }
            return Ok(false);
        }
    };

    if let Some(err) = &chunk.error {
        let msg = format!(
            "provider error {}: {}",
            err.code.unwrap_or(0),
            err.message.as_deref().unwrap_or("unknown")
        );
        let _ = tx
            .send(StreamEvent::Error {
                error: ProviderError {
                    status_code: 500,
                    body: msg,
                },
            })
            .await;
        return Err(());
    }

    if let Some(usage) = &chunk.usage {
        let mut usage = usage.clone();
        if let Some(ref details) = usage.prompt_tokens_details
            && usage.prompt_cache_hit_tokens == 0
        {
            usage.prompt_cache_hit_tokens = details.cached_tokens;
        }
        let _ = tx.send(StreamEvent::Usage { usage }).await;
    }

    if let Some(choices) = &chunk.choices {
        for choice in choices {
            if let Some(fr) = &choice.finish_reason
                && !fr.is_empty()
            {
                *finish_reason = fr.clone();
            }

            if let Some(delta) = &choice.delta {
                let mut detail_delta = false;

                if let Some(rd_list) = &delta.reasoning_details {
                    detail_delta = merge_reasoning_details(
                        reasoning_details,
                        reasoning_detail_order,
                        rd_list,
                        tx,
                    )
                    .await;
                }

                if !detail_delta {
                    if let Some(ref reasoning) = delta.reasoning {
                        if !reasoning.is_empty() {
                            let _ = tx
                                .send(StreamEvent::ReasoningDelta {
                                    delta: reasoning.clone(),
                                })
                                .await;
                        }
                    } else if let Some(ref rc) = delta.reasoning_content
                        && !rc.is_empty()
                    {
                        let _ = tx
                            .send(StreamEvent::ReasoningDelta { delta: rc.clone() })
                            .await;
                    }
                }

                if let Some(ref content) = delta.content
                    && !content.is_empty()
                {
                    let _ = tx
                        .send(StreamEvent::TextDelta {
                            delta: content.clone(),
                        })
                        .await;
                }

                if let Some(tc_list) = &delta.tool_calls {
                    merge_tool_call_deltas(tool_calls, tc_list);
                }
            }
        }
    }
    Ok(false)
}

async fn process_sse_line(
    line: &str,
    tx: &mpsc::Sender<StreamEvent>,
    debug: bool,
    finish_reason: &mut String,
    tool_calls: &mut HashMap<i32, StreamToolCall>,
    reasoning_details: &mut HashMap<i32, ReasoningDetail>,
    reasoning_detail_order: &mut Vec<i32>,
) -> Result<bool, ()> {
    if debug {
        eprintln!("stream line: {}", line);
    }

    if line.is_empty() {
        return Ok(false);
    }

    let Some(data) = line.strip_prefix("data: ") else {
        return Ok(false);
    };

    handle_sse_data(
        data,
        tx,
        debug,
        finish_reason,
        tool_calls,
        reasoning_details,
        reasoning_detail_order,
    )
    .await
}

pub(crate) async fn read_stream(
    resp: reqwest::Response,
    tx: mpsc::Sender<StreamEvent>,
    debug: bool,
) {
    use futures_util::StreamExt;

    let mut stream = resp.bytes_stream();
    let mut byte_buf: Vec<u8> = Vec::new();
    let mut buffer = String::new();

    let mut finish_reason = String::new();
    let mut tool_calls: HashMap<i32, StreamToolCall> = HashMap::new();
    let mut reasoning_details: HashMap<i32, ReasoningDetail> = HashMap::new();
    let mut reasoning_detail_order: Vec<i32> = Vec::new();

    loop {
        let chunk =
            match tokio::time::timeout(std::time::Duration::from_secs(60), stream.next()).await {
                Err(_) => {
                    log::warn!("stream idle timeout after 60s");
                    let _ = tx
                        .send(StreamEvent::Error {
                            error: ProviderError {
                                status_code: 0,
                                body: "stream idle timeout after 60s".to_string(),
                            },
                        })
                        .await;
                    return;
                }
                Ok(None) => break,
                Ok(Some(Err(_))) => {
                    let _ = tx
                        .send(StreamEvent::Error {
                            error: ProviderError {
                                status_code: 0,
                                body: "stream read error".to_string(),
                            },
                        })
                        .await;
                    return;
                }
                Ok(Some(Ok(c))) => c,
            };

        push_utf8_chunk(&mut byte_buf, &mut buffer, &chunk);

        let mut done = false;
        for line in next_sse_lines(&mut buffer) {
            match process_sse_line(
                &line,
                &tx,
                debug,
                &mut finish_reason,
                &mut tool_calls,
                &mut reasoning_details,
                &mut reasoning_detail_order,
            )
            .await
            {
                Ok(true) => {
                    done = true;
                    break;
                }
                Ok(false) => {}
                Err(()) => return,
            }
        }
        if done {
            break;
        }
    }

    if !buffer.trim().is_empty() {
        let rest = std::mem::take(&mut buffer);
        if let Err(()) = process_sse_line(
            rest.trim_end_matches('\r'),
            &tx,
            debug,
            &mut finish_reason,
            &mut tool_calls,
            &mut reasoning_details,
            &mut reasoning_detail_order,
        )
        .await
        {
            return;
        }
    }

    let mut tc_keys: Vec<&i32> = tool_calls.keys().collect();
    tc_keys.sort();
    for k in tc_keys {
        let tc = &tool_calls[k];
        if !tc.name.is_empty() {
            if debug {
                eprintln!(
                    "tool call name={} id={} args={}",
                    tc.name, tc.id, tc.arguments
                );
            }
            let _ = tx.send(StreamEvent::ToolCall { call: tc.clone() }).await;
        }
    }

    if !finish_reason.is_empty() {
        let _ = tx
            .send(StreamEvent::FinishReason {
                reason: finish_reason.clone(),
            })
            .await;
    }

    if !reasoning_detail_order.is_empty() {
        let mut merged: Vec<ReasoningDetail> = Vec::new();
        for idx in &reasoning_detail_order {
            if let Some(detail) = reasoning_details.get(idx) {
                merged.push(detail.clone());
            }
        }
        let _ = tx
            .send(StreamEvent::ReasoningDetails { details: merged })
            .await;
    }
}
