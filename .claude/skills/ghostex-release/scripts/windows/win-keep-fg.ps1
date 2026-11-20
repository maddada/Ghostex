# Release smoke (Windows): run a step that makes the test copy open or focus a window (browser open,
# settings open, show, focus), then hand the foreground straight back to whatever had it before.
#   win-keep-fg.ps1 -TestPid <test copy pid> -Run { <script block> } [-WaitMs 4000]
# GPUI activates a window it opens or focuses, even in the background; without this the user's
# typing lands in the test copy.
param([Parameter(Mandatory)][int]$TestPid, [Parameter(Mandatory)][scriptblock]$Run, [int]$WaitMs = 4000)
if (-not ('SmokeFg.F' -as [type])) {
  Add-Type -Namespace SmokeFg -Name F -MemberDefinition '[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);'
}
$prev = [SmokeFg.F]::GetForegroundWindow(); $prevPid = 0; [void][SmokeFg.F]::GetWindowThreadProcessId($prev, [ref]$prevPid)
& $Run
$until = (Get-Date).AddMilliseconds($WaitMs)
while ((Get-Date) -lt $until) {
  Start-Sleep -Milliseconds 250
  $q = 0; [void][SmokeFg.F]::GetWindowThreadProcessId([SmokeFg.F]::GetForegroundWindow(), [ref]$q)
  if ($q -eq $TestPid -and $prevPid -ne $TestPid) {
    (@{ pid = [int]$prevPid; window_id = $prev.ToInt64() } | ConvertTo-Json -Compress) | cua-driver bring_to_front 2>$null | Out-Null
    Write-Host "gave the foreground back to pid $prevPid"
  }
}
