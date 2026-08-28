[CmdletBinding()]
param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $projectRoot

Write-Host "[1/4] Checking Node dependencies"
if (-not (Test-Path "node_modules")) {
    npm ci
}

Write-Host "[2/4] Checking frontend command contract"
npm run verify:contracts

Write-Host "[3/4] Checking Rust formatting and tests"
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml

Write-Host "[4/4] Building Windows bundle"
if ($Release) {
    npm run tauri build
} else {
    npm run tauri build -- --debug
}

Write-Host "Build completed. Inspect src-tauri/target/release/bundle or src-tauri/target/debug/bundle."
