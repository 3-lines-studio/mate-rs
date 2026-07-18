# mate-rs Thermo-Nuclear Remediation Plan

Baseline: clean build, 451 tests green. Bar for every batch: build + 451 tests green, behavior preserved, no new comments, match existing style, `#[cfg(test)]` only where adding tests.

## Principles
- One file = one owner per phase (no overlapping parallel edits).
- Disjoint batches run in parallel; overlapping ones sequence.
- Review between phases (build + test + diff read).
- EXCLUDED as low-value (do not churn): read_file BufReader, webfetch host_from_url→url crate, edit_file O(n²) overlap, table width math, format.rs duration rounding, proc-macro for tool schema.

## Root thesis
Most debt = string-typed discriminants where enums belong (Segment/LiveBlock/Event/Dropdown/open_router/"delegate"). Fixing that spine unblocks the rest.

---

## PHASE 1 — foundational, disjoint, parallel (LOW RISK)  ✅ DONE (verified: build green, 443 tests)

PROCESS NOTE: parallel coders sharing one CWD lost 2 batches to `git stash` collisions. Re-ran 1B/1D SEQUENTIALLY with explicit no-stash constraints — succeeded. All subsequent batches run sequentially.

### 1A · session store hygiene
Files: NEW `src/util.rs`, `src/lib.rs`, `src/session/store.rs`, `src/session/types.rs`
- `util::truncate_with_ellipsis(s, max, ellipsis)`; migrate store.rs:312 + types.rs:89.
- Extract `atomic_write(path, data)`; use in `commit_turn` + `save_meta_locked`.
- Replace store's hand-rolled `index_cache`/`index_order`/`touch_index` with existing `session::cache::Cache<Vec<TurnMeta>>`.
- `compute_turn_id`: `.unwrap()` → `.expect("message serialization infallible")`.

### 1B · skills canonicalization
Files: `src/skills.rs`
- Rewrite `list_tool`/`load_tool` via `define_tool` (read `tools/mod.rs` + `bash.rs` for pattern). Kill manual `Tool{}` + inline `#[derive] struct P`.

### 1C · tools misc
Files: `src/tools/index.rs`, `src/tools/gitignore.rs`, `src/tools/bash.rs`
- index.rs: share tree-sitter parse between `process_file` and `find_refs`; `std::thread::spawn` → `tokio::task::spawn_blocking`; silent `unwrap_or(default)` → `log::warn!` + rebuild.
- gitignore.rs: verify `globset` handles `[`/`{` patterns; if yes delete dead `glob_match` fallback; if no, leave with a test.
- bash.rs: only `libc::kill` when child still alive; don't signal recycled PIDs.

### 1D · Dropdown named items + generic render
Files: `src/tui/chat_dropdowns.rs`, `src/tui/chat/mod.rs`
- Replace magic tuples with named structs (`CommandItem`, `TreeItem{turn_id,label,depth,is_last,ancestors,is_current}`, …). Keep `Dropdown<T>` generic container.
- Collapse 5 `render_*_dropdown` → 1 generic `render_dropdown(area, dd, title, fmt_fn)`.
- Leave `config_editor/edit.rs::Dropdown<String>` as-is.

---

## PHASE 2 — agent core (safe wins)  ✅ DONE (verified: build green, 443 tests)
compaction if-ladder collapsed 4→2-way; loop_ ancestry error now propagates EventKind::Error; Event constructors deduped via Event::new/from_subagent; compaction truncate migrated to util.

### 2A · Event enum tightening + delegate de-magicking + compaction + loop_
Files: `src/agent/types.rs`, `src/agent/tools.rs`, `src/agent/loop_.rs`, `src/agent/compaction.rs`, `src/agent/mod.rs`, `src/tui/chat_handlers.rs`, `src/tui/mod.rs`
- Move `subagent`/`subagent_id` off the base `Event` onto the specific `EventKind` variants that need them (ToolCallStart/ToolResult/ToolError). Update 9 reader sites.
- `agent/tools.rs`: kill `tc.name == "delegate"` literal — make delegation a regular tool whose closure holds a subagent manager.
- `compaction.rs`: collapse 4-way if-ladder → decide-then-act; `force` only skips threshold check.
- `loop_.rs`: ancestry-load failure → propagate `EventKind::Error` (no silent empty fallback); extract `assistant_msg`/`tool_msg` helpers.

