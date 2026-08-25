# Codex Universal - point the official Codex Desktop app at this fork's binary.
#
# 1. Locates the freshly built codex.exe
# 2. Verifies it executes
# 3. Backs up the current CODEX_CLI_PATH (if any)
# 4. Sets the user-level CODEX_CLI_PATH to this binary
#
# The official Codex installation is never modified or deleted.

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $repoRoot "codex-rs\target\release\codex.exe"

if (-not (Test-Path -LiteralPath $binary)) {
    throw "Binary not found at $binary. Run .\scripts\build-windows.ps1 first."
}

Write-Host "==> Verifying binary..."
& $binary --version
if ($LASTEXITCODE -ne 0) {
    throw "'codex --version' failed with exit code $LASTEXITCODE"
}

$current = [Environment]::GetEnvironmentVariable("CODEX_CLI_PATH", "User")
if ($current) {
    if ($current -eq $binary) {
        Write-Host "CODEX_CLI_PATH already points at this binary. Nothing to change."
        exit 0
    }
    $backupFile = Join-Path $env:USERPROFILE ".codex-universal-cli-path.bak"
    Set-Content -LiteralPath $backupFile -Value $current
    Write-Host "Saved previous CODEX_CLI_PATH to $backupFile"
    Write-Host "  previous: $current"
}

[Environment]::SetEnvironmentVariable("CODEX_CLI_PATH", $binary, "User")
Write-Host ""
Write-Host "Set CODEX_CLI_PATH (User) ="
Write-Host "  $binary"
Write-Host ""
Write-Host "Restart Codex Desktop completely:"
Write-Host "  taskkill /IM Codex.exe /F   (then relaunch the app)"
Write-Host ""
Write-Host "To undo later: .\scripts\uninstall-desktop-backend.ps1"
