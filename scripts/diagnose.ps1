# Codex Universal - diagnostics.
#
# Reports provider/model/backend configuration without ever printing secrets.

$ErrorActionPreference = "Continue"

function Test-EnvPresent([string]$Name) {
    $value = [Environment]::GetEnvironmentVariable($Name, "User")
    $proc = [Environment]::GetEnvironmentVariable($Name, "Process")
    return [bool]($value -or $proc)
}

Write-Host "== Codex Universal diagnostics =="
Write-Host ""

$repoRoot = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $repoRoot "codex-rs\target\release\codex.exe"
$upstreamSha = "unknown"

$gitDir = Join-Path $repoRoot ".git"
if (Test-Path $gitDir) {
    try {
        $upstreamSha = (git -C $repoRoot log --format=%H -1)
    } catch {}
}
Write-Host ("Upstream base commit : " + $upstreamSha)

if (Test-Path -LiteralPath $binary) {
    Write-Host ("Universal binary     : " + $binary)
    & $binary --version
} else {
    Write-Host ("Universal binary     : NOT BUILT (" + $binary + ")")
}
Write-Host ""

$currentCli = [Environment]::GetEnvironmentVariable("CODEX_CLI_PATH", "User")
Write-Host ("Desktop override     : " + $(if ($currentCli) { $currentCli } else { "(not set)" }))
Write-Host ("Points at this fork  : " + ($currentCli -and ($currentCli -eq $binary)))
Write-Host ""

Write-Host "-- Credentials (presence only, values are never shown) --"
"OPENAI_API_KEY", "OPENROUTER_API_KEY", "OPENCODE_API_KEY", "ANTHROPIC_API_KEY" | ForEach-Object {
    Write-Host ("  {0,-20}: {1}" -f $_, $(if (Test-EnvPresent $_) { "present" } else { "absent" }))
}

$opencodeAuth = Join-Path $env:USERPROFILE ".local\share\opencode\auth.json"
Write-Host ("  OpenCode auth store : " + $(if (Test-Path -LiteralPath $opencodeAuth) { "present ($opencodeAuth)" } else { "absent" }))
Write-Host ""

$configToml = Join-Path $env:USERPROFILE ".codex\config.toml"
Write-Host "-- ~/.codex/config.toml --"
if (Test-Path -LiteralPath $configToml) {
    Write-Host "  present: $configToml"
    Select-String -LiteralPath $configToml -Pattern '^\s*model\s*=|^\s*model_provider\s*=|^\[model_providers' | ForEach-Object {
        Write-Host ("  " + $_.Line.Trim())
    }
} else {
    Write-Host "  not found"
}
Write-Host ""

if (Test-Path -LiteralPath $binary) {
    Write-Host "-- app-server smoke test --"
    Write-Host "  launching 'codex app-server' handshake check..."
    $job = Start-Job -ScriptBlock {
        param($b)
        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName = $b
        $psi.Arguments = "app-server"
        $psi.RedirectStandardInput = $true
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.UseShellExecute = $false
        $p = [System.Diagnostics.Process]::Start($psi)
        $init = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"diagnose","title":"diagnose","version":"0.0.0"}}}'
        $p.StandardInput.WriteLine($init)
        $p.StandardInput.Flush()
        $line = $p.StandardOutput.ReadLine()
        if ($line) { "OK: got response of length $($line.Length)" } else { "FAIL: no response" }
        try { $p.Kill() } catch {}
    } -ArgumentList $binary

    $result = Wait-Job $job -Timeout 20 | Receive-Job
    Remove-Job $job -Force -ErrorAction SilentlyContinue
    if ($result) { Write-Host ("  " + $result) } else { Write-Host "  FAIL: app-server did not respond within 20s" }
}
