# Universal providers: configuration reference

All examples live under `~/.codex/config.toml`. Credentials belong in environment
variables — never in `config.toml`.

## 1. OpenAI (upstream, unchanged)

```toml
model = "gpt-5.3-codex"
```

Authenticate with ChatGPT login or `OPENAI_API_KEY`, exactly as upstream.

## 2. OpenRouter via the Responses API (default)

The built-in `openrouter` provider already does this; just set the key:

```powershell
[Environment]::SetEnvironmentVariable("OPENROUTER_API_KEY", "sk-or-...", "User")
```

Then pick any model from the Desktop picker (`[OpenRouter] ...`) or set:

```toml
model = "openrouter::deepseek/deepseek-r1"
```

OpenRouter model ids keep their `author/model` form after the `provider::` prefix.

## 3. OpenRouter via Chat Completions

Some models misbehave over Responses. Override the built-in provider:

```toml
[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
wire_api = "chat"
```

Optional attribution headers are supported through normal header config:

```toml
[model_providers.openrouter.http_headers]
"HTTP-Referer" = "https://your-site.example"
"X-Title" = "Codex Universal"
```

## 4. OpenCode Zen / Ox Alpha Free

Nothing to configure. If you have a key:

```powershell
[Environment]::SetEnvironmentVariable("OPENCODE_API_KEY", "...", "User")
```

Otherwise Codex Universal reuses OpenCode's own `auth.json` if present, and finally
falls back to anonymous access, which OpenCode Zen accepts for free models.

```toml
model = "opencode::x-preview-f-free"
```

See `docs/OPENCODE_INTEGRATION.md`.

## 5. Arbitrary OpenAI-compatible provider (Responses)

```toml
[model_providers.example]
name = "Example"
base_url = "https://api.example.com/v1"
env_key = "EXAMPLE_API_KEY"
wire_api = "responses"

# optional:
[model_providers.example.query_params]
"api-version" = "2025-01-01"
```

## 6. Arbitrary OpenAI-compatible provider (Chat Completions)

```toml
[model_providers.example]
name = "Example"
base_url = "https://api.example.com/v1"
env_key = "EXAMPLE_API_KEY"
wire_api = "chat"                # alias: "chat_completions"
```

Select it with either `model_provider = "example"` plus `model = "..."`,
or the composite form `model = "example::their-model-id"`.

## 7. Ollama / LM Studio (regression)

Upstream built-ins remain available with their Responses defaults:

```toml
model_provider = "ollama"        # http://localhost:11434/v1
# or
model_provider = "lmstudio"      # http://localhost:1234/v1
```

To force chat wire against a local server, define a custom provider pointing at it.

## Profiles

Standard upstream `[profiles.*]` work; each profile may pin its own
`model`, `model_provider`, and provider table entries.

## Notes & limits

- `wire_api = "chat"` sends only universally supported fields; reasoning params,
  verbosity, service tiers, and prompt-cache keys are omitted on this transport.
- Providers configured with `wire_api = "responses"` must actually implement the
  Responses API; otherwise use `chat`.
- Remote (cloud-managed) thread configs cannot express the chat wire API; local
  `config.toml` is the supported way.
