# Codex Universal - restore the previous Codex Desktop backend configuration.
#
# Restores the CODEX_CLI_PATH value saved by install-desktop-backend.ps1, or
# removes the variable if no backup exists.

$ErrorActionPreference = "Stop"

$backupFile = Join-Path $env:USERPROFILE ".codex-universal-cli-path.bak"

if (Test-Path -LiteralPath $backupFile) {
    $previous = Get-Content -LiteralPath $backupFile -Raw
    $previous = $previous.Trim()
    [Environment]::SetEnvironmentVariable("CODEX_CLI_PATH", $previous, "User")
    Remove-Item -LiteralPath $backupFile -Force
    Write-Host "Restored CODEX_CLI_PATH (User) to:"
    Write-Host "  $previous"
}
else {
    [Environment]::SetEnvironmentVariable("CODEX_CLI_PATH", $null, "User")
    Write-Host "No backup found; removed CODEX_CLI_PATH (User)."
}

Write-Host ""
Write-Host "Restart Codex Desktop completely for this to take effect:"
Write-Host "  taskkill /IM Codex.exe /F   (then relaunch the app)"
