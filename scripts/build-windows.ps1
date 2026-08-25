# Codex Universal - Windows build script.
# Builds the release codex.exe from this fork.

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$cargoManifest = Join-Path $repoRoot "codex-rs"

Write-Host "==> Building Codex Universal (release)..."
Push-Location $cargoManifest
try {
    cargo build --release -p codex-cli
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

$binary = Join-Path $cargoManifest "target\release\codex.exe"
if (-not (Test-Path -LiteralPath $binary)) {
    throw "Expected binary not found at $binary"
}

Write-Host ""
& $binary --version
Write-Host ""
Write-Host "Build OK: $binary"
Write-Host "Next step: .\scripts\install-desktop-backend.ps1"
