# Architecture: Codex Universal

## Request path

```text
Codex Desktop (closed source, unmodified)
    │  spawns CODEX_CLI_PATH with `app-server`
    ▼
Codex Universal app-server (JSON-RPC over stdio)
    │
    ▼
Codex agent loop (core)          ← unchanged harness: tools, approvals, history
    │
    ▼
ModelClientSession::stream()                     core/src/client.rs
    │   match provider.info().wire_api {
    ├── WireApi::Responses ──▶ ResponsesClient      codex-api/src/endpoint/responses.rs
    │                            └─ OpenAI / OpenRouter /api/v1/responses
    └── WireApi::Chat ───────▶ ChatCompletionsClient codex-api/src/endpoint/chat.rs
                                 └─ OpenCode Zen /chat/completions, generic providers
```

## Components added by the fork

| Component | Location | Purpose |
| --- | --- | --- |
| Chat request builder | `codex-api/src/requests/chat.rs` | Translates the internal Responses-shaped prompt (`instructions`, `ResponseItem` history, tool definitions) into an OpenAI-compatible `/chat/completions` body. Groups consecutive tool calls into one assistant message with `tool_calls: [...]`; maps outputs to role-`tool` messages. |
| Chat SSE parser | `codex-api/src/sse/chat.rs` | Converts `chat.completion.chunk` events into the same internal `ResponseEvent` stream used by Responses: text deltas, reasoning deltas (`reasoning`/`reasoning_content`), incremental tool-call arguments merged by index/id, finish reasons (`stop`/`tool_calls`/`length`), `[DONE]` sentinel, usage extraction, provider error payloads. |
| Chat endpoint client | `codex-api/src/endpoint/chat.rs` | Mirrors `ResponsesClient`: auth provider, retries, idle timeout, telemetry. Fixed route `chat/completions`. |
| `WireApi::Chat` | `model-provider-info/src/lib.rs` | Serde variants `chat` and alias `chat_completions`. |
| Core dispatch arm | `core/src/client.rs::stream_chat_completions` | Builds the same Responses-shaped request (so shared formatting/cleanup applies), then converts it to chat format; reuses the unauthorized-retry loop and stream mapping. |
| Built-in providers | `model-provider-info/src/lib.rs` | `openrouter` (Responses wire), `opencode` (Chat wire). Both are fully overridable via `config.toml` (fork change to `merge_configured_model_providers`). |
| Credential discovery | `model-provider-info/src/opencode.rs` | Read-only lookup of `OPENCODE_API_KEY` → OpenCode `auth.json` → anonymous `public` bearer. Result stored in a redacted bearer-token field. |
| Composite identities | `model-provider-info/src/universal.rs` | `<provider>::<model>` encode/split with strict provider-key validation. |
| Config decode | `core/src/config/mod.rs` | Decodes composite identities before provider resolution and strips the prefix from the effective model slug. Applies identically on thread start/turn/resume because everything flows through config resolution. |
| Dynamic catalogs | `universal-catalog/` crate | TTL-cached fetch+parse of OpenRouter `/api/v1/models` and OpenCode Zen `/models` + models.opencode.ai metadata (free = cost 0). Network failures contribute zero entries. |
| Desktop picker entries | `app-server/src/models.rs` | Appends `[Provider] Model` entries to `model/list` responses without touching the protocol shape. |

## Capability handling

The chat conversion only emits fields every OpenAI-compatible server accepts:
`model`, `messages`, optional `tools`, `parallel_tool_calls` (only when tools exist),
`stream`, `stream_options.include_usage`.

Deliberately **omitted** (unsupported by many third-party models): `reasoning`,
`include`, `store`, `text` controls, service tiers, prompt cache keys, WebSocket
transports (`supports_websockets = false` for fork built-ins). Reasoning *output*
is parsed when providers emit `delta.reasoning` / `delta.reasoning_content`, but no
reasoning parameters are ever requested unless a user adds them through custom headers.

## What was deliberately NOT done

- No proxy process, no LiteLLM, no Python wrapper. The binary is Codex.
- OpenCode is never used as a harness — only its public catalog endpoints and its
  on-disk credential store are consulted (read-only).
- The closed-source Desktop renderer is not patched; integration uses the supported
  `CODEX_CLI_PATH` override.

## Upstream base

Forked at upstream commit `0280d73c1e86365847c83dfe719f7eeba1c8f8a8`
(branch `universal-providers`). Fork changes are isolated to the components listed
above to keep future rebases small.

## Verified end-to-end (live, OpenCode Ox Alpha Free via Chat Completions)

1. `codex --version` → `codex-cli 0.0.0 (codex-universal)`
2. `codex exec --model opencode::x-preview-f-free` basic inference → `OX_ALPHA_OK`
3. Multi-step coding task (`exec`, workspace-write sandbox): model created
   `primes.py` + `test_primes.py`, Codex executed `python -m pytest`
   (12 passed), model summarized changes. Trace shows shell execution by Codex.
4. app-server protocol (Desktop's transport):
   - `model/list` → 482 universal entries (`[OpenCode] ...`, `[OpenRouter] ...`)
   - `thread/start { model: "opencode::x-preview-f-free" }` ✓
   - `turn/start` with a tool-using prompt → reasoning stream → **tool call executed
     by Codex** (PowerShell `echo E2E_OK`) → agent message → `turn/completed` ✓

