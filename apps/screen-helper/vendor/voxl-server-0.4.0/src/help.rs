//! `--help` and `--version`: printed instead of starting the server.

const USAGE: &str = "
Usage: voxl [--flag=value]...

Streams this machine's desktop to a browser at http://ADDR:PORT/ as H.264 over a
WebSocket, and prints a link that signs you in. Flags take --name=value or
--name value; true/false flags also work on their own (--read-only).

Server:
  --addr, --port                listen address (default ADDR_DEFAULT, 6901)
  --title                       the page's title (default: Voxl Computer Stream)
  --favicon                     the page's icon: an .ico, .png, .gif, .webp or .svg
                                file (default: voxl's own)
  --allowed-hosts               host names to answer for besides IP addresses and
                                localhost (.example.com covers subdomains, * any)
  --allowed-origins             other sites whose pages may connect (* any)
  --tls                         serve HTTPS with a self-signed certificate, made once
                                and kept (VOXL_TLS_DIR, else %LOCALAPPDATA%\\voxl or
                                ~/.config/voxl)
  --tls-cert, --tls-key         serve HTTPS with this PEM certificate chain and key

Login:
  --enable-basic-auth           ask for a password (default true)
  --basic-auth-password         the password (default: $VOXL_PASSWORD, else one is
                                made up and printed at startup)
  --basic-auth-user             the user name a browser's sign-in has to give
                                (default: any)

Video:
  --framerate                   max frames per second (default 60)
  --crf                         base quality (default 24)
  --max-kbps, --min-kbps        adaptive bitrate ceiling and floor
                                (default 30000, 2000)
  --preset                      x264 preset (default ultrafast)
  --encoder-threads             x264 sliced threads (default min(4, cores/4))
  --adaptive-threads            encode on one thread, fanning out only under
                                sustained heavy motion (default false)
  --paintover-crf, --paintover-burst, --paintover-delay-ms
                                sharpening once the screen goes still
                                (default 16, 3, 150)
  --stripes                     encode the screen as N independent streams
                                (experimental, default 1)
  --convert-threads, --coalesce-us
                                tuning (default 4, 0)
";

#[cfg(not(any(target_os = "macos", windows)))]
const DESKTOP: &str = "
Desktop (the X11 display in $DISPLAY):
  --enable-clipboard            true, in, out or false (default true)
  --audio-enabled               desktop audio (default true)
  --audio-device-name           PulseAudio source (default output.monitor)
  --audio-bitrate               Opus bitrate (default 128000)
  --microphone-enabled          the browser's microphone into the desktop
                                (default false)
  --microphone-fifo             FIFO a PulseAudio pipe source reads
                                (default /tmp/voxl-mic.fifo)
  --enable-resize               the desktop follows the viewer's window
                                (default false)
  --resize-scale                desktop pixels per CSS pixel (default 1.5)
  --read-only                   viewers only watch (default false)
  --scroll-px-per-notch         desktop pixels one wheel notch scrolls (default 180)
  --round-corners               corner radius for top-level windows (default 0)
  --round-windows               title:radius,... rounds windows with that title
";

#[cfg(target_os = "macos")]
const DESKTOP: &str = "
Desktop (this Mac's screen):
  --display                     display to stream: an index (0 is the main
                                display) or a display ID
  --capture-scale               streamed pixels per point (default: native,
                                within the budget)
  --max-capture-pixels          pixel budget for the automatic scale
                                (default 4096000)
  --enable-clipboard            true, in, out or false (default true)
  --audio-enabled               system audio (default true)
  --audio-bitrate               Opus bitrate (default 128000)
  --microphone-enabled          the browser's microphone into the Mac
                                (default false)
  --microphone-device           output device to play it into (default: the
                                first loopback device found)
  --enable-resize               the display follows the viewer's window
                                (default false)
  --resize-scale                display pixels per CSS pixel (default: the
                                capture scale)
  --read-only                   viewers only watch (default false)
  --show-cursor                 include the host cursor in the video stream
                                (default: true with --read-only, else false)
  --scroll-px-per-notch         pixels one wheel notch scrolls (default 180)
";

#[cfg(windows)]
const DESKTOP: &str = "
Desktop (this PC's screen):
  --display                     display to stream, by index (0 is the primary)
  --max-capture-pixels          stream a smaller copy of a display with more
                                pixels than this (default 0: its own size)
  --enable-clipboard            true, in, out or false (default true)
  --audio-enabled               system audio (default true)
  --audio-device-name           output device whose sound to stream
                                (default: the default output device)
  --audio-bitrate               Opus bitrate (default 128000)
  --microphone-enabled          the browser's microphone into the PC
                                (default false)
  --microphone-device           output device to play it into (default: the
                                first virtual cable found)
  --enable-resize               the display follows the viewer's window
                                (default false)
  --resize-scale                display pixels per CSS pixel (default: the
                                display's scaling)
  --read-only                   viewers only watch (default false)
  --scroll-px-per-notch         pixels one wheel notch scrolls (default 180)
";

const FILES: &str = "
Files and recordings:
  --file-transfers              upload, download, upload,download or none
                                (default upload,download)
  --files-root                  folder the Files panel can see (default $HOME)
  --upload-dir                  folder under the root that uploads go to
                                (default Downloads)
  --max-upload-mb               largest upload, 0 for no limit (default 0)
  --max-record-seconds          longest recording (default 300)

  -h, --help                    print this and exit
  -V, --version                 print the version and exit

Agents and scripts can use GET /screenshot, /record and /wait; see
https://github.com/tembo/voxl#for-agents-and-scripts
";

/// Which of the two the arguments ask for, if either. Help wins over version.
#[derive(Debug, PartialEq)]
enum Request {
    Help,
    Version,
}

fn request(args: &[String]) -> Option<Request> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        Some(Request::Help)
    } else if args.iter().any(|a| a == "--version" || a == "-V") {
        Some(Request::Version)
    } else {
        None
    }
}

/// Prints the help or the version if the arguments ask for either, and says whether it did.
pub fn print_if_asked(args: &[String]) -> bool {
    let version = env!("CARGO_PKG_VERSION");
    match request(args) {
        Some(Request::Help) => {
            // A Mac's or a PC's own screen is only served locally by default (see serve()).
            let addr = if cfg!(target_os = "linux") {
                "0.0.0.0"
            } else {
                "127.0.0.1"
            };
            print!(
                "voxl {version}: low-latency desktop streaming for the browser.\n{}{DESKTOP}{FILES}",
                USAGE.replace("ADDR_DEFAULT", addr)
            );
            true
        }
        Some(Request::Version) => {
            println!("voxl {version}");
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn help_and_version_flags_are_recognized_anywhere() {
        assert_eq!(request(&args(&["--help"])), Some(Request::Help));
        assert_eq!(request(&args(&["--port=6901", "-h"])), Some(Request::Help));
        assert_eq!(request(&args(&["-V"])), Some(Request::Version));
        assert_eq!(
            request(&args(&["--version", "--port", "6901"])),
            Some(Request::Version)
        );
        assert_eq!(
            request(&args(&["--version", "--help"])),
            Some(Request::Help)
        );
    }

    #[test]
    fn other_flags_start_the_server() {
        assert_eq!(request(&args(&[])), None);
        assert_eq!(
            request(&args(&["--port=6901", "--read-only", "--title", "help"])),
            None
        );
    }
}