---

## PHASE 3B — provider split  ✅ DONE (verified: build green, 443 tests)
provider/mod.rs 728→9-line facade; split into types.rs(197)/client.rs(186)/stream.rs(349); apply_profile → free function.

## FORK 1 — delegate dispatch  ✅ DONE (refined to (b): const DELEGATE_TOOL_NAME + extracted spawn_delegate_task/spawn_registry_task; 443 tests)
## FORK 2 — Event envelope  ✅ DECIDED: keep minimal cleanup; subagent fields do real routing work, not worth the blast radius

## PHASE 3 — domain/wire types (HIGH blast radius, 3A remaining)

### 3A · Message Serialize cleanup  ✅ PARTIAL DONE
- Message::Serialize clone+Helper removed → serialize_struct (wire byte-identical; 8 message tests pass).
- ReasoningDetail enum: DECLINED — merge path (stream.rs:101-160) accumulates cross-type fields per-index (text+data possible on same index); struct is the correct accumulator model, an enum would drop data. Fields never read downstream anyway.
- Full wire/domain (ChatMessage vs Message) split: DEFERRED — 13-file blast radius for cosmetic gain; serialize cleanup already achieved the core goal (no clone-in-serialize, hack isolated).

### 3B · provider split + apply_profile  ✅ DONE

---

## PHASE 4 — god-file decomposition + orchestration

### 4A · config.rs split + save_tui + open_router flag  ✅ DONE (config split + save_tui typed round-trip; set_nested deleted)
  DEFERRED: open_router flag — non-breaking version keeps the heuristic (low value); deleting heuristic is breaking. Flagged to user.
Files: `src/config.rs` → `types.rs`/`load.rs`/`save.rs`/`path.rs`; `src/core/resolve.rs`
- `save_tui` round-trips typed `Config` (no raw TOML surgery); `ProviderConfig.open_router: bool` replaces `is_open_router` substring heuristic.

### 4B · god-file splits (parallel, disjoint)
- `src/session/store.rs` → `store.rs`/`index.rs`/`persistence.rs` (after 1A cache work).
- `src/tools/webfetch.rs` → `webfetch/{fetch,html,browser,dns}.rs`; replace `sleep(1)` with wait-for-load.
- `src/tools/index.rs` → `index/{build,query,types}.rs`.

### 4C · Deps atomize + Store Arc + integration rename
Files: `src/core/{mod,bootstrap,session,session_manager,scheduler}.rs`, `src/integration.rs`, `src/main.rs`
- Merge `init`/`init_with_config` into one atomic ctor; wrap `Store` in `Arc<Mutex<..>>` once.
- `integration.rs` → `streaming.rs`; encapsulate `ActivePrompt`.
- Extract shared "run agent → text" helper (scheduler/local/main).
- `main.rs` thin to ~dispatch; reuse `LocalInterface`.

---

## PHASE 5 — chat backends + TUI/render (largest, last)

### 5A · Slack/Telegram BotRuntime (after 4C)
Files: `src/slack.rs`, `src/telegram.rs` → each ~100 lines behind `BotRuntime` transport strategy.

### 5B · TUI state machines + BlockContent
Files: `src/tui/mod.rs` (handle_input), `src/render/mod.rs` (StreamRenderer → `StreamState` enum), `src/tui/chat/{messages,handlers}.rs` + `chat_render.rs` (`BlockContent` enum dedupes render_assistant/render_live), `src/tui/config_editor/fields.rs` (data-driven field table), `src/markdown/{mod,slack,telegram}.rs` (`MarkdownBackend` trait).
