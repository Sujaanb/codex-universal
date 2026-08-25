# Desktop setup (Windows first-class)

## How the official Codex Desktop picks its backend

Codex Desktop launches the app-server through a codex binary resolved in this order:
the user-level `CODEX_CLI_PATH` environment variable, then its bundled binary. This
is a supported mechanism (see openai/codex issues #22672, #13709, #27389 where OpenAI
staff and maintainers reference it). The app invokes:

```text
<CODEX_CLI_PATH> app-server --analytics-default-enabled
```

over stdio JSON-RPC. Codex Universal is that binary — no files inside the signed
application are touched.

## Install

```powershell
cd <repo>
.\scripts\build-windows.ps1        # cargo build --release -p codex-cli
.\scripts\install-desktop-backend.ps1
taskkill /IM Codex.exe /F          # fully stop Desktop, then relaunch it
```

`install-desktop-backend.ps1` verifies `codex --version`, backs up any existing
`CODEX_CLI_PATH` to `%USERPROFILE%\.codex-universal-cli-path.bak`, and sets the
user-level variable.

## Uninstall / restore

```powershell
.\scripts\uninstall-desktop-backend.ps1
```

Restores the backed-up value or clears the variable.

## Diagnostics

```powershell
.\scripts\diagnose.ps1
```

Reports build state, current override, credential *presence* (never values),
config summary, and runs an app-server handshake smoke test.

## Using non-OpenAI models in Desktop

1. Set provider keys (`OPENROUTER_API_KEY`, optionally `OPENCODE_API_KEY`).
2. Restart Desktop after installing the backend.
3. The model picker now lists `[OpenRouter] ...` and `[OpenCode] ...` entries.
4. Select one; threads/turns/resumes keep using that provider via the composite
   `<provider>::<model>` identity, which the backend decodes transparently.

If dynamic catalogs are unreachable (offline), picker entries for those providers are
simply absent — configure `model = "provider::model"` explicitly instead; inference
does not require catalog availability.

## WSL agent mode

For "Windows Subsystem for Linux" agent environments, Desktop executes the
`CODEX_CLI_PATH` target through WSL interop. A Windows `.exe` generally works but
produces a hybrid session (see upstream issue #28086). For a native-WSL experience,
build the fork inside WSL (`cargo build --release -p codex-cli`) and point
`CODEX_CLI_PATH` at a POSIX shell wrapper on a Windows-visible path:

```sh
#!/usr/bin/env sh
exec ~/.local/bin/codex "$@"
```

## Known limitations

- The Desktop renderer itself cannot be modified; provider selection relies on the
  encoded model identities described above.
- If Desktop pins a feature flag unknown to this backend's version, turns may fail —
  keep the fork reasonably close to upstream (see upstream issue #30432 requesting an
  explicit app-server override).
- macOS: use `launchctl setenv CODEX_CLI_PATH /path/to/codex-universal/codex-rs/target/release/codex`
  then restart Desktop. Linux has no official Desktop app; the CLI/app-server still work.
