//! Going back: a small script, started detached, waits for Ghostex to quit, puts the kept app
//! where Ghostex runs from and starts it. If the swap can't finish, the version that was running
//! is put back and started, so a failed go back never leaves no Ghostex. What happened goes to
//! `go-back.log` in the logs folder.
//!
//! The caller quits right after `start` returns Ok: the message window (window.rs) or the app
//! (Settings > Updates, app/go_back_actions.rs).
use super::keep::kept_app_path;
use super::{GoBackHow, GoBackOffer, kept_dir, note_going_back};
use std::path::PathBuf;

const LOG_FILE: &str = "go-back.log";

fn log_path() -> PathBuf {
    crate::support_logs::logs_directory().join(LOG_FILE)
}

/// Starts going back to `offer.previous`. Only for `GoBackHow::Swap`; the other ways are the
/// caller's (copy the command, open the page).
pub(crate) fn start(offer: &GoBackOffer) -> Result<(), String> {
    if offer.how != GoBackHow::Swap {
        return Err("This computer can't go back by itself.".to_string());
    }
    let kept = kept_app_path().ok_or("The previous version is no longer on this computer.")?;
    let _ = std::fs::create_dir_all(crate::support_logs::logs_directory());
    spawn_swap(&kept)?;
    note_going_back(&offer.previous);
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::HostLifecycle,
        "goBack.started",
        serde_json::json!({ "from": offer.current, "to": offer.previous }),
    );
    Ok(())
}

/// macOS: `$1` Ghostex's pid, `$2` the running bundle, `$3` the kept bundle, `$4` the kept
/// folder, `$5` the log, `$6` the bundle id, `$7` the running build number to skip (may be
/// empty). The skip is Sparkle's own "Skip This Version" (SUSkippedVersion), so the version
/// gone back from isn't offered again until a newer one is out.
#[cfg(target_os = "macos")]
const SWAP_SCRIPT: &str = r#"
set -u
pid=$1 app=$2 kept=$3 keep_dir=$4 log=$5 bundle_id=$6 skip_build=$7
exec >>"$log" 2>&1
echo "$(date '+%F %T') going back: $kept -> $app"
waited=0
while kill -0 "$pid" 2>/dev/null; do
	sleep 0.2
	waited=$((waited + 1))
	[ "$waited" -lt 900 ] || { echo "Ghostex did not quit; nothing changed"; exit 1; }
done
aside="$keep_dir/going-back-from.app"
rm -rf "$aside"
if ! mv "$app" "$aside"; then
	echo "could not move the running app aside; nothing changed"
	open "$app"
	exit 1
fi
if ! mv "$kept" "$app"; then
	echo "could not put the kept app in place; putting the running one back"
	mv "$aside" "$app"
	open "$app"
	exit 1
fi
rm -f "$keep_dir/kept.json"
if [ -n "$bundle_id" ] && [ -n "$skip_build" ]; then
	defaults write "$bundle_id" SUSkippedVersion "$skip_build" || true
fi
echo "went back; starting $app"
open "$app"
rm -rf "$aside"
"#;

#[cfg(target_os = "macos")]
fn spawn_swap(kept: &std::path::Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt as _;
    let app = super::keep::running_app_bundle().ok_or("can't tell where Ghostex runs from")?;
    let bundle_id = bundle_identifier(&app).unwrap_or_default();
    // The build to skip is the running one: the kept file names the one going back to.
    let skip_build = running_build(&app).unwrap_or_default();
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(SWAP_SCRIPT)
        .arg("ghostex-go-back")
        .arg(std::process::id().to_string())
        .arg(&app)
        .arg(kept)
        .arg(kept_dir())
        .arg(log_path())
        .arg(bundle_id)
        .arg(skip_build)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid only detaches the child from this process's session; it allocates nothing.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start going back: {error}"))
}

#[cfg(target_os = "macos")]
fn plist_value(app: &std::path::Path, key: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !value.is_empty()).then_some(value)
}

#[cfg(target_os = "macos")]
fn bundle_identifier(app: &std::path::Path) -> Option<String> {
    plist_value(app, "CFBundleIdentifier")
}

#[cfg(target_os = "macos")]
fn running_build(app: &std::path::Path) -> Option<String> {
    plist_value(app, "CFBundleVersion")
}

