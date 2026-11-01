# Uninstall "Delete the application data": Gond Code's own data goes, everything else stays.
# Usage: powershell -File scripts/purge-check.ps1 -Installer path\to\*-setup.exe
# Windows PowerShell 5 (not pwsh): the checkbox pass drives the real uninstaller with UI Automation.
param([Parameter(Mandatory = $true)][string]$Installer)
$ErrorActionPreference = 'Stop'
$results = @()
function Check($name, $ok, $detail) {
  $ok = [bool]$ok
  $script:results += [pscustomobject]@{ name = $name; ok = $ok; detail = "$detail" }
  Write-Host ("[{0}] {1} - {2}" -f ($(if ($ok) { 'PASS' } else { 'FAIL' })), $name, $detail)
}

$installDir = Join-Path $env:LOCALAPPDATA 'Gond Code'
$roaming = Join-Path $env:APPDATA 'GondCode'
$local = Join-Path $env:LOCALAPPDATA 'GondCode'
$db = Join-Path $roaming 'gond-code.db'
$cred = 'GondCode/claude/PURGE-TEST'
$other = 'OtherApp/keep-me'
$keepDir = Join-Path $env:APPDATA 'GondCodeNotOurs'   # a look-alike folder: must survive
$project = Join-Path $env:TEMP 'purge-project'        # a user's project: must survive

function Install { $p = Start-Process $Installer -ArgumentList '/S' -Wait -PassThru; if ($p.ExitCode) { throw "install exit $($p.ExitCode)" } }
# `cmdkey /list:<name>` echoes the name even when there's no such credential ("* NONE *"),
# so look for the stored target in the full list instead.
function Has-Cred($n) { (cmdkey /list | Out-String) -match ('target=' + [regex]::Escape($n) + '\s') }

# Run the app once so it makes its database, and plant an account secret, a sign-in home and
# the things that must survive.
function Seed {
  New-Item -ItemType Directory -Force $project, $keepDir | Out-Null
  if (-not (Test-Path "$project\.git")) { git -C $project init -q; Set-Content "$project\README.md" 'mine' }
  $app = Start-Process (Join-Path $installDir 'gond-code.exe') -PassThru
  for ($i = 0; $i -lt 60 -and -not (Test-Path (Join-Path $roaming 'gond-code.db')); $i++) { Start-Sleep -Milliseconds 500 }
  Start-Sleep 2
  Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
  Start-Sleep 2
  # A sign-in home, as an account's own login keeps in local app data.
  New-Item -ItemType Directory -Force (Join-Path $local 'accounts\purge-test') | Out-Null
  cmdkey /generic:$cred /user:x /pass:secret | Out-Null
  cmdkey /generic:$other /user:x /pass:keep | Out-Null
  # What purge-data would remove right now (folders and GondCode/ credentials), for the log.
  $ErrorActionPreference = 'Continue'
  & (Join-Path $installDir 'gond-code.exe') purge-data --dry-run 2>&1 | ForEach-Object { "  dry run: $_" }
  $ErrorActionPreference = 'Stop'
}

Add-Type @"
using System; using System.Runtime.InteropServices;
public static class GondMouse {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  public static void Click(int x, int y) { SetCursorPos(x, y); mouse_event(2, 0, 0, 0, IntPtr.Zero); mouse_event(4, 0, 0, 0, IntPtr.Zero); }
}
"@
# A real click in the middle of a control: NSIS's controls don't all offer UIA patterns.
function Click-Element($e) {
  $r = $e.Current.BoundingRectangle
  [GondMouse]::Click([int]($r.X + [Math]::Min(12, $r.Width / 2)), [int]($r.Y + $r.Height / 2))
  Start-Sleep -Milliseconds 400
}

