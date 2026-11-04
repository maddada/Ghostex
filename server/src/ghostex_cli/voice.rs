//! `ghostex voice`: runs a voice-agent conversation or a Grok dictation against this computer's
//! gxserver without a client, from typed text or a WAV file, printing captions, tool calls, state
//! changes and announcements and saving the agent's audio. A developer tool for testing
//! `/api/voice` (docs/2026-10-20/voice-mode/voice-agent-client-contract.md); not in Ghostex Help.

use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest as _;
use tungstenite::{Message, WebSocket};

use super::rpc::{resolve_local_gxserver_target, CliError, CliResult};

const SAMPLE_RATE: u32 = 24_000;
/// Audio chunk sent per step: 40 ms of 24 kHz PCM16.
const CHUNK_BYTES: usize = 1_920;
const CHUNK_MS: u64 = 40;

const USAGE: &str = "Usage:
  ghostex voice --text \"What are my agents doing?\" [--text \"…\"]… [--provider xai|openai] [--save reply.wav] [--listen <seconds>]
  ghostex voice --wav question.wav [--wav next.wav]… [--save reply.wav] [--listen <seconds>]
  ghostex voice --dictate speech.wav [--language en]
  Testing extras: --barge-in talk.wav (spoken over the agent's first reply), --report-playback
  (answer an interruption with how much audio a real-time player had played), --detach (leave
  without hanging up and print the conversation id), --resume <conversationId> (reattach),
  --voice <id> (a turn that switches the agent's voice; place it between --text turns),
  --rate <1.0-2.0> (report a client playback rate, as the desktop's speed buttons do),
  --continue (a turn that hands the remembered last conversation to the agent, as Continue does),
  --mode pushToTalk|regular (the call mode for this call; push-to-talk sends each --wav as a held
  Talk and no audio in between), --wait <seconds> (a quiet turn: nothing is said or sent),
  --focus <projectId:sessionId> (a call about one session), --refocus <projectId:sessionId>|all
  (a turn that moves the call to a session or to all sessions), --client-remote (as the desktop
  app of another computer: sessions are shown by the client).
  Push-to-talk also prints [usage] with the seconds the provider has billed so far.

Runs a voice-agent conversation (or, with --dictate, a Grok dictation) against this computer's
gxserver and prints captions, tool calls and state changes. Each --text or --wav is one turn;
--listen keeps the conversation open that many seconds after the last turn (for announcements).
WAV files must be 16-bit mono at 24 kHz:  say -o q.wav --data-format=LEI16@24000 \"Hello\"";

struct Options {
    turns: Vec<Turn>,
    dictate: Option<String>,
    provider: Option<String>,
    language: Option<String>,
    save: Option<String>,
    listen: u64,
    turn_timeout: u64,
    barge_in: Option<String>,
    report_playback: bool,
    detach: bool,
    resume: Option<String>,
    rate: Option<f64>,
    push_to_talk: Option<bool>,
    /// `--focus <projectId:sessionId>`: a call about one session.
    focus: Option<String>,
    client_remote: bool,
}

enum Turn {
    Text(String),
    Wav(String),
    Voice(String),
    Continue,
    Wait(u64),
    /// A `focus` frame: move the call to a session (`Some(ref)`) or to all sessions.
    Refocus(Option<String>),
}

fn parse(args: &[String]) -> CliResult<Options> {
    let mut options = Options {
        turns: Vec::new(),
        dictate: None,
        provider: None,
        language: None,
        save: None,
        listen: 0,
        turn_timeout: 120,
        barge_in: None,
        report_playback: false,
        detach: false,
        resume: None,
        rate: None,
        push_to_talk: None,
        focus: None,
        client_remote: false,
    };
    let mut index = 0;
    let value = |index: usize, flag: &str| -> CliResult<String> {
        args.get(index + 1)
            .cloned()
            .ok_or_else(|| CliError::Other(format!("{flag} needs a value.\n\n{USAGE}")))
    };
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--text" => options.turns.push(Turn::Text(value(index, flag)?)),
            "--wav" => options.turns.push(Turn::Wav(value(index, flag)?)),
            "--voice" => options.turns.push(Turn::Voice(value(index, flag)?)),
            "--focus" => options.focus = Some(value(index, flag)?),
            "--refocus" => {
                let target = value(index, flag)?;
                options
                    .turns
                    .push(Turn::Refocus((target != "all").then_some(target)));
            }
            "--client-remote" => {
                options.client_remote = true;
                index += 1;
                continue;
            }
            "--mode" => {
                options.push_to_talk = match value(index, flag)?.as_str() {
                    "pushToTalk" => Some(true),
                    "regular" => Some(false),
                    _ => return Err(CliError::Other("--mode is pushToTalk or regular.".into())),
                }
            }
            "--wait" => options.turns.push(Turn::Wait(
                value(index, flag)?
                    .parse()
                    .map_err(|_| CliError::Other("--wait needs seconds.".into()))?,
            )),
            "--continue" => {
                options.turns.push(Turn::Continue);
                index += 1;
                continue;
            }
            "--rate" => {
                options.rate = Some(
                    value(index, flag)?
                        .parse()
                        .map_err(|_| CliError::Other("--rate needs a number.".into()))?,
                )
            }
            "--dictate" => options.dictate = Some(value(index, flag)?),
            "--provider" => options.provider = Some(value(index, flag)?),
            "--language" => options.language = Some(value(index, flag)?),
            "--save" => options.save = Some(value(index, flag)?),
            "--listen" => {
                options.listen = value(index, flag)?
                    .parse()
                    .map_err(|_| CliError::Other("--listen needs seconds.".into()))?
            }
            "--turn-timeout" => {
                options.turn_timeout = value(index, flag)?
                    .parse()
                    .map_err(|_| CliError::Other("--turn-timeout needs seconds.".into()))?
            }
            "--barge-in" => options.barge_in = Some(value(index, flag)?),
            "--resume" => options.resume = Some(value(index, flag)?),
            "--report-playback" => {
                options.report_playback = true;
                index += 1;
                continue;
            }
            "--detach" => {
                options.detach = true;
                index += 1;
                continue;
            }
            "-h" | "--help" | "help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => {
                return Err(CliError::Other(format!(
                    "Unknown option {other}.\n\n{USAGE}"
                )))
            }
        }
        index += 2;
    }
    if options.turns.is_empty()
        && options.dictate.is_none()
        && options.listen == 0
        && options.resume.is_none()
    {
        return Err(CliError::Other(USAGE.to_string()));
    }
    Ok(options)
}

