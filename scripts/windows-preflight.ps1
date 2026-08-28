$ErrorActionPreference = "Stop"

Write-Host "UniSnap-Windows Windows preflight"
Write-Host "==============================="

$os = Get-CimInstance Win32_OperatingSystem
Write-Host ("OS       : {0} {1}" -f $os.Caption, $os.Version)
Write-Host ("Computer : {0}" -f $env:COMPUTERNAME)

Write-Host "`nGPU:"
Get-CimInstance Win32_VideoController |
    Select-Object Name, DriverVersion, VideoModeDescription |
    Format-Table -AutoSize

Write-Host "Audio Endpoints:"
$audioDevices = Get-PnpDevice -Class AudioEndpoint -ErrorAction SilentlyContinue
if ($audioDevices) {
    $audioDevices | Select-Object Status, FriendlyName, InstanceId | Format-Table -AutoSize
} else {
    Write-Warning "AudioEndpoint not found. Microphone and system audio verification require device/driver configuration."
}

Write-Host "Tools:"
$missingTools = @()
foreach ($tool in @("node", "npm", "cargo")) {
    $command = Get-Command $tool -ErrorAction SilentlyContinue
    if ($command) {
        Write-Host ("{0,-7}: {1}" -f $tool, $command.Source)
    } else {
        Write-Warning ("Tool missing: {0}" -f $tool)
        $missingTools += $tool
    }
}

if (-not (Test-Path "src-tauri/tauri.conf.json")) {
    throw "Please run this script from the UniSnap-Windows project root directory."
}

if ("npm" -in $missingTools -or "node" -in $missingTools) {
    Write-Warning "Node.js/npm is missing. Skipping 'npm run verify:contracts'. Please install Node.js to complete contract verification."
} else {
    Write-Host "`nRunning command contract check:"
    npm run verify:contracts
}

Write-Host "`nPreflight complete. This verifies environment and static contracts only."

