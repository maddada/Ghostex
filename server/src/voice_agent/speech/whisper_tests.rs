//! Whisper's front end and vocabulary, and (ignored by default) a real local transcription: the
//! model downloads through the same code Settings uses and transcribes a recorded sentence.

use std::time::{Duration, Instant};

use serde_json::json;

use super::whisper::{mel_filters, split_at_quiet, Tokenizer};

/// A vocabulary of single bytes plus a few merges, shaped like Hugging Face's tokenizer.json.
fn tiny_tokenizer() -> Tokenizer {
    let mut vocab = serde_json::Map::new();
    let mut next = 0u32;
    let mut add = |piece: &str| {
        vocab.insert(piece.to_string(), json!(next));
        next += 1;
    };
    for c in "abcdefghijklmnopqrstuvwxyzGOS".chars() {
        add(&c.to_string());
    }
    add("Ġ"); // a space
    add("Ġg");
    add("ho");
    add("Ġgho");
    let json = json!({
        "model": {"type": "BPE", "vocab": vocab, "merges": ["Ġ g", "h o", "Ġg ho"]},
        "added_tokens": [{"id": 50257, "content": "<|endoftext|>"}],
    });
    Tokenizer::from_json(json.to_string().as_bytes()).expect("tokenizer")
}

#[test]
fn text_round_trips_through_the_byte_level_vocabulary() {
    let tokenizer = tiny_tokenizer();
    let tokens = tokenizer.encode(" ghost");
    assert_eq!(
        tokens[0],
        tokenizer.token("Ġgho").expect("merged"),
        "{tokens:?}"
    );
    assert_eq!(tokenizer.decode(&tokens), " ghost");
    let end = tokenizer.token("<|endoftext|>").expect("eot");
    assert_eq!(end, 50257);
    assert_eq!(
        tokenizer.decode(&[tokens[0], end]),
        " gho",
        "special tokens are skipped"
    );
}

#[test]
fn the_mel_filterbank_matches_whispers_shape() {
    for mels in [80, 128] {
        let filters = mel_filters(mels);
        assert_eq!(filters.len(), mels * 201);
        assert!(filters.iter().all(|w| *w >= 0.0 && w.is_finite()));
        for row in filters.chunks(201) {
            assert!(row.iter().any(|w| *w > 0.0), "every band hears something");
        }
    }
    // librosa.filters.mel(sr=16000, n_fft=400, n_mels=80)[0][1], the band Whisper's front end
    // was trained with.
    let filters = mel_filters(80);
    assert!((filters[1] - 0.02486).abs() < 0.0002, "{}", filters[1]);
}

#[test]
fn long_audio_is_cut_at_quiet_moments_into_windows_of_30_seconds_at_most() {
    let rate = 16_000;
    let mut samples = vec![0.5f32; 70 * rate];
    // Quiet at 27 s and at 55 s.
    for s in &mut samples[27 * rate..27 * rate + rate / 5] {
        *s = 0.0;
    }
    for s in &mut samples[55 * rate..55 * rate + rate / 5] {
        *s = 0.0;
    }
    let pieces = split_at_quiet(&samples);
    assert!(pieces.iter().all(|piece| piece.len() <= 30 * rate));
    assert_eq!(
        pieces.iter().map(|piece| piece.len()).sum::<usize>(),
        samples.len()
    );
    let first = pieces[0].len() as f32 / rate as f32;
    assert!((27.0..27.3).contains(&first), "cut in the quiet: {first}");
    assert_eq!(split_at_quiet(&samples[..rate]).len(), 1);
}

/// `GHOSTEX_DICTATE_TEST_HOME` is a folder that keeps the downloaded model between runs (never
/// /tmp), `GHOSTEX_DICTATE_TEST_WAV` a recording, `GHOSTEX_DICTATE_TEST_EXPECT` comma-separated
/// words it must contain, and `GHOSTEX_DICTATE_TEST_MODEL` the model (Whisper base by default).
#[test]
#[ignore = "downloads a model; run with GHOSTEX_DICTATE_TEST_HOME and GHOSTEX_DICTATE_TEST_WAV"]
fn a_downloaded_model_transcribes_a_real_recording() {
    let home = std::env::var("GHOSTEX_DICTATE_TEST_HOME").expect("GHOSTEX_DICTATE_TEST_HOME");
    let wav = std::env::var("GHOSTEX_DICTATE_TEST_WAV").expect("GHOSTEX_DICTATE_TEST_WAV");
    let id =
        std::env::var("GHOSTEX_DICTATE_TEST_MODEL").unwrap_or_else(|_| "whisper-base".to_string());
    let expect = std::env::var("GHOSTEX_DICTATE_TEST_EXPECT").unwrap_or_default();
    let paths = crate::paths::get_gxserver_paths(Some(home.into()));
    let spec = super::catalog::model(&id).expect("a model id");

    let started = Instant::now();
    let (done, finished) = std::sync::mpsc::channel();
    super::models::start_download(&paths, spec, move |result| {
        let _ = done.send(result);
    });
    if !super::models::is_ready(&paths, spec) {
        finished
            .recv_timeout(Duration::from_secs(60 * 60))
            .expect("the download ended")
            .expect("the download worked");
    }
    eprintln!("{id}: ready after {:.1} s", started.elapsed().as_secs_f64());
    assert!(super::models::is_ready(&paths, spec));
    let disk = super::models::disk_bytes(&super::models::model_dir(&paths, spec.id));
    eprintln!("{id}: {:.0} MB on disk", disk as f64 / 1e6);

    let words = vec!["Ghostex".to_string(), "gxserver".to_string()];
    let aliases = vec![("gxserver".to_string(), "GX server".to_string())];
    let hints = super::engine::Hints {
        language: Some("en"),
        words: &words,
        aliases: &aliases,
    };
    let audio = std::path::Path::new(&wav);
    let seconds = super::engine::read_audio(audio).expect("audio").len() as f64 / 16_000.0;
    for run in ["cold (loads the model)", "warm"] {
        let started = Instant::now();
        let text = super::engine::transcribe(
            spec,
            &super::models::model_dir(&paths, spec.id),
            audio,
            &hints,
        )
        .expect("transcribed");
        eprintln!(
            "{id} {run}: {:.2} s for {seconds:.1} s of audio: {text:?}",
            started.elapsed().as_secs_f64()
        );
        let lower = text.to_lowercase();
        for word in expect
            .split(',')
            .map(str::trim)
            .filter(|word| !word.is_empty())
        {
            assert!(
                lower.contains(&word.to_lowercase()),
                "{word:?} missing from {text:?}"
            );
        }
    }
}
