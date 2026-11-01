# Installer smoke test: silent NSIS install, launch, stay alive, silent uninstall.
# Usage: pwsh scripts/smoke-windows.ps1 -Installer path\to\*-setup.exe [-AliveSeconds 10]
param(
  [Parameter(Mandatory = $true)][string]$Installer,
  [int]$AliveSeconds = 10
)
$ErrorActionPreference = 'Stop'

$installDir = Join-Path $env:LOCALAPPDATA 'Gond Code'  # installMode: currentUser
Write-Host "Installing $Installer silently"
$p = Start-Process -FilePath $Installer -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "Installer exited with $($p.ExitCode)" }

$exe = Get-ChildItem $installDir -Filter *.exe | Where-Object { $_.Name -notlike 'uninstall*' } | Select-Object -First 1
if (-not $exe) { throw "No app executable in $installDir" }
$uninstaller = Join-Path $installDir 'uninstall.exe'
if (-not (Test-Path $uninstaller)) { throw "No uninstaller at $uninstaller" }
Write-Host "Installed $($exe.FullName)"

# Builds before 0.1.0 put <install>\bin on the user PATH; this one leaves none behind.
if (Test-Path (Join-Path $installDir 'bin')) { throw "$installDir\bin exists after install" }

$app = Start-Process -FilePath $exe.FullName -PassThru
Start-Sleep -Seconds $AliveSeconds
$app.Refresh()
if ($app.HasExited) { throw "App exited within $AliveSeconds s (exit code $($app.ExitCode))" }
Write-Host "App alive after $AliveSeconds s (pid $($app.Id))"

Stop-Process -Id $app.Id -Force
$app.WaitForExit(10000) | Out-Null

Write-Host 'Uninstalling silently'
$u = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru
if ($u.ExitCode -ne 0) { throw "Uninstaller exited with $($u.ExitCode)" }
# NSIS uninstallers copy themselves to %TEMP% and finish there; give that a moment.
for ($i = 0; $i -lt 30 -and (Test-Path $exe.FullName); $i++) { Start-Sleep -Seconds 1 }
if (Test-Path $exe.FullName) { throw "$($exe.FullName) is still there after uninstall" }
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ((($userPath -split ';') -contains (Join-Path $installDir 'bin'))) { throw "$installDir\bin is still on the user PATH after uninstall" }
Write-Host 'Smoke test passed'