function Uninstall-With-Checkbox {
  Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
  $u = Start-Process (Join-Path $installDir 'uninstall.exe') -PassThru
  $root = [System.Windows.Automation.AutomationElement]::RootElement
  # NSIS re-launches the uninstaller from %TEMP% (Au_.exe), so find its window by title, not pid.
  $tops = { $root.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition) | Where-Object { $_.Current.Name -like 'Gond Code*Uninstall*' } }
  $win = $null
  for ($i = 0; $i -lt 60 -and -not $win; $i++) { Start-Sleep -Milliseconds 500; $win = & $tops | Select-Object -First 1 }
  if (-not $win) { throw 'no uninstaller window' }
  $find = { param($name) $c = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::NameProperty), $name; $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c) }
  $box = $null; for ($i = 0; $i -lt 20 -and -not $box; $i++) { Start-Sleep -Milliseconds 300; $box = & $find 'Delete the application data' }
  if (-not $box) { throw 'no "Delete the application data" checkbox' }
  try { $win.SetFocus() } catch {}
  Click-Element $box
  $tp = $null; try { $tp = $box.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern) } catch {}
  Write-Host "checkbox: $(if ($tp) { $tp.Current.ToggleState } else { 'clicked' })"
  Click-Element (& $find 'Uninstall')
  # The finish page: Close.
  for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Milliseconds 500
    if (-not (Get-Process Au_ -ErrorAction SilentlyContinue)) { break }
    $close = & $find 'Close'
    if ($close -and $close.Current.IsEnabled) { Click-Element $close; break }
  }
  $u.WaitForExit(30000) | Out-Null
  # The uninstaller re-launches itself from %TEMP% (Au_.exe); wait for every copy.
  for ($i = 0; $i -lt 60 -and (Get-Process Au_ -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Milliseconds 500 }
}

function Survivors($pass) {
  Check "$pass`: the project folder is untouched" (Test-Path "$project\README.md") $project
  Check "$pass`: a look-alike folder is untouched" (Test-Path $keepDir) $keepDir
  Check "$pass`: another app's credential is untouched" (Has-Cred $other) $other
}

# 1. Box unticked (silent uninstall): everything stays, as before.
Install; Seed
Check 'seeded: database exists' (Test-Path $db) $db
Check 'seeded: GondCode credential exists' (Has-Cred $cred) $cred
Start-Process (Join-Path $installDir 'uninstall.exe') -ArgumentList '/S' -Wait | Out-Null
for ($i = 0; $i -lt 60 -and (Get-Process Au_ -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Milliseconds 500 }
Check 'unticked: database kept' (Test-Path $db) $db
Check 'unticked: local data kept' (Test-Path $local) $local
Check 'unticked: credential kept' (Has-Cred $cred) $cred
Survivors 'unticked'

# 2. The real checkbox, ticked, through the uninstaller's own window.
Install
try { Uninstall-With-Checkbox } catch { Check 'ticked: drove the uninstaller window' $false $_ }
Check 'ticked: %APPDATA%\GondCode gone' (-not (Test-Path $roaming)) $roaming
Check 'ticked: %LOCALAPPDATA%\GondCode gone' (-not (Test-Path $local)) $local
Check 'ticked: GondCode credentials gone' (-not (Has-Cred $cred)) $cred
Survivors 'ticked'

# 3. Reinstall: no database, so the app starts with a fresh one.
Install
$db = Join-Path $roaming 'gond-code.db'
Check 'reinstalled: no database before launch' (-not (Test-Path $db)) $db
$app = Start-Process (Join-Path $installDir 'gond-code.exe') -PassThru
for ($i = 0; $i -lt 60 -and -not (Test-Path $db); $i++) { Start-Sleep -Milliseconds 500 }
Check 'reinstalled: the app made a fresh database' (Test-Path $db) $db
Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue; Start-Sleep 2

# 4. Scripted: /S /PURGE does the same as the ticked box.
Seed
Start-Process (Join-Path $installDir 'uninstall.exe') -ArgumentList '/S', '/PURGE' -Wait | Out-Null
for ($i = 0; $i -lt 60 -and (Get-Process Au_ -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Milliseconds 500 }
Check '/PURGE: database gone' (-not (Test-Path $db)) $db
Check '/PURGE: credential gone' (-not (Has-Cred $cred)) $cred
Survivors '/PURGE'

cmdkey /delete:$other | Out-Null
$failed = @($results | Where-Object { -not $_.ok }).Count
Write-Host "purge check: $($results.Count - $failed)/$($results.Count) passed"
if ($failed) { exit 1 }
