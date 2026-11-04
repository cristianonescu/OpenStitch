# B17. AI assistant: generate and edit embroidery by asking

Research date: 2026-11-04. The owner asked to "research how solidcraft made the AI
integrations and mirror that feature for OpenStitch ... basically the user should be able to
use it the same as it works for solidcraft in order to generate embroidery". This note:
- maps how SolidCraft's assistant works, with file and line references;
- lists what OpenStitch already has that an assistant can drive;
- records the owner's decisions;
- sets out the OpenStitch design, phased tasks with acceptance criteria, and the risks.

The owner approved the plan on 2026-11-04, adding one requirement: "each and every OpenStitch
feature is supported through the AI chat. The user should be able to control the tool
completely from there". Progress is in section 9.

## 1. The owner's decisions (2026-11-04)

| Question | Answer |
|---|---|
| Provider, model and key | **SolidCraft's full set.** The user's own key, kept in the OS keychain. Anthropic's Claude API is the built-in default with `claude-opus-5-5`. Any Anthropic- or OpenAI-compatible endpoint can be added (Ollama, LM Studio, OpenRouter and others), and so can ACP coding agents (Claude Code, Codex and others) that run under the user's own account. OpenStitch never sells, proxies or pays for a model. |
| Where the user talks to it | **A chat panel in the editor**, like SolidCraft's Assistant. `openstitch-mcp` stays as it is for external clients. |
| What is in scope | **All of it.** Text to design through the engine commands, editing the open design, explaining or fixing a design, text to image followed by digitizing, and in the owner's words "support any user request or questions". |
| Privacy and offline | **Data goes only to the provider the user chose.** Nothing goes to OpenStitch and there is no telemetry. Offline, the panel says that no provider can be reached and the editor works as usual. A local model (Ollama, LM Studio) works fully offline. |
| How it ships | **On by default, like SolidCraft.** A Preferences ▸ AI ▸ **Show AI features** master switch starts on. Nothing is detected, started or sent until the user opens the panel, sends a message or presses **Connect**. |

Being on by default does not touch PE-DESIGN compatibility. The assistant changes a design
only through the existing engine commands, so a design it makes is a design a user could
have made by hand. No compatibility spec, golden file or default changes.

## 2. How SolidCraft does it

