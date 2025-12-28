# Build all WASM test fixtures (Windows PowerShell)
#
# Prerequisites:
#   cargo install cargo-component
#   rustup target add wasm32-wasip1
#
# Usage:
#   .\build.ps1
#
# Output:
#   Built WASM files will be in target\wasm32-wasip1\release\*.wasm
#   and copied to ..\modules\

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

Write-Host "Building WASM test fixtures..."

# Build all components in release mode
cargo component build --release --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Create modules directory if it doesn't exist
$ModulesDir = Join-Path (Split-Path -Parent $ScriptDir) "modules"
if (-not (Test-Path $ModulesDir)) {
    New-Item -ItemType Directory -Path $ModulesDir | Out-Null
}

Write-Host ""
Write-Host "Copying WASM files to modules directory..."

$fixtures = @("echo", "panic", "infinite_loop", "memory_hog", "fuel_burner")

foreach ($name in $fixtures) {
    $wasmFile = "target\wasm32-wasip1\release\$name.wasm"
    if (Test-Path $wasmFile) {
        Copy-Item $wasmFile "$ModulesDir\$name.wasm"
        Write-Host "  Copied $name.wasm"
    } else {
        Write-Host "  Warning: $name.wasm not found" -ForegroundColor Yellow
    }
}

Write-Host ""
Write-Host "Build complete! WASM files are in:"
Write-Host "  - $ScriptDir\target\wasm32-wasip1\release\"
Write-Host "  - $ModulesDir\"
