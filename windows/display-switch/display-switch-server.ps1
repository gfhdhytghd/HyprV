param(
    [int]$Port = 8767,
    [int]$DelaySeconds = 30
)

$ErrorActionPreference = 'Stop'
$stateDir = Join-Path $env:LOCALAPPDATA 'DdcDisplaySwitch'
$tokenFile = Join-Path $stateDir 'token'
$logFile = Join-Path $stateDir 'display-switch-server.log'

Add-Type @'
using System;
using System.Runtime.InteropServices;

public static class MonitorPower {
    private const int HWND_BROADCAST = 0xffff;
    private const int WM_SYSCOMMAND = 0x0112;
    private const int SC_MONITORPOWER = 0xf170;
    private const int SMTO_ABORTIFHUNG = 0x0002;

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SendMessageTimeout(
        IntPtr hWnd, int msg, IntPtr wParam, IntPtr lParam,
        int flags, int timeout, out IntPtr result);

    public static void SetPower(bool enabled) {
        IntPtr result;
        SendMessageTimeout(
            new IntPtr(HWND_BROADCAST), WM_SYSCOMMAND,
            new IntPtr(SC_MONITORPOWER), new IntPtr(enabled ? -1 : 2),
            SMTO_ABORTIFHUNG, 2000, out result);
    }
}
'@

function Write-Log([string]$Message) {
    $line = '{0:o} {1}' -f (Get-Date), $Message
    Add-Content -LiteralPath $logFile -Value $line -Encoding UTF8
}

function Send-Response($Stream, [int]$Status, [string]$Reason, [hashtable]$Body) {
    $json = $Body | ConvertTo-Json -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($json)
    $header = "HTTP/1.1 $Status $Reason`r`nContent-Type: application/json`r`nContent-Length: $($bytes.Length)`r`nConnection: close`r`n`r`n"
    $headerBytes = [Text.Encoding]::ASCII.GetBytes($header)
    $Stream.Write($headerBytes, 0, $headerBytes.Length)
    $Stream.Write($bytes, 0, $bytes.Length)
    $Stream.Flush()
}

function Test-Token([string]$Actual, [string]$Expected) {
    $actualBytes = [Text.Encoding]::UTF8.GetBytes($Actual)
    $expectedBytes = [Text.Encoding]::UTF8.GetBytes($Expected)
    if ($actualBytes.Length -ne $expectedBytes.Length) { return $false }
    $difference = 0
    for ($index = 0; $index -lt $actualBytes.Length; $index++) {
        $difference = $difference -bor ($actualBytes[$index] -bxor $expectedBytes[$index])
    }
    return $difference -eq 0
}

New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
if (-not (Test-Path -LiteralPath $tokenFile)) {
    throw "Missing token file: $tokenFile"
}
$token = (Get-Content -LiteralPath $tokenFile -Raw).Trim()
if (-not $token) { throw 'Token file is empty' }

$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Any, $Port)
$listener.Start()
$busy = $false
Write-Log "listening on 0.0.0.0:$Port delay_seconds=$DelaySeconds"

try {
    while ($true) {
        $client = $listener.AcceptTcpClient()
        try {
            $client.ReceiveTimeout = 5000
            $stream = $client.GetStream()
            $reader = [IO.StreamReader]::new($stream, [Text.Encoding]::ASCII, $false, 1024, $true)
            $requestLine = $reader.ReadLine()
            if (-not $requestLine) { continue }

            $headers = @{}
            while (($line = $reader.ReadLine()) -ne '') {
                $separator = $line.IndexOf(':')
                if ($separator -gt 0) {
                    $headers[$line.Substring(0, $separator).Trim().ToLowerInvariant()] = $line.Substring($separator + 1).Trim()
                }
            }

            $parts = $requestLine.Split(' ')
            $method = $parts[0]
            $path = $parts[1]
            if ($method -eq 'GET' -and $path -eq '/health') {
                Send-Response $stream 200 'OK' @{ ok = $true; service = 'display-switch'; busy = $busy; port = $Port; delay_seconds = $DelaySeconds }
                continue
            }
            if ($method -ne 'POST' -or $path -ne '/switch') {
                Send-Response $stream 404 'Not Found' @{ ok = $false; error = 'not_found' }
                continue
            }

            $authorization = [string]$headers['authorization']
            if (-not $authorization.StartsWith('Bearer ') -or -not (Test-Token $authorization.Substring(7) $token)) {
                Send-Response $stream 401 'Unauthorized' @{ ok = $false; error = 'unauthorized' }
                continue
            }
            if ($busy) {
                Send-Response $stream 409 'Conflict' @{ ok = $false; error = 'busy' }
                continue
            }

            $busy = $true
            try {
                Write-Log 'switch started'
                [MonitorPower]::SetPower($false)
                Start-Sleep -Seconds $DelaySeconds
                [MonitorPower]::SetPower($true)
                Write-Log 'switch completed'
                Send-Response $stream 200 'OK' @{ ok = $true }
            }
            finally {
                $busy = $false
            }
        }
        catch {
            Write-Log "request failed: $($_.Exception.Message)"
        }
        finally {
            $client.Dispose()
        }
    }
}
finally {
    $listener.Stop()
}