Source: [tangency-labs/solidcraft](https://github.com/tangency-labs/solidcraft) at commit
`6bca7d2854469b279c9ce07a519ba60f462dbf56` (2026-11-04). Paths are relative to that
repository. It was read as source in a scratch clone. Nothing in it was built or run, and no
reverse engineering was needed because everything is public source.

**Licence:** dual MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`), the same as
OpenStitch. Its code may be copied if the copyright line is kept. Section 6 says what to
reuse.

### 2.1 What the user sees

The user documentation is `docs/agent-chat.md` (527 lines):

- **Panel.** The Assistant panel opens from the application bar, with Ctrl/Cmd+Shift+A, or from
  View settings (`docs/agent-chat.md:35-43`). It can dock left or right, pop out into its own
  window, or minimise to a strip (`:61-78`).
- **One master switch.** Preferences → AI → **Show AI features** is on by default. Turned off,
  SolidCraft shows nothing of the assistant and starts nothing (`:15-33`).
- **Nothing runs early.** SolidCraft looks for installed agents only when the panel or the AI
  page first opens, and starts one only on Send or Connect (`:41-43`).
- **Two kinds of "agent"** (`:188-346`):
  - an ACP coding agent found on `PATH` (Claude Code through `claude-agent-acp`, Codex,
    Antigravity, OpenCode, Grok Build, …), which signs in under the user's own account;
  - an endpoint the user adds: OpenAI-compatible (presets for Ollama, LM Studio, OpenRouter,
    OpenAI, …) or Anthropic-compatible (`{base}/v1/messages`), with a model picker filled
    from the endpoint's own list.
- **Queue, Stop, Send now and New chat** (`:155-186`). Each design tab has its own
  conversation (`:157-169`).
- **Attachments** (`:115-153`): up to eight files per message. Pictures are scaled to 2048 px.
  Text files go inline. CAD files are imported into the design. **Attach view** sends a
  picture of the viewport and **Attach selection** sends what is selected (`:395-416`).
- **One undo per message.** Everything the agent changes while answering one message is a
  single undo step named after the message, with **Undo these changes** in the chat
  (`:388-390`).
- **Permissions** (`:418-447`): *Ask every time*, *Auto-allow model edits* (the default; saving,
  opening, exporting, importing and any command given a file name still ask) or *Allow
  everything*. **Always allow** is remembered per agent and command. An optional limit caps
  the changes per message. A question nobody answers is declined after four minutes (`:449-468`).
- **Privacy** (`:470-479`): data goes only to the chosen agent or endpoint. An endpoint's key is
  kept in the keychain and only sent to that endpoint.
- **Benchmark** (`:494-525`): `solidcraft-cli bench-assistant` runs eleven fixed modelling tasks
  against a model and checks each result on the design itself.

### 2.2 How it is built

The assistant lives in `crates/ui-egui/src/agent_chat/`: 18 files and 21,018 lines, 4,599 of
them tests.

| Piece | Where | What it does |
|---|---|---|
| State and turn logic | `mod.rs` (2,563 lines): `AgentChat` :329, `connect` :963, `send` :1261, `start_turn_with` :1463, `finish_turn` :1738, `pump_tools` :1797 | One conversation per design tab, the message queue, and running each tool call through the gate. |
| Backend seam | `backend.rs:137` `trait ChatBackend`, `:171` `HttpRequest`/`HttpFn`, `:183` `trait Secrets` | The panel talks to one trait. HTTP and the keychain are injected by the app, so the crate stays testable. |
| ACP client | `acp.rs` (953 lines, hand-written JSON-RPC 2.0, `PROTOCOL_VERSION = 1` at :124), `acp_client.rs:56` `AcpBackend::start`, `process.rs:149` `AgentProcess::spawn` | Starts the agent as a child process in its own process group (`process.rs:64`; stopped with `killpg`, or `taskkill /T` on Windows). Declares no file system and no terminal to the agent (`acp.rs:145-156`). |
| MCP bridge for agents | `acp.rs:161` `BRIDGE_NAME = "solidcraft-app"`, `acp.rs:182-190` `McpBridge::to_json` | Hands the agent an MCP server, `solidcraft-cli mcp --connect 127.0.0.1:PORT`, which reaches the running app over its loopback control port with a per-launch token. The token never passes through the agent: `acp_client.rs:30` removes it from the agent's environment. |
| Claude Code's own tools off | `acp.rs:240` `CLAUDE_DISALLOWED_TOOLS` (Bash, Read, Write, Edit, Glob, Grep, WebFetch, WebSearch, Task, …), sent in `session_new_params` :245-270 | Claude Code can only touch the design through SolidCraft's tools. It runs in an empty working folder (`mod.rs:952`). |
| System prompt | `acp.rs:228` `SYSTEM_NOTE` | A short note: use the `solidcraft-app` tools, prefer `batch`, verify with `measure`/`inspect_design`, and read `skill_reference` before building more than one primitive. |
| Endpoint loop | `api_client.rs` (3,077 lines): `ApiBackend` :663, `start_mcp` :886, `turn` :906 | SolidCraft runs the tool loop itself, as a client of the same MCP server. MCP `tools/list` is converted to OpenAI tools (:1558) or Anthropic tools (:1582), capped at 128. At most 32 calls run per round, and rounds are uncapped until Stop. |
| Endpoint details | `api_client.rs:46` `ANTHROPIC_MAX_TOKENS = 8192`, :49 `anthropic-version: 2023-06-01`, `stop_reason` :1387 | **No streaming.** `max_tokens`/`length` and `refusal`/`content_filter` stop reasons are reported in the chat. The request timeout defaults to 300 s, from 30 s to an hour (`agents.rs:139-155`). |
| Keys | `agents.rs:326` `key_safe_for`; app side `apps/solidcraft/src/main.rs:674-695` (`keyring` 3.6) | A key is only ever sent over `https://` or to exactly localhost / 127.0.0.1 / [::1]. It is stored as `keyring::Entry::new("SolidCraft agent chat", id)`. HTTP uses `ureq` 3.4 (`apps/solidcraft/src/main.rs:618`). |
| Permission gate | `gate.rs` (363 lines): `Verdict` :20, `looks_like_file` :81, `names_a_file` :93, `request` :151; modes `agents.rs:12-21` | A command is classed Run, Ask or Refuse. "Given a file name" is a heuristic over the parameter names and values. The turn limit is `gate.rs:12-17` (default 0 = none, 24 when turned on). The 4-minute timeout is `mod.rs:63`. |
| One undo per message | `mod.rs:1484-1491` (records the start), `merge_undo` :1764-1781, `undo_changes` :1784 | The turn's snapshots are folded into one, labelled with the first 48 characters of the message. |
| Attachments | `attach.rs` (`MAX_FILES = 8` :26, `MAX_SIDE = 2048` :34, `MAX_SENT_BYTES` :37), `context.rs:123` `describe_selection` | Pictures are resized and re-encoded (:295-322). The selection is sent as text with ids and sizes. |
| MCP server | `crates/mcp` (7,126 lines): `tools.rs:96` `tool_definitions` | About 20 **meta tools**, not one tool per command: `list_commands`, `describe_command`, `execute`, `batch`, `inspect_design`, `measure`, `screenshot`, `compare_picture`, `undo`, `redo`, `skill_reference`, … |
| Domain knowledge | `crates/mcp/src/skill_reference.rs:11-31` | Four Agent Skills (`skills/solidcraft-modeling` and three others) are compiled in with `include_str!`. The model reads them on demand through `skill_reference`; they are not in the system prompt. |
| Benchmark | `apps/solidcraft-cli/src/bench_assistant.rs` (701 lines): `TASKS` :26, `check` :204, `classify` :278 | Results are scored on the design (volume, bounding box), not on the model's words. Failures are put in categories such as "wrote the call as text" or "only talked". |

What carries over to embroidery: one tool layer shared by every model; meta tools instead of
hundreds of tool definitions; knowledge loaded on demand; changes allowed by effect with files
behind approval; one undo per message; keys in the keychain; and scoring on the result.

## 3. What OpenStitch already has

Most of SolidCraft's plumbing already exists in OpenStitch:

| SolidCraft piece | OpenStitch today | Gap |
|---|---|---|
| Control port with per-launch token | `openstitch --listen PORT` (`crates/openstitch-remote`): 127.0.0.1 only, 32-byte token from `getrandom`, `automation.json` 0600, at most 4 connections, 1 MiB requests (docs/ARCHITECTURE.md, Automation socket) | The assistant needs the socket even when the user did not pass `--listen`. It should start a private listener of its own, with a token that is not written to `automation.json`. |
| `solidcraft-cli mcp --connect` | `openstitch-mcp --live` (`crates/openstitch-mcp/src/main.rs:19`) | Tools are one per command: about 190 `CommandSpec`s plus `run_batch` and `preview`. That is more than SolidCraft's 128-tool cap, and expensive in tokens on every request. `preview` is refused in live mode (`server.rs:430`). |
| Command registry with effects | `CommandSpec` with `read`/`edit`/`file`/`history` effects, `Session::set_capabilities`, `--read-only`, `--no-files`, `FILE_READING_COMMANDS` | None. This is better than SolidCraft's file-name heuristic: every command already **declares** whether it touches files. |
| Sandbox roots | engine `Sandbox`, with read and write roots per client (`Policy::apply`) | The assistant needs a scratch folder for SVG and image files the model writes. |
| One undo per gesture | `EditorApp::grouped` (`crates/openstitch-app/src/app.rs:520`), `undo_groups`; remote edits push one group each (`remote.rs:63`) | A turn spans many frames, so `grouped` cannot wrap it. It needs a start marker and a fold at the turn's end, as SolidCraft's `merge_undo` does. |
| Previews | `openstitch-render` (tiny-skia) for MCP `preview` and thumbnails | Needs a live-mode render of the editor's own design, so the model can look at its work. |
| Image digitizing | `digitize.autoPunch`, `photoStitch`, `crossStitch`, `designCenter`, `lineImage`, `figure` (`crates/openstitch-engine/src/digitize.rs`), with `compatMode: openStitch` | They read an image **file** under the session directory, so a generated image has to be written to the scratch folder first. |
| Vector import | `import.svg` (`crates/openstitch-engine/src/imports.rs:219`) | Also takes a file path. |
| HTTPS client | `ureq` 3 with rustls in `crates/openstitch-update` | None: reuse it. |
| Preferences | `ui.json` through `crates/openstitch-app/src/editor/prefs.rs` | Add an `ai` section. Keys never go in it. |
| Keychain | — | Add `keyring` (pure-Rust backends: `apple-native`, `windows-native`, `async-secret-service` + `crypto-rust`, as SolidCraft does). |
| Knowledge | README, docs/UI_LAYOUT.md, the command summaries and parameter docs | Write an embroidery skill: hoop sizes, density, underlay, satin widths, lettering sizes, thread charts, what PE-DESIGN-compatible mode means, and recipes. |

## 4. The OpenStitch design

### 4.1 Shape

```text
 Assistant panel (egui, openstitch-app)
   │  ChatBackend trait  (Anthropic · OpenAI-compatible · ACP)
   │
   ├─ Anthropic / OpenAI-compatible: OpenStitch runs the tool loop ──┐
   └─ ACP agent (Claude Code, Codex, …) runs its own loop ───────────┤
                                                                     ▼
                       openstitch-mcp --live --assistant   (MCP over stdio, meta tools)
                                                                     │  per-launch token, 127.0.0.1
                                                                     ▼
                  editor's private assistant listener ─► Gate (Ask / Edits / All)
                                                                     │
                                                                     ▼
                     Session::execute ─► one undo step per message (undo_groups fold)
```

Every backend reaches the design through **one** MCP server, so a tool, a permission rule or
a fix works the same for Claude, a local model and Claude Code. This is SolidCraft's split:
the API loop and the ACP agent both use the same `mcp --connect` server.

### 4.2 Tools the model sees (`openstitch-mcp --assistant`)

This is a new mode of the existing server. The per-command tools stay as they are for
`openstitch-mcp` without the flag. In assistant mode the server exposes meta tools in a fixed,
sorted order, so prompt caching holds:

| Tool | What it does |
|---|---|
| `commands_list` | Command ids, summaries and effects, filtered by a prefix or search text (from `Session::commands()`). |
| `command_describe` | One command's parameters, with the schema `openstitch-mcp` already generates. |
| `execute` | Runs one command through `Session::execute`. Failures come back as `isError` results the model can correct. |
| `run_batch` | Up to 256 steps, checked before any runs (existing). |
| `design_info` / `object_get` | The existing read commands: what is in the design, its sizes, stitch statistics and colours. |
| `view_render` | **New.** A PNG of the editor's design (Design Page plus stitches, 64–2048 px), drawn by `openstitch-render` from the live session, so the model can check its work. |
| `scratch_write` | **New.** Writes an SVG (text) or a picture (base64 PNG/JPEG, at most 16 MiB) into the conversation's scratch folder and returns its path, ready for `import.svg` or `digitize.*`. |
| `skill_reference` | **New.** Returns a bundled skill file (section 4.6). |

### 4.3 The default model and the Anthropic request

The built-in default follows the current Claude API guidance (Anthropic `claude-api`
reference, cached 2026-10-06):

- **Model `claude-opus-5-5`.** Users can pick any model the endpoint lists, as in SolidCraft's
  picker. Nothing is hard-coded beyond the default.
- **Raw HTTP to `POST {base}/v1/messages`** with `ureq`. There is no official Anthropic SDK for
  Rust, and the project already uses `ureq`. Send a current `anthropic-version`, not
  SolidCraft's `2023-06-01`.
- **Adaptive thinking** (`thinking: {type: "adaptive"}`, which is what Opus 5.5 does when the
  field is omitted). `output_config.effort` is set **explicitly**, because Opus 5.5 defaults to
  `medium`. Use `high` for generating a design and `medium` for questions, adjustable under
  Preferences ▸ AI. Opus 5.5 rejects `thinking: disabled` and `budget_tokens` with a 400.
- **Streaming (SSE)** with `max_tokens` around 64,000, so long turns never hit the HTTP
  timeout and text appears as it is written. SolidCraft does not stream and caps output at
  8,192 tokens. The tools are client tools, so set `eager_input_streaming: true` on each one.
  Every streamed tool input is then validated against its schema before it runs, and an
  input cut off by `max_tokens` is never run.
- **Stop reasons:** check `refusal` and `max_tokens` before running any tool call, and say in
  the chat which one ended the turn (as SolidCraft does at `api_client.rs:1387`). Turn on
  server-side refusal fallbacks (`fallbacks: "default"` with the
  `server-side-fallback-2026-07-01` beta) on the Claude API, and tell the user it is on.
- **No forced tool choice.** `tool_choice: any`/`tool` returns a 400 on Opus 5.5, so use
  `auto`, `strict: true` tool schemas, and the system prompt.
- **Prompt caching:** tools, then system prompt, then messages, with stable content first and
  a cache breakpoint after the system prompt. Volatile data such as the date, the selection or
  the design summary goes in the user turn, never in the system prompt.
- **Parallel tool calls:** return all results of one assistant message in one user message,
  with failures marked `is_error`.
- **Thinking blocks:** pass the model's content blocks back unchanged every turn, and keep
  history append-only (preserved thinking).

OpenAI-compatible endpoints get the same loop over `/chat/completions`. As in SolidCraft,
there is an optional `reasoning_effort` and an Ollama `tools` capability check with a warning
(`docs/agent-chat.md:325-346`).

### 4.4 Permissions

Commands are judged by their **declared effect**, not by guessing from parameter names:

| Mode | Runs without asking | Asks |
|---|---|---|
| Ask every time | `read` commands | every `edit`, `history` and `file` command |
| **Auto-allow design edits** (default) | `read`, `edit`, and `history` within the current message | `file` commands, the edit commands in `FILE_READING_COMMANDS`/`library.*` outside the scratch folder, machine transfer (`transfer.*`), and undo past the message's start |
| Allow everything | all, within the sandbox roots | — |

- **Always allow** is remembered per provider and command.
- There is an optional limit on edits per message (default none).
- A question nobody answers is declined after 4 minutes.
- **Stop** always works.
- The assistant's sandbox is read/write on the scratch folder only. The design's own folder
  and the user's roots are reached through **Ask**.
- The assistant cannot click the interface, change preferences, read crash reports, or drive
  the chat.

### 4.5 One undo per message

At the start of a turn, record `undo_groups.len()` and the message's first line (at most 48
characters). At the end of the turn, or on Stop, or when an error ends it, fold the groups
pushed since then into one, named *Assistant: "…"*. The chat then shows *N changes, one undo
step* with **Undo these changes**. Switching design tabs ends the turn first. This is the same
fold as `EditorApp::grouped`, split across frames.

