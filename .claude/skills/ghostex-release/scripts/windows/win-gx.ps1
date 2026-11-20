# Release smoke (Windows): run the test copy's own `ghostex` CLI against its own gxserver.
#   win-gx.ps1 -Root <folder> <ghostex args...>
# Same GHOSTEX_HOME / USERPROFILE / HOME / CODEX_HOME as win-launch-test-copy.ps1, and none of
# this agent session's GHOSTEX_*/WMX_* variables, so it can never reach the user's real gxserver.
param([Parameter(Mandatory)][string]$Root, [Parameter(ValueFromRemainingArguments)][string[]]$Rest)
Get-ChildItem env: | Where-Object { $_.Name -match '^(GHOSTEX_|WMX_|ZMX_|CLAUDE|CODEX_)' -or $_.Name -eq 'CLAUDECODE' } | ForEach-Object { Remove-Item "env:$($_.Name)" }
$env:GHOSTEX_HOME = "$Root\home"; $env:USERPROFILE = "$Root\home"; $env:HOME = "$Root\unixhome"; $env:CODEX_HOME = "$Root\home\.codex"
& "$Root\current\resources\native\ghostex.exe" @Rest
exit $LASTEXITCODE
