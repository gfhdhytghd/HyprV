param(
    [Parameter(Mandatory = $true)]
    [string]$Token,
    [int]$Port = 8767
)

$ErrorActionPreference = 'Stop'
$sourceScript = Join-Path $PSScriptRoot 'display-switch-server.ps1'
$installDir = Join-Path $env:LOCALAPPDATA 'DdcDisplaySwitch'
$installedScript = Join-Path $installDir 'display-switch-server.ps1'
$tokenFile = Join-Path $installDir 'token'
$taskName = 'DdcDisplaySwitchServer'

if (-not (Test-Path -LiteralPath $sourceScript)) {
    throw "Missing server script: $sourceScript"
}

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Copy-Item -LiteralPath $sourceScript -Destination $installedScript -Force
Set-Content -LiteralPath $tokenFile -Value $Token -NoNewline -Encoding ASCII

$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument (
    '-NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "{0}" -Port {1}' -f $installedScript, $Port
)
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null

$firewallRule = Get-NetFirewallRule -DisplayName 'DDC Display Switch HTTP' -ErrorAction SilentlyContinue
if ($firewallRule) {
    $firewallRule | Remove-NetFirewallRule
}
New-NetFirewallRule -DisplayName 'DDC Display Switch HTTP' -Direction Inbound -Action Allow -Protocol TCP -LocalPort $Port -Profile Private,Public -RemoteAddress '172.16.105.0/24' | Out-Null

Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
Start-ScheduledTask -TaskName $taskName
Start-Sleep -Seconds 2

$health = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/health" -TimeoutSec 5
if (-not $health.ok) { throw 'Local health check failed' }
Write-Host "Installed $taskName on port $Port"
Write-Host "Health: http://127.0.0.1:$Port/health"