pub(super) fn voice_command(args: &[String]) -> CliResult<()> {
    let options = parse(args)?;
    let mode = if options.dictate.is_some() {
        "dictation"
    } else {
        "conversation"
    };
    let mut socket = connect(mode, &options)?;
    if let Some(path) = &options.dictate {
        return dictate(&mut socket, path);
    }
    converse(&mut socket, &options)
}

fn connect(mode: &str, options: &Options) -> CliResult<WebSocket<TcpStream>> {
    let target = resolve_local_gxserver_target()?;
    let host = target.base_url.trim_start_matches("http://").to_string();
    let mut url = format!("ws://{host}/api/voice?protocolVersion=1&mode={mode}&client=cli");
    if let Some(provider) = &options.provider {
        url.push_str(&format!("&provider={provider}"));
    }
    if let Some(push_to_talk) = options.push_to_talk {
        let mode = if push_to_talk {
            "pushToTalk"
        } else {
            "regular"
        };
        url.push_str(&format!("&callMode={mode}"));
    }
    if let Some(language) = &options.language {
        url.push_str(&format!("&language={language}"));
    }
    if let Some(id) = &options.resume {
        url.push_str(&format!("&conversationId={id}"));
    }
    if let Some(focus) = &options.focus {
        url.push_str(&format!("&focus={focus}"));
    }
    if options.client_remote {
        url.push_str("&clientRemote=1");
    }
    let mut request = url
        .into_client_request()
        .map_err(|error| CliError::Other(error.to_string()))?;
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {}", target.token)
            .parse()
            .map_err(|_| CliError::Other("The gxserver token is not a valid header.".into()))?,
    );
    let stream = TcpStream::connect(&host).map_err(|error| {
        CliError::Connection(format!("Couldn't reach gxserver at {host}: {error}"))
    })?;
    let (socket, _) = tungstenite::client(request, stream)
        .map_err(|error| CliError::Connection(format!("The voice socket was refused: {error}")))?;
    socket
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(15)))
        .map_err(|error| CliError::Other(error.to_string()))?;
    Ok(socket)
}