### 4.6 Knowledge: embroidery skills

`skills/openstitch-embroidery/` holds Agent Skills files, compiled in with `include_str!` and
served by `skill_reference`:

- `SKILL.md`: units (mm, degrees counter-clockwise), the Design Page and hoops, how to build a
  design (shapes → sew types → colours → order), and verifying with `view_render` and
  `design_info`.
- `reference/density-and-sizes.md`: minimum lettering heights, satin widths, fill density,
  underlay and pull compensation, and what sews badly (tiny specks, long jumps, stacked needle
  drops).
- `reference/commands.md`: recipes per task: badge, monogram, name drop, appliqué, patch,
  lettering on a path, image to embroidery with each wizard and when to use which, and
  `compatMode`.
- `reference/answers.md`: for "support any user request or questions": what OpenStitch can and
  cannot do (from docs/FEATURE_MATRIX.md), file formats and machines, and where things are in
  the UI (from docs/UI_LAYOUT.md).

The system prompt stays short, as SolidCraft's `SYSTEM_NOTE` is:
- you are inside OpenStitch;
- use the `openstitch` tools only;
- prefer `run_batch`;
- read `skill_reference` before building anything;
- check the result with `view_render`;
- never claim a result you did not check.

### 4.7 The five kinds of request

1. **Text to design.** "A 70 mm round badge, navy satin border, 'ANNA' in gold" → the model
   reads the skill, then runs `run_batch` over the shape, text, sew-type and thread commands,
   then `view_render`.
