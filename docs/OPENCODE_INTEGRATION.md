# OpenCode integration (Codex Universal)

Codex Universal uses OpenCode as a **model source**, never as a coding harness.
Inference requests go directly from the Codex agent loop to
`https://opencode.ai/zen/v1/chat/completions` using Codex's own Chat Completions
transport.

## Free-model discovery

Two public endpoints are consulted (TTL-cached for 5 minutes; failures are silent):

1. `https://opencode.ai/zen/v1/models` — which models Zen currently serves
   (verified live; returns an OpenAI-style `{object:"list", data:[{id}]}` payload).
2. `https://models.opencode.ai/api.json` — the same metadata catalog the upstream
   OpenCode client uses. A model is **free** when `cost.input == 0 && cost.output == 0`
   there, matching how OpenCode itself gates anonymous access
   (`packages/opencode/src/provider/provider.ts`: models with non-zero cost are removed
   when the user has no key).

If the metadata catalog is unavailable, only ids ending in `-free` are marked free.
Picker entries appear as `[OpenCode] Ox Alpha Free`, etc. Free models sort first.

## Credentials

Resolution order (implemented in `codex-rs/model-provider-info/src/opencode.rs`):

1. `OPENCODE_API_KEY` environment variable.
2. The `opencode` entry of OpenCode's credential store:
   - Linux/macOS: `~/.local/share/opencode/auth.json`
   - Windows: `%USERPROFILE%\.local\share\opencode\auth.json`
     (OpenCode resolves XDG paths identically on Windows — it does *not* use
     `%LOCALAPPDATA%`; `XDG_DATA_HOME` is honored)
   - schema: `{"opencode": {"type": "api", "key": "..."}}`
3. Anonymous access with bearer token `public`, mirroring the upstream client's
   `apiKey = "public"` fallback. This works for zero-cost models; paid models return
   HTTP 401 (verified live during development).

Security properties:

- Keys are read-only; nothing is ever written back to OpenCode's store.
- Discovered keys are stored in Codex's redacted-string type and never logged,
  serialized into errors, or included in diagnostics output.
- Missing OpenCode never breaks other providers — discovery simply yields the
  anonymous fallback.

## Verified live behavior (during development)

| Test | Result |
| --- | --- |
| `GET /zen/v1/models` without auth | 200, full model list |
| `POST /zen/v1/chat/completions`, paid model, no auth | 401 (auth enforced) |
| `POST /zen/v1/chat/completions`, `x-preview-f-free`, no auth / `Bearer public` | 503 at test time (free tier capacity), i.e. auth not required but service saturated |

A 503 from a free model means temporary capacity/rate limiting, not a request-format
error. Retry later or pick another `-free` model (`mimo-v2.5-free`, `hy3-free`,
`nemotron-3-ultra-free`, ...).

## Harness boundary

The implementation never calls OpenCode session/agent APIs. Only:

- catalog discovery (public HTTP GETs above),
- credential reading (local file),

are used. Tool execution, file edits, shell commands, approvals, and the agent loop
are performed exclusively by Codex.