/// One frame, or `None` when nothing arrived within the read timeout.
fn read(socket: &mut WebSocket<TcpStream>) -> CliResult<Option<Message>> {
    match socket.read() {
        Ok(message) => Ok(Some(message)),
        Err(tungstenite::Error::Io(error))
            if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
        {
            Ok(None)
        }
        Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
            Err(CliError::Other("The voice socket closed.".into()))
        }
        Err(error) => Err(CliError::Other(format!("The voice socket failed: {error}"))),
    }
}

fn send_json(socket: &mut WebSocket<TcpStream>, value: Value) -> CliResult<()> {
    socket
        .send(Message::Text(value.to_string().into()))
        .map_err(|error| CliError::Other(format!("The voice socket failed: {error}")))
}

fn send_audio(socket: &mut WebSocket<TcpStream>, chunk: &[u8]) -> CliResult<()> {
    socket
        .send(Message::Binary(chunk.to_vec().into()))
        .map_err(|error| CliError::Other(format!("The voice socket failed: {error}")))
}

/// The PCM samples of a 16-bit mono 24 kHz WAV file.
fn read_wav(path: &str) -> CliResult<Vec<u8>> {
    let bytes = std::fs::read(path)
        .map_err(|error| CliError::Other(format!("Couldn't read {path}: {error}")))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(CliError::Other(format!("{path} is not a WAV file.")));
    }
    let mut offset = 12;
    let mut format_ok = false;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let body = offset + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            let channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            let rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().unwrap());
            let bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
            if channels != 1 || rate != SAMPLE_RATE || bits != 16 {
                return Err(CliError::Other(format!(
                    "{path} is {channels} channel(s), {rate} Hz, {bits}-bit; it must be mono 24000 Hz 16-bit (say -o file.wav --data-format=LEI16@24000 \"…\")."
                )));
            }
            format_ok = true;
        } else if id == b"data" {
            if !format_ok {
                return Err(CliError::Other(format!(
                    "{path} has no format chunk before its data."
                )));
            }
            let end = (body + size).min(bytes.len());
            return Ok(bytes[body..end].to_vec());
        }
        offset = body + size + (size % 2);
    }
    Err(CliError::Other(format!("{path} has no audio data.")))
}

