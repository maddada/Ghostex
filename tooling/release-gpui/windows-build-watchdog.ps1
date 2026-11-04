# CDXC:Release 2026-10-07 WHY:
# Runs one -Phase of tooling/release-gpui/windows.ps1 as a watched child process
# and restarts it once when it goes silent (no stdout or stderr) for
# GHOSTEX_WINDOWS_BUILD_SILENCE_SECONDS (default 900). Silence, not elapsed time,
# is the wedge signal; a non-zero exit fails on the first attempt. The rationale
# lives beside the callers in .github/workflows/release-gpui-windows.yml. It was
# written inline to RUNNER_TEMP by that workflow's build job until the native
# runtime compile moved to its own job, which needs the same watchdog.
param(
    [Parameter(Mandatory = $true)][string]$Phase,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Arch
)
$ErrorActionPreference = 'Stop'

$SilenceLimitSeconds = [int]$env:GHOSTEX_WINDOWS_BUILD_SILENCE_SECONDS
if ($SilenceLimitSeconds -le 0) { $SilenceLimitSeconds = 900 }
$PollSeconds = 10
$MaxAttempts = 2

$BuildScript = Join-Path $env:GITHUB_WORKSPACE 'tooling/release-gpui/windows.ps1'
$ExitCode = $null

for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    if ($attempt -gt 1) {
        Write-Host "::notice::Windows $Arch ${Phase}: restarting from scratch (attempt $attempt of $MaxAttempts) after the wedged-runner watchdog killed attempt $($attempt - 1)"
    }
    Write-Host "::group::Windows $Arch $Phase attempt $attempt of $MaxAttempts (stdout-silence watchdog: ${SilenceLimitSeconds}s)"

    $stdoutPath = Join-Path $env:RUNNER_TEMP "ghostex-windows-$Phase-$attempt.out.log"
    $stderrPath = Join-Path $env:RUNNER_TEMP "ghostex-windows-$Phase-$attempt.err.log"
    foreach ($logPath in @($stdoutPath, $stderrPath)) {
        Set-Content -LiteralPath $logPath -Value '' -NoNewline -Encoding utf8
    }
    $offsets = @{ $stdoutPath = [int64]0; $stderrPath = [int64]0 }

    # No -UseNewEnvironment: the child inherits this step's full
    # environment block, including PATH (kache, cargo, zig) and the
    # RUSTC_WRAPPER/KACHE_* variables kache-action exported.
    $proc = Start-Process -FilePath 'pwsh' -PassThru -NoNewWindow `
        -WorkingDirectory $env:GITHUB_WORKSPACE `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath `
        -ArgumentList @('-NoProfile', '-NonInteractive', '-File', $BuildScript, '-Version', $Version, '-Arch', $Arch, '-Phase', $Phase)

    $lastOutput = Get-Date
    $wedged = $false

    while ($true) {
        Start-Sleep -Seconds $PollSeconds
        # Sample the exit state BEFORE draining, so the final drain
        # after the child exits cannot miss its last bytes.
        $exited = $proc.HasExited
        $sawOutput = $false
        foreach ($logPath in @($stdoutPath, $stderrPath)) {
            $length = (Get-Item -LiteralPath $logPath).Length
            if ($length -le $offsets[$logPath]) { continue }
            $stream = [IO.File]::Open($logPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
            try {
                $stream.Seek($offsets[$logPath], [IO.SeekOrigin]::Begin) | Out-Null
                $buffer = [byte[]]::new([int]($length - $offsets[$logPath]))
                $read = $stream.Read($buffer, 0, $buffer.Length)
            }
            finally {
                $stream.Dispose()
            }
            if ($read -le 0) { continue }
            $offsets[$logPath] += $read
            [Console]::Out.Write([Text.Encoding]::UTF8.GetString($buffer, 0, $read))
            [Console]::Out.Flush()
            $sawOutput = $true
        }
        if ($sawOutput) { $lastOutput = Get-Date }
        if ($exited) { break }

        $silentFor = (Get-Date) - $lastOutput
        if ($silentFor.TotalSeconds -ge $SilenceLimitSeconds) {
            $wedged = $true
            $silenceLabel = '{0}m{1}s' -f [int]$silentFor.TotalMinutes, $silentFor.Seconds
            $nextAction = if ($attempt -lt $MaxAttempts) { 'killing and retrying once' } else { 'killing and failing the job' }
            Write-Host "::error::Windows $Arch $Phase (attempt $attempt of $MaxAttempts): no stdout or stderr for $silenceLabel - assuming wedged runner, $nextAction"
            & taskkill.exe /T /F /PID $proc.Id 2>&1 | ForEach-Object { Write-Host $_ }
            if (-not $proc.WaitForExit(60000)) {
                Write-Host "::warning::Windows $Arch ${Phase}: the wedged process tree did not exit within 60s of taskkill"
            }
            break
        }
    }

    Write-Host '::endgroup::'

    if (-not $wedged) {
        $ExitCode = $proc.ExitCode
        break
    }
    if ($attempt -ge $MaxAttempts) {
        throw "Windows $Arch $Phase wedged $MaxAttempts times in a row (no output for ${SilenceLimitSeconds}s each time). This is not a slow build and not a source problem - the runner instance is stuck. Failing instead of retrying again."
    }
    if (-not [string]::IsNullOrEmpty($env:GITHUB_STEP_SUMMARY)) {
        Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Value "- Wedged-runner watchdog fired on $Phase attempt $attempt (no output for ${SilenceLimitSeconds}s); rebuilt that phase from scratch"
    }
    Start-Sleep -Seconds 15
}

if ($ExitCode -ne 0) {
    throw "Windows $Arch $Phase failed with exit code $ExitCode"
}
