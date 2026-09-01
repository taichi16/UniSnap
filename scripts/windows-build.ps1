[CmdletBinding()]
param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $projectRoot

function Invoke-CheckedNative {
    param(
        [Parameter(Mandatory = $true)]
        [scriptblock]$Command,
        [Parameter(Mandatory = $true)]
        [string]$Description
    )

    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

Write-Host "[1/4] Checking Node dependencies"
if (-not (Test-Path "node_modules")) {
    Invoke-CheckedNative { npm ci } "npm ci"
}

Write-Host "[2/4] Checking frontend command contract"
Invoke-CheckedNative { npm run verify:contracts } "Frontend command contract"

Write-Host "[3/4] Checking Rust formatting and tests"
Invoke-CheckedNative { cargo fmt --manifest-path src-tauri/Cargo.toml -- --check } "Rust formatting"
Invoke-CheckedNative { cargo test --manifest-path src-tauri/Cargo.toml } "Rust tests"

Write-Host "[4/4] Building Windows bundle"
if ($Release) {
    Invoke-CheckedNative { npm run tauri build } "Release bundle"
} else {
    Invoke-CheckedNative { npm run tauri build -- --debug } "Debug bundle"
}

Write-Host "Build completed. Inspect src-tauri/target/release/bundle or src-tauri/target/debug/bundle."