2. **Edit the open design.** The user's message carries the selection (ids, kinds, sizes), as
   SolidCraft's **Attach selection** does. The edits are one undo step.
3. **Explain or fix.** `design_info` and the stitch statistics, then the skill's sewability
   rules, then fixes that need approval. The **troubleshoot** commands of B13
   (quality-tools.md) make this sharper once they exist. Until then the model judges from the
   statistics and the render.
4. **Text to image, then digitize.** There are three routes, and the model picks one per
   request:
   - *Vector art (no image model needed):* the model writes SVG → `scratch_write` →
     `import.svg` → sew types. This suits logos, clip art and monograms, and works with any
     provider, Claude included.
   - *The user's own picture:* an attached picture goes to the scratch folder → `digitize.*`
     with `compatMode: openStitch` (B16/B16.1) → `view_render`.
   - *A generated picture:* this needs an **image-generation provider**, because Claude
     produces text, not pictures. Add an optional "Image generation" endpoint (an
     OpenAI-compatible `/images/generations`, or a local server) under Preferences ▸ AI, with
     its own keychain key. The picture goes to the scratch folder, then `digitize.*`. Without
     that provider, the assistant offers the vector route and says why.
5. **Questions.** These are answered from the skill references and `commands_list`. They need
   no design changes and no approval.

