# Release smoke (Windows): start a throwaway portable copy of Ghostex without taking the foreground.
#
#   win-launch-test-copy.ps1 -Root <folder> [-From <installed current\ folder>] [-Fresh]
#
# -Fresh (or a missing <Root>\current) copies the app from -From (default: the installed app,
# %LOCALAPPDATA%\Ghostex\current; pass a staged release build's folder instead to test that).
# A `.portable` marker beside current\ gives the copy its own ports and data, and
# GHOSTEX_HOME / USERPROFILE / HOME / CODEX_HOME point into <Root>, so the agent configs and
# skills the copy's gxserver installs never touch the user's own ~/.claude, ~/.codex etc.
# The process starts with SW_SHOWNOACTIVATE (no Start-Process); GPUI still activates its window, so
# the script hands the foreground back to its previous owner. Prints the app pid.
param([Parameter(Mandatory)][string]$Root, [string]$From = "$env:LOCALAPPDATA\Ghostex\current", [switch]$Fresh)
$ErrorActionPreference = 'Stop'
if ($Fresh -or -not (Test-Path "$Root\current\Ghostex.exe")) {
  robocopy $From "$Root\current" /MIR /NFL /NDL /NJH /NJS /NP /R:1 /W:1 | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "copy failed ($LASTEXITCODE)" }
}
New-Item -ItemType File -Force "$Root\.portable" | Out-Null
New-Item -ItemType Directory -Force "$Root\home\.codex", "$Root\unixhome" | Out-Null
$vars = @{}
foreach ($e in [Environment]::GetEnvironmentVariables().GetEnumerator()) {
  if ($e.Key -match '^(GHOSTEX_|WMX_|ZMX_|CLAUDE|CODEX_|COPILOT_|FACTORY_)' -or $e.Key -eq 'CLAUDECODE') { continue }
  $vars[$e.Key] = $e.Value
}
$vars['Path'] = (($vars['Path'] -split ';') | Where-Object { $_ -and $_ -notmatch '\\AppData\\Local\\Ghostex(ThemeLab)?\\' }) -join ';'
$vars['GHOSTEX_HOME'] = "$Root\home"; $vars['USERPROFILE'] = "$Root\home"
$vars['HOME'] = "$Root\unixhome"; $vars['CODEX_HOME'] = "$Root\home\.codex"
$block = (($vars.GetEnumerator() | Sort-Object Key | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join "`0") + "`0`0"
if (-not ('SmokeLaunch.P' -as [type])) {
  Add-Type -Namespace SmokeLaunch -Name P -MemberDefinition @'
[StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct SI { public int cb; public string r, d, t; public int x, y, w, h, xc, yc, fill, flags; public short show, r2; public IntPtr r3, i, o, e; }
[StructLayout(LayoutKind.Sequential)] public struct PI { public IntPtr hp, ht; public int pid, tid; }
[DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool CreateProcessW(string app, System.Text.StringBuilder cmd, IntPtr pa, IntPtr ta, bool inherit, uint flags, IntPtr env, string dir, ref SI si, out PI pi);
[DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
'@
}
if (-not ('SmokeLaunch.F' -as [type])) {
  Add-Type -Namespace SmokeLaunch -Name F -MemberDefinition '[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);'
}
$prevFg = [SmokeLaunch.F]::GetForegroundWindow(); $prevPid = 0; [void][SmokeLaunch.F]::GetWindowThreadProcessId($prevFg, [ref]$prevPid)
$exe = "$Root\current\Ghostex.exe"
$si = New-Object SmokeLaunch.P+SI; $si.cb = [Runtime.InteropServices.Marshal]::SizeOf($si)
$si.flags = 1; $si.show = 4   # STARTF_USESHOWWINDOW, SW_SHOWNOACTIVATE
$envPtr = [Runtime.InteropServices.Marshal]::StringToHGlobalUni($block)
$pi = New-Object SmokeLaunch.P+PI
$ok = [SmokeLaunch.P]::CreateProcessW($exe, (New-Object Text.StringBuilder "`"$exe`""), [IntPtr]::Zero, [IntPtr]::Zero, $false, 0x400, $envPtr, "$Root\current", [ref]$si, [ref]$pi)
[Runtime.InteropServices.Marshal]::FreeHGlobal($envPtr)
if (-not $ok) { throw "CreateProcess failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
[void][SmokeLaunch.P]::CloseHandle($pi.hp); [void][SmokeLaunch.P]::CloseHandle($pi.ht)
# GPUI activates its first window itself (seen 2026-11-20: the copy took the foreground within 2 s
# despite SW_SHOWNOACTIVATE). Wait for the main window, then hand the foreground straight back.
foreach ($i in 1..40) {
  Start-Sleep -Milliseconds 500
  $w = ('{"pid":' + $pi.pid + '}') | cua-driver list_windows 2>$null | ConvertFrom-Json
  if ($w.windows | Where-Object { $_.bounds.width -gt 600 }) { break }
}
Start-Sleep -Milliseconds 800
$q = 0; [void][SmokeLaunch.F]::GetWindowThreadProcessId([SmokeLaunch.F]::GetForegroundWindow(), [ref]$q)
if ($q -eq $pi.pid -and $prevPid -and $prevPid -ne $pi.pid) {
  (@{ pid = [int]$prevPid; window_id = $prevFg.ToInt64() } | ConvertTo-Json -Compress) | cua-driver bring_to_front 2>$null | Out-Null
  Write-Host "gave the foreground back to pid $prevPid"
}
$pi.pid