/// Windows: `-ProcessId` Ghostex's pid, `-App` the folder it runs from (Velopack's `current`),
/// `-Kept` the kept folder, `-Log`. gxserver runs from inside `current` and outlives the app, so
/// the folder can't be renamed while it runs: it is asked to stop the way a newer app would stop
/// it, and ended if it doesn't. The relaunched app starts its own gxserver.
#[cfg(target_os = "windows")]
const SWAP_SCRIPT: &str = r#"
param([int]$ProcessId, [string]$App, [string]$Kept, [string]$Log)
function Note($text) { Add-Content -LiteralPath $Log -Value ("{0} {1}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $text) }
Note "going back: $Kept -> $App"
try { Wait-Process -Id $ProcessId -Timeout 180 -ErrorAction Stop } catch { if (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) { Note 'Ghostex did not quit; nothing changed'; exit 1 } }
$aside = "$App.going-back-from"
if (Test-Path -LiteralPath $aside) { Remove-Item -LiteralPath $aside -Recurse -Force -ErrorAction SilentlyContinue }
$moved = $false
for ($i = 0; $i -lt 40 -and -not $moved; $i++) {
  try { Rename-Item -LiteralPath $App -NewName (Split-Path -Leaf $aside) -ErrorAction Stop; $moved = $true }
  catch {
    if ($i -eq 10) {
      Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($App + '\', [StringComparison]::OrdinalIgnoreCase) } | ForEach-Object { Note "stopping $($_.Name) ($($_.Id))"; Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
    }
    Start-Sleep -Milliseconds 500
  }
}
if (-not $moved) { Note 'could not move the running app aside; nothing changed'; Start-Process -FilePath (Join-Path $App 'Ghostex.exe'); exit 1 }
try { Move-Item -LiteralPath $Kept -Destination $App -ErrorAction Stop }
catch { Note "could not put the kept app in place ($_); putting the running one back"; Rename-Item -LiteralPath $aside -NewName (Split-Path -Leaf $App); Start-Process -FilePath (Join-Path $App 'Ghostex.exe'); exit 1 }
Remove-Item -LiteralPath (Join-Path (Split-Path -Parent $Kept) 'kept.json') -Force -ErrorAction SilentlyContinue
Note "went back; starting $App"
Start-Process -FilePath (Join-Path $App 'Ghostex.exe')
Remove-Item -LiteralPath $aside -Recurse -Force -ErrorAction SilentlyContinue
"#;

#[cfg(target_os = "windows")]
fn spawn_swap(kept: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let app = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
        .ok_or("can't tell where Ghostex runs from")?;
    let script = kept_dir().join("go-back.ps1");
    std::fs::write(&script, SWAP_SCRIPT).map_err(|error| error.to_string())?;
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg("-ProcessId")
        .arg(std::process::id().to_string())
        .arg("-App")
        .arg(&app)
        .arg("-Kept")
        .arg(kept)
        .arg("-Log")
        .arg(log_path())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start going back: {error}"))
}

/// Linux AppImage: `$1` Ghostex's pid, `$2` the install copy, `$3` the kept copy, `$4` the kept
/// folder, `$5` the log. The AppImage launcher only copies a payload when its stamp differs from
/// the install copy's, so the app menu entry (which runs the install copy) opens the older
/// version; starting the newer AppImage file again installs that one again.
#[cfg(target_os = "linux")]
const SWAP_SCRIPT: &str = r#"
set -u
pid=$1 install=$2 kept=$3 keep_dir=$4 log=$5
exec >>"$log" 2>&1
echo "$(date '+%F %T') going back: $kept -> $install"
waited=0
while kill -0 "$pid" 2>/dev/null; do
	sleep 0.2
	waited=$((waited + 1))
	[ "$waited" -lt 900 ] || { echo "Ghostex did not quit; nothing changed"; exit 1; }
done
aside="$install.going-back-from"
rm -rf "$aside"
mv "$install" "$aside" || { echo "could not move the install copy aside; nothing changed"; exec "$install/Ghostex" >/dev/null 2>&1; }
if ! mv "$kept" "$install"; then
	echo "could not put the kept copy in place; putting the running one back"
	mv "$aside" "$install"
	exec "$install/Ghostex" >/dev/null 2>&1
fi
rm -f "$keep_dir/kept.json"
rm -rf "$aside"
echo "went back; starting $install"
exec "$install/Ghostex" >/dev/null 2>&1
"#;

#[cfg(target_os = "linux")]
fn spawn_swap(kept: &std::path::Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt as _;
    let install = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
        .ok_or("can't tell where Ghostex runs from")?;
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(SWAP_SCRIPT)
        .arg("ghostex-go-back")
        .arg(std::process::id().to_string())
        .arg(&install)
        .arg(kept)
        .arg(kept_dir())
        .arg(log_path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::linux_updater::prepare_relaunch_environment(&mut command);
    // SAFETY: setsid only detaches the child from this process's session; it allocates nothing.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start going back: {error}"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn spawn_swap(_kept: &std::path::Path) -> Result<(), String> {
    Err("Going back isn't available here.".to_string())
}