### 4.8 The panel and preferences

The panel mirrors SolidCraft's: an Assistant button and Cmd/Ctrl+Shift+A; dock left or right,
or its own window; and a provider switcher with a model picker. It also has:
- Queue, Stop, Send now and New chat;
- attachments (pictures, SVG, text, stitch files imported as SolidCraft imports CAD files);
- **Attach view**;
- tool cards, and Markdown answers.

Preferences ▸ AI holds:
- **Show AI features** (on);
- the button;
- connect at launch (off) and open at launch (off);
- providers, with Test, Default, Edit and Remove;
- permissions;
- the edit limit;
- request timeout and effort;
- the image-generation endpoint.

These live in `ui.json`. Keys go in the keychain under `OpenStitch assistant`, and are only
ever sent over `https://` or to exactly localhost / 127.0.0.1 / [::1] (SolidCraft's
`key_safe_for` rule).

### 4.9 Determinism

The model is not deterministic, but the result it leaves is a list of engine commands, and
those are. Every turn keeps its tool calls, so **Save as script** writes them as a
`stitchtool exec` JSON script that rebuilds the same design byte for byte on every OS. No
engine, generator or golden test changes. Tests use a scripted test backend (as SolidCraft's
`acp_test_agent` does), never a live model.

## 5. Phased tasks

Each phase is its own PR batch. Docs (README, FEATURE_MATRIX, ROADMAP and this file) are
updated in every PR.

