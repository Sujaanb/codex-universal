# Codex Universal

A fork of [openai/codex](https://github.com/openai/codex) that lets the **official Codex
Desktop application** (and the CLI) use **arbitrary LLM providers** — OpenRouter, OpenCode
Zen free models, local servers, and any OpenAI-compatible endpoint — while **Codex itself
remains the coding harness** (agent loop, tools, file edits, shell, approvals).

```text
Official Codex Desktop  ──CODEX_CLI_PATH──▶  codex-universal binary
                                                │
                                             app-server + agent loop (unchanged)
                                                │
                                    ┌───────────┴────────────┐
                                Responses API           Chat Completions API
                                (OpenAI, OpenRouter)    (OpenCode Zen, most providers)
```

Based on upstream commit `0280d73c1e86365847c83dfe719f7eeba1c8f8a8`.

## Quick start (Windows)

```powershell
git clone <this-fork> codex-universal
cd codex-universal

.\scripts\build-windows.ps1          # builds codex-rs\target\release\codex.exe
.\scripts\install-desktop-backend.ps1
```

Then fully restart Codex Desktop (`taskkill /IM Codex.exe /F`, relaunch).
To undo: `.\scripts\uninstall-desktop-backend.ps1`.

## Configuration

Set credentials as environment variables (never in config.toml):

| Variable | Provider |
| --- | --- |
| `OPENROUTER_API_KEY` | OpenRouter (`openrouter`) |
| `OPENCODE_API_KEY` *(optional)* | OpenCode Zen (`opencode`); free models work without it |

Pick a model either via the Desktop model picker (entries like
`[OpenRouter] DeepSeek R1`, `[OpenCode] Ox Alpha Free`) or directly:

```toml
# ~/.codex/config.toml
model = "opencode::x-preview-f-free"        # composite provider::model identity
```

or with an explicit custom provider:

```toml
[model_providers.my_provider]
name = "My Provider"
base_url = "https://example.com/v1"
env_key = "MY_PROVIDER_API_KEY"
wire_api = "chat"        # or "responses"
```

See `docs/` for details:

- [`docs/ARCHITECTURE_UNIVERSAL.md`](docs/ARCHITECTURE_UNIVERSAL.md) — how it works
- [`docs/DESKTOP_SETUP.md`](docs/DESKTOP_SETUP.md) — Desktop integration & troubleshooting
- [`docs/UNIVERSAL_PROVIDERS.md`](docs/UNIVERSAL_PROVIDERS.md) — all provider options & examples
- [`docs/OPENCODE_INTEGRATION.md`](docs/OPENCODE_INTEGRATION.md) — free-model discovery, credentials

## What changed vs upstream

- Restored the Chat Completions wire transport (`wire_api = "chat"`, alias `"chat_completions"`),
  ported from upstream history and adapted to the current architecture.
- Built-in `openrouter` and `opencode` providers; both fully overridable from `config.toml`.
- Safe read-only discovery of OpenCode's credential store for anonymous/free access.
- Composite `<provider>::<model>` identities so the unmodified Desktop picker can select
  models across providers; decoded during config resolution so threads/resumes keep the
  right provider.
- Dynamic model catalogs (TTL-cached, failure-tolerant) surfaced through the existing
  `model/list` app-server method.

Everything else — sandboxing, approvals, MCP, skills, the Responses transport for
OpenAI/OpenAI-compatible endpoints — is untouched upstream behavior.

## License

Apache-2.0, matching upstream.
