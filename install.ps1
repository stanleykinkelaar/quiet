# Installs (or updates) Quiet on Windows from the latest GitHub release.
#
#   irm https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.ps1 | iex
#
# Per-user install, no admin prompt. Uninstall from Settings, Apps, as usual.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$url = 'https://github.com/stanleykinkelaar/Quiet/releases/latest/download/QuietSetup.exe'
$setup = Join-Path $env:TEMP 'QuietSetup.exe'

Write-Host 'Downloading Quiet...'
Invoke-WebRequest -Uri $url -OutFile $setup -UseBasicParsing

# Setup cannot replace a running copy.
Get-Process -Name quiet -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

Write-Host 'Installing...'
$process = Start-Process -FilePath $setup -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait -PassThru
Remove-Item $setup -ErrorAction SilentlyContinue
if ($process.ExitCode -ne 0) { throw "Setup failed with exit code $($process.ExitCode)" }

Start-Process (Join-Path $env:LOCALAPPDATA 'Programs\Quiet\quiet.exe')
Write-Host 'Quiet is installed and running. Press Ctrl+Alt+Space anywhere to capture a task.'