| Phase | Work | Acceptance criteria |
|---|---|---|
| **A0 Tool surface** | `openstitch-mcp --assistant` with the meta tools; `view_render` in live mode; `scratch_write`; `skill_reference`; the editor's private assistant listener; the first `skills/openstitch-embroidery` files | `tools/list` returns at most 12 tools, in sorted order, and stays the same between runs. `view_render` returns the editor's design. `scratch_write` refuses paths outside the scratch folder, `..` components and sizes over the limit. The existing MCP tests and `live.rs` stay green, and the new tests run on all three OSes. |
| **A1 Claude in the panel** | `ChatBackend` trait; Anthropic backend (streaming, adaptive thinking, explicit effort, eager input streaming with validation, refusal and `max_tokens` handling, fallbacks, caching); panel MVP; Preferences ▸ AI; `keyring`; permission gate; one undo per message; Save as script | With a scripted backend, the tests show: a three-step turn becomes one undo step; a `file` command waits for approval and is declined after 4 minutes; Stop ends the turn and keeps its changes undoable; a truncated tool input is not run; a key is refused for a plain `http://` non-loopback URL; Save as script replays to the same `.osd`. A manual run with a real key makes the README badge example from one sentence. |
| **A2 Endpoints and local models** | OpenAI-compatible backend, presets, the model list, the Ollama tools warning, request timeout | A scripted OpenAI-compatible server passes the same suite. With a local Ollama model and the network off, a simple badge is made. |
| **A3 ACP agents** | Discovery (`--version` on `PATH` and the usual folders), spawning in a process group, the MCP bridge, Claude Code's own tools turned off, sign-in prompts | A scripted ACP test agent makes and edits a design through the bridge. Killing the panel kills the agent's process tree. Claude Code cannot run a shell or read files (checked from the `session/new` params). |
| **A4 Pictures and digitizing** | Attachments (8 files, 2048 px), Attach view, Attach selection, the vector route, digitizing an attached picture, the optional image-generation endpoint | An attached PNG becomes an Auto Punch design in OpenStitch mode in one turn. An SVG written by the model is imported as shapes. With no image provider the assistant offers the vector route. |
| **A5 Benchmark and docs** | `stitchtool bench-assistant`: about 12 embroidery tasks scored on `design_info` (sizes, colours, object kinds, stitch count ranges); README section; packaging check | The benchmark runs against a scripted endpoint in CI (no network), and against real models by hand with a Markdown report. The README documents setup, privacy and permissions. |

## 6. Reuse from SolidCraft and reimplementation

The licences are compatible (both MIT OR Apache-2.0). A file adapted from SolidCraft keeps a
header line, "Adapted from SolidCraft (github.com/tangency-labs/solidcraft), Copyright (c)
2026 SolidCraft contributors, MIT OR Apache-2.0", and gets a line in NOTICE and
docs/LEGAL_AND_LICENSING.md. None of the agent-chat files are among the third-party
adaptations in SolidCraft's own `NOTICE`.