fn write_wav(path: &str, pcm: &[u8]) -> CliResult<()> {
    let mut out = Vec::with_capacity(pcm.len() + 44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(pcm);
    std::fs::write(path, out)
        .map_err(|error| CliError::Other(format!("Couldn't write {path}: {error}")))
}

fn elapsed(start: Instant) -> String {
    format!("{:>6.1}s", start.elapsed().as_secs_f64())
}

/// Where a conversation turn stands, from the frames printed so far.
#[derive(Default)]
struct TurnWatch {
    agent_spoke: bool,
    tools_running: usize,
    state: String,
    quiet_since: Option<Instant>,
    ended: bool,
    /// The agent reply being received: its id, when its audio started and how many bytes came.
    audio_item: Option<(String, Instant, usize)>,
    /// `--report-playback`: answer `interrupted` with the `playback` frame a real client sends.
    report_playback: bool,
    playback_report: Option<(String, u64)>,
    /// A `voice` answer arrived since the last `--voice` turn.
    voice_answered: bool,
    /// Push-to-talk: no microphone audio is sent except during a held `--wav` turn.
    push_to_talk: bool,
    /// `--rate`: how fast the simulated player plays the agent's audio.
    rate: f64,
}

impl TurnWatch {
    /// The agent answered, nothing runs, and it has been listening for a moment.
    fn done(&self) -> bool {
        self.agent_spoke
            && self.tools_running == 0
            && self.state == "listening"
            && self
                .quiet_since
                .is_some_and(|since| since.elapsed() >= Duration::from_millis(2_500))
    }
}

fn handle_frame(message: Message, start: Instant, watch: &mut TurnWatch, audio: &mut Vec<u8>) {
    match message {
        Message::Binary(bytes) => {
            audio.extend_from_slice(&bytes);
            watch.quiet_since = None;
            if let Some((_, _, received)) = &mut watch.audio_item {
                *received += bytes.len();
            }
        }
        Message::Text(text) => {
            let frame: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            let field = |key: &str| {
                frame
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let at = elapsed(start);
            match field("type").as_str() {
                "ready" if field("mode") == "dictation" => println!("{at}  [ready] dictation"),
                "ready" => {
                    // gxserver says which kind of call this is (the setting or `--mode`).
                    watch.push_to_talk = field("callMode") == "pushToTalk";
                    println!(
                        "{at}  [ready] conversation {} · {} · voice {} · {} call · resumed {} · memory {} · focus {}",
                        field("conversationId"),
                        field("provider"),
                        field("voice"),
                        field("callMode"),
                        frame["resumed"],
                        frame["memory"]["gist"],
                        frame.get("focus").map_or("(not supported)".to_string(), Value::to_string)
                    )
                }
                "focus" => println!("{at}  [focus] {}", frame["focus"]),
                "memory" => println!(
                    "{at}  [memory] {} {}",
                    field("state"),
                    frame
                        .get("memory")
                        .map(Value::to_string)
                        .unwrap_or_default()
                ),
                "state" => {
                    watch.state = field("state");
                    watch.quiet_since = (watch.state == "listening").then(Instant::now);
                    println!("{at}  [state] {}", watch.state);
                }
                "caption" => {
                    if frame.get("final").and_then(Value::as_bool) == Some(true) {
                        let who = if field("role") == "user" {
                            "You  "
                        } else {
                            "Agent"
                        };
                        if field("role") == "agent" {
                            watch.agent_spoke = true;
                        }
                        if frame.get("cut").and_then(Value::as_bool) == Some(true) {
                            println!("{at}  {who} (cut, as heard): {}", field("text"));
                        } else {
                            println!("{at}  {who}: {}", field("text"));
                        }
                    }
                }
                "tool" => {
                    if field("phase") == "started" {
                        watch.tools_running += 1;
                        println!("{at}  [tool] {} · {}", field("name"), field("label"));
                    } else {
                        watch.tools_running = watch.tools_running.saturating_sub(1);
                        println!(
                            "{at}  [tool] {} finished (ok: {})",
                            field("name"),
                            frame["ok"]
                        );
                    }
                }
                "announcement" => println!("{at}  [announcement] {}", field("text")),
                "interrupted" => {
                    // A real-time player (at `--rate`) has played the reply's audio since it started,
                    // up to what arrived.
                    let rate = if watch.rate > 0.0 { watch.rate } else { 1.0 };
                    let played = watch
                        .audio_item
                        .as_ref()
                        .filter(|(id, _, _)| *id == field("itemId"))
                        .map(|(_, since, received)| {
                            ((since.elapsed().as_secs_f64() * 1000.0 * rate) as u64)
                                .min((*received / 48) as u64)
                        });
                    match played {
                        Some(ms) if watch.report_playback => {
                            println!(
                                "{at}  [interrupted] {} (reporting {ms} ms played)",
                                field("itemId")
                            );
                            watch.playback_report = Some((field("itemId"), ms));
                        }
                        Some(ms) => println!(
                            "{at}  [interrupted] {} (about {ms} ms played)",
                            field("itemId")
                        ),
                        None => println!("{at}  [interrupted] {}", field("itemId")),
                    }
                }
                "error" => println!(
                    "{at}  [error] {}: {} (fatal: {})",
                    field("code"),
                    field("message"),
                    frame["fatal"]
                ),
                "ended" => {
                    watch.ended = true;
                    println!("{at}  [ended] {}: {}", field("reason"), field("message"));
                }
                "transcript" => println!(
                    "{at}  [dictation{}] {}",
                    if frame["final"] == true {
                        ", final"
                    } else {
                        ""
                    },
                    field("text")
                ),
                "done" => println!("{at}  [dictation done] {}", field("text")),
                "usage" => println!(
                    "{at}  [usage] {} s billed so far",
                    frame["billableAudioSeconds"]
                ),
                "voice" => {
                    watch.voice_answered = true;
                    println!(
                        "{at}  [voice] {} applied {} (speaking with {}{})",
                        field("voice"),
                        field("applied"),
                        field("current"),
                        match field("reason").as_str() {
                            "" => String::new(),
                            reason => format!("; {reason}"),
                        }
                    );
                }
                "audioStart" => watch.audio_item = Some((field("itemId"), Instant::now(), 0)),
                "audioDone" | "notice" => {}
                other => println!("{at}  [{other}] {text}"),
            }
        }
        Message::Close(_) => watch.ended = true,
        _ => {}
    }
}

fn send_playback_report(socket: &mut WebSocket<TcpStream>, watch: &mut TurnWatch) -> CliResult<()> {
    match watch.playback_report.take() {
        Some((item_id, played_ms)) => send_json(
            socket,
            json!({"type": "playback", "itemId": item_id, "playedMs": played_ms}),
        ),
        None => Ok(()),
    }
}

fn converse(socket: &mut WebSocket<TcpStream>, options: &Options) -> CliResult<()> {
    let start = Instant::now();
    let mut watch = TurnWatch {
        report_playback: options.report_playback,
        rate: options.rate.unwrap_or(1.0),
        ..TurnWatch::default()
    };
    let mut audio: Vec<u8> = Vec::new();
    let silence = vec![0u8; CHUNK_BYTES];
    // Wait for `ready`.
    let mut ready = false;
    while !ready && !watch.ended && start.elapsed() < Duration::from_secs(30) {
        if let Some(message) = read(socket)? {
            ready = matches!(&message, Message::Text(text) if text.contains("\"type\":\"ready\""));
            handle_frame(message, start, &mut watch, &mut audio);
        }
    }
    if !ready {
        return Err(CliError::Other(
            "The voice conversation did not start.".into(),
        ));
    }
    if let Some(rate) = options.rate {
        send_json(socket, json!({"type": "playbackRate", "rate": rate}))?;
    }
    let mut barge: Option<(Vec<u8>, usize)> = match &options.barge_in {
        Some(path) => Some((read_wav(path)?, 0)),
        None => None,
    };
    let mut speaking_since: Option<Instant> = None;
    let mut pump = |socket: &mut WebSocket<TcpStream>,
                    watch: &mut TurnWatch,
                    audio: &mut Vec<u8>,
                    until: &dyn Fn(&TurnWatch) -> bool,
                    deadline: Instant|
     -> CliResult<()> {
        // Keep sending silence so the provider's voice detection keeps running, and print frames;
        // a --barge-in file is spoken 1.5 s into the agent's first reply instead.
        let mut next_chunk = Instant::now();
        while !watch.ended && !until(watch) && Instant::now() < deadline {
            if Instant::now() >= next_chunk {
                speaking_since = match (watch.state == "speaking", speaking_since) {
                    (true, None) => Some(Instant::now()),
                    (true, since) => since,
                    (false, _) => None,
                };
                let barge_now = barge.as_ref().is_some_and(|(_, at)| *at > 0)
                    || speaking_since
                        .is_some_and(|since| since.elapsed() >= Duration::from_millis(1_500));
                match barge.as_mut().filter(|_| barge_now) {
                    Some((pcm, at)) => {
                        if *at == 0 {
                            println!("{}  (talking over the agent)", elapsed(start));
                        }
                        let end = (*at + CHUNK_BYTES).min(pcm.len());
                        send_audio(socket, &pcm[*at..end])?;
                        *at = end;
                        if end >= pcm.len() {
                            barge = None;
                        }
                    }
                    // Push-to-talk sends nothing while Talk is up, as the apps do.
                    None if watch.push_to_talk => {}
                    None => send_audio(socket, &silence)?,
                }
                next_chunk += Duration::from_millis(CHUNK_MS);
            }
            while let Some(message) = read(socket)? {
                handle_frame(message, start, watch, audio);
                send_playback_report(socket, watch)?;
            }
        }
        Ok(())
    };
    for turn in &options.turns {
        watch.agent_spoke = false;
        watch.quiet_since = None;
        if let Turn::Wait(seconds) = turn {
            println!("{}  (quiet for {seconds}s)", elapsed(start));
            let deadline = Instant::now() + Duration::from_secs(*seconds);
            pump(
                socket,
                &mut watch,
                &mut audio,
                &|_: &TurnWatch| false,
                deadline,
            )?;
            continue;
        }
        if let Turn::Voice(voice) = turn {
            println!("{}  (switching to the voice {voice})", elapsed(start));
            watch.voice_answered = false;
            send_json(socket, json!({"type": "voice", "voice": voice}))?;
            let deadline = Instant::now() + Duration::from_secs(10);
            pump(
                socket,
                &mut watch,
                &mut audio,
                &|watch: &TurnWatch| watch.voice_answered,
                deadline,
            )?;
            continue;
        }
        match turn {
            Turn::Text(text) => {
                println!("{}  (typed) {text}", elapsed(start));
                send_json(socket, json!({"type": "text", "text": text}))?;
            }
            Turn::Wav(path) => {
                let pcm = read_wav(path)?;
                println!(
                    "{}  (speaking {path}, {:.1}s)",
                    elapsed(start),
                    pcm.len() as f64 / (SAMPLE_RATE as f64 * 2.0)
                );
                if watch.push_to_talk {
                    send_json(socket, json!({"type": "talkStart"}))?;
                }
                let mut next_chunk = Instant::now();
                for chunk in pcm.chunks(CHUNK_BYTES) {
                    while Instant::now() < next_chunk {
                        while let Some(message) = read(socket)? {
                            handle_frame(message, start, &mut watch, &mut audio);
                            send_playback_report(socket, &mut watch)?;
                        }
                    }
                    send_audio(socket, chunk)?;
                    next_chunk += Duration::from_millis(CHUNK_MS);
                }
                if watch.push_to_talk {
                    send_json(socket, json!({"type": "talkEnd"}))?;
                }
            }
            Turn::Continue => {
                println!("{}  (continue the last conversation)", elapsed(start));
                send_json(socket, json!({"type": "continueMemory"}))?;
            }
            Turn::Refocus(target) => {
                println!(
                    "{}  (move the call to {})",
                    elapsed(start),
                    target.as_deref().unwrap_or("all sessions")
                );
                send_json(socket, json!({"type": "focus", "session": target}))?;
            }
            Turn::Voice(_) | Turn::Wait(_) => {}
        }
        let deadline = Instant::now() + Duration::from_secs(options.turn_timeout);
        pump(
            socket,
            &mut watch,
            &mut audio,
            &|watch: &TurnWatch| watch.done(),
            deadline,
        )?;
        if watch.ended {
            break;
        }
        if !watch.done() {
            println!(
                "{}  (turn timed out after {}s)",
                elapsed(start),
                options.turn_timeout
            );
        }
    }
    if options.listen > 0 && !watch.ended {
        println!("{}  (listening for {}s)", elapsed(start), options.listen);
        let deadline = Instant::now() + Duration::from_secs(options.listen);
        pump(
            socket,
            &mut watch,
            &mut audio,
            &|_: &TurnWatch| false,
            deadline,
        )?;
    }
    if options.detach && !watch.ended {
        println!(
            "{}  (leaving without hanging up; reattach with --resume <conversationId>)",
            elapsed(start)
        );
        let _ = socket.close(None);
        let _ = socket.flush();
    } else if !watch.ended {
        send_json(socket, json!({"type": "end"}))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !watch.ended && Instant::now() < deadline {
            match read(socket) {
                Ok(Some(message)) => handle_frame(message, start, &mut watch, &mut audio),
                Ok(None) => {}
                Err(_) => break,
            }
        }
    }
    if let Some(path) = &options.save {
        write_wav(path, &audio)?;
        println!(
            "Saved {:.1}s of agent audio to {path}",
            audio.len() as f64 / (SAMPLE_RATE as f64 * 2.0)
        );
    }
    Ok(())
}

fn dictate(socket: &mut WebSocket<TcpStream>, path: &str) -> CliResult<()> {
    let start = Instant::now();
    let mut watch = TurnWatch::default();
    let mut audio = Vec::new();
    let pcm = read_wav(path)?;
    let mut finished = false;
    let track = |message: Message, watch: &mut TurnWatch, audio: &mut Vec<u8>| {
        let done = matches!(&message, Message::Text(text) if text.contains("\"type\":\"done\""));
        handle_frame(message, start, watch, audio);
        done
    };
    let mut next_chunk = Instant::now();
    for chunk in pcm.chunks(CHUNK_BYTES) {
        while Instant::now() < next_chunk {
            while let Some(message) = read(socket)? {
                finished |= track(message, &mut watch, &mut audio);
            }
        }
        send_audio(socket, chunk)?;
        next_chunk += Duration::from_millis(CHUNK_MS);
    }
    send_json(socket, json!({"type": "end"}))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !finished && !watch.ended && Instant::now() < deadline {
        match read(socket) {
            Ok(Some(message)) => finished |= track(message, &mut watch, &mut audio),
            Ok(None) => {}
            Err(_) => break,
        }
    }
    Ok(())
}