| Adapt (protocol code that has nothing to do with CAD) | Reimplement (tied to SolidCraft's document and UI) |
|---|---|
| `acp.rs` message builders and `PROTOCOL_VERSION` handling | `mod.rs` turn logic (OpenStitch's `Session`, `undo_groups` and frames) |
| `process.rs` process-group spawning and stopping | `gate.rs` (OpenStitch judges by declared effects instead) |
| `discover.rs` `KNOWN` agents and search folders | `panel.rs`/`settings.rs` (OpenStitch's ribbon, panes and theme) |
| `agents.rs` `key_safe_for`, the timeout range | the MCP tool layer (OpenStitch's `openstitch-mcp` and registry) |
| `attach.rs` picture limits and re-encoding (OpenStitch must use its own `png`/`image` stack) | `bench_assistant.rs` (embroidery tasks and checks) |
| `api_client.rs` OpenAI-compatible request and response mapping (not its Anthropic path, which does not stream) | the system prompt and skills (embroidery knowledge) |

## 7. Risks

| Risk | Mitigation |
|---|---|
| **Cost.** Each turn resends the tools and the history, and previews are images. | The user pays their provider, and OpenStitch pays nothing. The meta tools keep the tool block small and cacheable, and previews default to 800 px. The panel shows the tokens used per turn when the provider reports usage. There is an optional edit limit per message, and effort and model can be chosen. |
| **Key storage.** | Keys are kept only in the OS keychain and never in `ui.json`, logs or crash reports. They are only sent over HTTPS or loopback. A stored key is never shown again. The keychain is not available in the Flatpak sandbox without portal access, so say so in the panel. |
| **Commands the model runs.** | The sandbox has the scratch folder only. File and transfer commands ask. Engine `Params` validation stays the boundary, and `bad_parameters_fail_cleanly` covers it. The `--listen` socket's capabilities apply per request. Claude Code's own tools are off. The token stays out of the agent's environment. Text the model writes that only looks like a call is never run. |
| **Prompt injection** from an attached file or SVG. | File content is data for the model, not an instruction to OpenStitch. Nothing a file says can widen permissions, and every file and transfer action still goes through the gate. |
| **Determinism and compatibility.** | The model's output varies, but the commands it leaves do not, and Save as script records them. No generator, default or golden file changes, and the PE-DESIGN reference specs are untouched. |
| **On by default** (the owner's choice). | Nothing is detected, started or sent until the panel is used. The master switch hides everything. `--listen`, `--screenshot` and scripted runs never start the assistant. |
| **Quality of designs.** | Models do not know sewability. The skills carry the rules, `view_render` lets the model check its work, and A5's benchmark measures it. Results must be checked in a sew-out before README claims are made. |
| **Image generation** depends on a second provider. | It is optional. The vector route and digitizing an attached picture need no image model. |
| **Licensing of generated artwork.** | The user owns their prompt and its results. The README says that trademarks and copyrighted characters are the user's responsibility. |
| **Size.** SolidCraft's version is about 21,000 lines. | The work is phased, and A1 is useful alone. Socket, sandbox, registry, MCP and rendering are already done, which is about a third of SolidCraft's work. |

## 8. What is not established yet

- **Model quality.** How well each model builds embroidery through these tools has not been
  measured. A5's benchmark measures it. Until then, no claim is made.
- **Token cost per design.** No turn has been run. It will be measured in A1 with the real
  API and recorded here.
- **Keychain support in the Flatpak and in the portable Windows zip** has not been tested.
- **Which image-generation provider** to recommend. None was tested, and it is optional.
- **Behaviour of each ACP agent.** Only SolidCraft's documentation of it was read. The current
  `claude-agent-acp` options must be checked against its own release notes in A3.
- **Facts taken from SolidCraft** in section 2 are as of commit `6bca7d2`, and its code moves
  fast.

## 9. Progress

### A0, the tool surface (landed 2026-11-04)

Complete control (the owner's requirement) is built and tested as follows.

**Design changes.** Every design change is an engine command. The assistant reaches all of
them through `commands_list`, `command_describe`, `execute` and `run_batch`. These read the
editor's own `commands.list`, so a new engine command is offered with no assistant code.

**The window.** Everything the window holds has an editor command on the automation socket
(docs/ARCHITECTURE.md, "Assistant commands"):
- `ui.state`, `ui.actions` and `ui.run`. `ui.run` covers every ribbon, menu and Quick Access
  Toolbar command.
- `ui.select` and `ui.zoom`.
- `ui.settings` and `ui.set`, with 25 settings: view, grid, rulers, guides, panes, theme,
  splash, Quick Access Toolbar position, update checks, realistic view, font filter, optimise
  entry/exit, simulator speed.
- `ui.render`, `ui.simulate`, `ui.help` and `ui.update`.
- `ui.scratchWrite`, which writes into a per-launch scratch folder that becomes the socket's
  extra write root.

**Routes.** `editor::assistant::route` gives each of the 153 `Action`s a way in:
- `ui.run`;
- a drawing tool plus its engine command;
- the engine command with a path, in place of a native file dialog;
- the command behind a list of choices.

Dialogs are reached two ways. `ui.run` opens one for the user. The engine command behind the
dialog sets its values.

**Coverage tests.**
- `every_editor_feature_is_reachable_from_the_assistant` fails when an engine command, an
  action's engine or route command, or an Options or Help menu entry has no assistant route.
  The menu entries are found by a scan of `ribbon.rs`.
- `every_run_action_runs_through_ui_run` runs every runnable action with four selections.
  `ui.run` must succeed exactly when the button is enabled.

**MCP and skills.**
- `openstitch-mcp --assistant` offers seven meta tools. Their order is fixed and their bytes
  are identical across runs, so prompt caching holds. Their schemas are strict. A
  `view_render` image block replaces `preview`, which is refused in live mode.
- `skills/openstitch-embroidery` holds `SKILL.md` and three references. A test checks that
  every command id they name exists.

**Not done in A0.**
- One undo step per message. It needs the turn boundary that the A1 panel owns. Until then
  each socket edit is its own undo step, as before.
- The permission gate (Ask, Edits, All) is A1. A socket client keeps the
  `--listen-read-only` and `--listen-no-files` capabilities and the file roots.
- No real model has driven these tools yet. That comes with A1 and A5.

### A1, the Assistant panel (landed 2026-11-04, #306)

**What the user sees.** View ▸ Show/Hide ▸ **Assistant** (Ctrl/Cmd+Shift+A) opens a chat on the right. The settings (⚙) hold:
- the address (the Claude API by default);
- the model (`claude-opus-5-5`);
- the effort;
- the permissions and the change limit;
- the API key, which is saved in the system keychain and never shown again.

Options ▸ **Show AI features** is on by default. While it is off, the panel, its button and the shortcut are gone.

**How a request runs.** As planned (section 4):
- `openstitch-assistant` streams from the Messages API, sending `anthropic-version` 2026-10-21. The request uses adaptive thinking with a summarised display, explicit effort, `max_tokens` 64000, and `eager_input_streaming` tool inputs that are validated before they run. Caching breakpoints sit on the tools, the system prompt and the conversation. On api.anthropic.com it also sends `fallbacks: "default"`.
- The tools are the A0 assistant server's, driven in-process.
- Every command passes the effect-based gate.
- Each message is one undo step.
- **Save as script** writes the engine edits as a `stitchtool exec` script, and a test checks that the script rebuilds the same objects.

**Choices made while building.**
- The editor's context (file name, object count, selection, scratch folder) goes in each user message, not the system prompt, so the prompt cache holds.
- The API key is stored per address, so a local endpoint and the Claude API keep separate keys.
- The undo step has no name: the editor's undo history has no labels yet. The chat shows "N changes, one undo step" with **Undo these changes**.

**Not done in A1.**
- No turn has run against the real API in CI, since that needs a key. The owner's first run with their key is the live check, and its token use goes into this section.
- Pictures and attachments are A4.
- Other endpoints and local models are A2. The address field already takes an Anthropic-compatible URL.
- The Assistant command's own icon is #313.

### A2, other endpoints and local models (landed 2026-11-04, #317)

**Protocols.** `openstitch-assistant` speaks a second protocol, OpenAI-compatible
`/chat/completions`:
- The reply is streamed, and `delta.tool_calls` are rebuilt across chunks and checked to be
  JSON objects before they run.
- Tool results go back as `role: tool` messages. Those take text only, so a `view_render`
  image becomes a short note.
- `length` and `content_filter` are handled like `max_tokens` and `refusal`.
- No `temperature` is sent, and no token limit either: Ollama documents only `max_tokens`,
  and OpenAI has deprecated it.
- A request with no `Authorization` header goes only to this computer.

Each protocol detail is cited from the OpenAI reference, the Ollama docs and the Ollama source
in `src/openai.rs` and `src/models.rs`.

**Settings.**
- The endpoint presets are Claude API, Ollama (`http://localhost:11434/v1`), LM Studio
  (`http://localhost:1234/v1`) and OpenRouter. Any other address works too.
- The protocol can be chosen, and **List** fills a model picker from the endpoint
  (`/v1/models` or `/models`).
- The request timeout is 30-3600 s. The reasoning effort is optional and is sent only when set.
- Ollama's `/api/show` capabilities give a warning when a model can't call tools.
- The keychain is read once per address, not every frame.

**Conversations.**
- A conversation keeps the protocol it started with, so changing the protocol starts a new one,
  and the chat says so.
- When a model answers an edit request with text only, a notice points to models with tool
  calling. This is a conservative check of the English opening words, as SolidCraft's is.

**Not verified.**
- No run against a real Ollama, LM Studio or OpenRouter server. The tests use scripted streams
  shaped from the documentation.
- The `data:` URL form of `image_url` and in-stream error objects are handled defensively,
  because the docs don't show them.

### A3, coding agents (landed 2026-11-04, #338)

**The user's side.** In the settings, Protocol ▸ **Coding agent** offers the agents found on this
computer: Claude Code (`claude-agent-acp`), Codex (`codex-acp`), OpenCode (`opencode acp`) and the
others SolidCraft lists, each with its version. A missing agent shows its install command with
**Copy**. An agent that needs signing in says how; the panel never asks for an agent's key.

**How it works.**
- `openstitch-assistant::acp` (adapted from SolidCraft, with attribution) starts the agent in its
  own process group in an empty folder.
  - It speaks ACP v1 (newline-delimited JSON-RPC on stdio), checked against agentclientprotocol.com.
  - It declares no file system and no terminal, and sends Claude Code's `disallowedTools`.
  - It maps `session/update` to the panel's events.
  - It allows permission requests for the bridge's tools once each and refuses any other tool.
- The agent's only tool server is the editor's own executable in a windowless mode,
  `openstitch --assistant-mcp <file>`, the same server as `openstitch-mcp --assistant` through a
  shared stdio loop. The packages ship `openstitch-mcp` only in the CLI archive, so the editor
  can't count on it. This mode was checked against a running editor: initialize, the seven
  tools, a rectangle drawn, `ui.state`, a clean exit. The endpoint file is in
  the scratch folder (0700 on Unix) and points at a private loopback listener the panel opens with
  a per-launch token. The token never goes in the agent's environment.
- Every request on that listener passes the same gate and the same undo bookkeeping as the
  in-process path. `the_agent_socket_goes_through_the_gate` shows an edit running, a file command
  waiting for approval and its denial reaching the client, and a wrong token refused.

**Not verified.**
- No real agent has been run; the tests use a scripted ACP agent.
- The Claude Code `_meta` option keys and the exact error `session/new` returns when sign-in is
  needed come from SolidCraft's code, not from the ACP spec.
- Killing an agent's process tree on Windows (`taskkill /T`) is not exercised; the
  process-group tests are Unix-only.

