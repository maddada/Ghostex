//! Whisper (base and large-v3-turbo) on the CPU with candle, in pure Rust: a downloaded
//! `model.safetensors` is converted once to 8-bit weights (`model-q8_0.gguf`), loaded with
//! candle's quantized Whisper, and decoded greedily in 30-second windows cut at quiet moments.
//!
//! CDXC:Dictate 2026-11-05 WHY:
//! gxserver ships as a static musl binary on Linux and links the static C runtime on Windows; ONNX Runtime's prebuilt libraries exist for neither (pyke's Windows `onnxruntime.lib` is built `MD_DynamicRelease`, and there is no musl build), and whisper.cpp is C++ that the musl build can't link. candle is pure Rust and builds for every release target, so Whisper runs on it. 8-bit weights keep turbo at about 0.9 GB in memory instead of 3.2 GB as f32, at no audible cost in accuracy.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use candle_core::quantized::{gguf_file, GgmlDType, QTensor};
use candle_core::{DType, Device, IndexOp, Tensor, D};
use candle_transformers::models::whisper::{self as m, quantized_model, Config};
use candle_transformers::quantized_var_builder::VarBuilder;
use serde_json::Value;

pub(super) const WEIGHTS_FILE: &str = "model-q8_0.gguf";
const SOURCE_FILE: &str = "model.safetensors";

/// Converts the downloaded `model.safetensors` in `dir` to `model-q8_0.gguf` and removes it.
/// Matrices whose rows divide into 32-value blocks become Q8_0; everything else stays f32.
pub(super) fn prepare(dir: &Path) -> Result<(), String> {
    let source = dir.join(SOURCE_FILE);
    let failed =
        |error: candle_core::Error| format!("Couldn't prepare the Whisper weights: {error}");
    // SAFETY: the file is ours, fully downloaded and verified, and nothing writes it while mapped.
    let tensors =
        unsafe { candle_core::safetensors::MmapedSafetensors::new(&source) }.map_err(failed)?;
    let mut quantized: Vec<(String, QTensor)> = Vec::new();
    for (name, _) in tensors.tensors() {
        let tensor = tensors
            .load(&name, &Device::Cpu)
            .and_then(|tensor| tensor.to_dtype(DType::F32))
            .map_err(failed)?;
        let dims = tensor.dims().to_vec();
        let dtype = if dims.len() == 2 && dims[1] % 32 == 0 {
            GgmlDType::Q8_0
        } else {
            GgmlDType::F32
        };
        quantized.push((name, QTensor::quantize(&tensor, dtype).map_err(failed)?));
    }
    drop(tensors);
    let target = dir.join(WEIGHTS_FILE);
    let partial = dir.join(format!("{WEIGHTS_FILE}.partial"));
    let mut file = fs::File::create(&partial).map_err(|error| error.to_string())?;
    let refs: Vec<(&str, &QTensor)> = quantized
        .iter()
        .map(|(name, tensor)| (name.as_str(), tensor))
        .collect();
    gguf_file::write(&mut file, &[], &refs).map_err(failed)?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);
    fs::rename(&partial, &target).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(source);
    Ok(())
}

/// Whisper's byte-level BPE vocabulary: enough to read its output and to write a prompt.
pub(super) struct Tokenizer {
    pieces: Vec<String>,
    ids: HashMap<String, u32>,
    ranks: HashMap<(String, String), usize>,
    byte_chars: [char; 256],
}

impl Tokenizer {
    pub(super) fn from_json(bytes: &[u8]) -> Result<Tokenizer, String> {
        let root: Value =
            serde_json::from_slice(bytes).map_err(|error| format!("tokenizer.json: {error}"))?;
        let mut ids: HashMap<String, u32> = HashMap::new();
        if let Some(vocab) = root.pointer("/model/vocab").and_then(Value::as_object) {
            for (piece, id) in vocab {
                if let Some(id) = id.as_u64() {
                    ids.insert(piece.clone(), id as u32);
                }
            }
        }
        for token in root
            .get("added_tokens")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(content), Some(id)) = (
                token.get("content").and_then(Value::as_str),
                token.get("id").and_then(Value::as_u64),
            ) {
                ids.insert(content.to_string(), id as u32);
            }
        }
        if ids.is_empty() {
            return Err("tokenizer.json has no vocabulary.".to_string());
        }
        let size = ids.values().copied().max().unwrap_or(0) as usize + 1;
        let mut pieces = vec![String::new(); size];
        for (piece, id) in &ids {
            pieces[*id as usize] = piece.clone();
        }
        let mut ranks = HashMap::new();
        for (rank, merge) in root
            .pointer("/model/merges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let pair = match merge {
                Value::String(text) => text
                    .split_once(' ')
                    .map(|(a, b)| (a.to_string(), b.to_string())),
                Value::Array(parts) if parts.len() == 2 => Some((
                    parts[0].as_str().unwrap_or_default().to_string(),
                    parts[1].as_str().unwrap_or_default().to_string(),
                )),
                _ => None,
            };
            if let Some(pair) = pair {
                ranks.insert(pair, rank);
            }
        }
        Ok(Tokenizer {
            pieces,
            ids,
            ranks,
            byte_chars: byte_chars(),
        })
    }

    pub(super) fn token(&self, piece: &str) -> Option<u32> {
        self.ids.get(piece).copied()
    }

    /// The text of ordinary tokens (special ones are skipped).
    pub(super) fn decode(&self, tokens: &[u32]) -> String {
        let mut reverse = HashMap::with_capacity(256);
        for (byte, c) in self.byte_chars.iter().enumerate() {
            reverse.insert(*c, byte as u8);
        }
        let mut bytes = Vec::new();
        for token in tokens {
            let Some(piece) = self.pieces.get(*token as usize) else {
                continue;
            };
            if piece.starts_with("<|") && piece.ends_with("|>") {
                continue;
            }
            for c in piece.chars() {
                if let Some(byte) = reverse.get(&c) {
                    bytes.push(*byte);
                }
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Byte-level BPE of `text`, split GPT-2's way into runs of letters, digits and other
    /// characters, each with the space before it.
    pub(super) fn encode(&self, text: &str) -> Vec<u32> {
        let mut tokens = Vec::new();
        for word in pre_tokenize(text) {
            let mut parts: Vec<String> = word
                .bytes()
                .map(|byte| self.byte_chars[byte as usize].to_string())
                .collect();
            loop {
                let best = parts
                    .windows(2)
                    .enumerate()
                    .filter_map(|(index, pair)| {
                        self.ranks
                            .get(&(pair[0].clone(), pair[1].clone()))
                            .map(|rank| (*rank, index))
                    })
                    .min();
                let Some((_, index)) = best else { break };
                let merged = format!("{}{}", parts[index], parts[index + 1]);
                parts.splice(index..index + 2, [merged]);
            }
            tokens.extend(parts.iter().filter_map(|part| self.token(part)));
        }
        tokens
    }
}

/// GPT-2's map of bytes to printable characters.
fn byte_chars() -> [char; 256] {
    let mut chars = ['\0'; 256];
    let mut next = 256u32;
    for byte in 0..=255u32 {
        let printable = (33..=126).contains(&byte)
            || (161..=172).contains(&byte)
            || (174..=255).contains(&byte);
        chars[byte as usize] = if printable {
            char::from_u32(byte).unwrap_or('?')
        } else {
            let c = char::from_u32(next).unwrap_or('?');
            next += 1;
            c
        };
    }
    chars
}

fn pre_tokenize(text: &str) -> Vec<String> {
    #[derive(PartialEq, Clone, Copy)]
    enum Class {
        Letter,
        Digit,
        Other,
    }
    let class = |c: char| {
        if c.is_alphabetic() {
            Class::Letter
        } else if c.is_numeric() {
            Class::Digit
        } else {
            Class::Other
        }
    };
    let mut words = Vec::new();
    let mut current = String::new();
    let mut current_class = None;
    for c in text.chars() {
        if c.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(' ');
            current_class = None;
            continue;
        }
        let this = class(c);
        match current_class {
            Some(previous) if previous == this => current.push(c),
            None if current == " " => {
                current.push(c);
                current_class = Some(this);
            }
            _ => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                current.push(c);
                current_class = Some(this);
            }
        }
    }
    if !current.trim().is_empty() {
        words.push(current);
    }
    words
}

/// librosa's mel filterbank (Slaney scale and norm), which Whisper's front end uses:
/// `n_mels` rows of `1 + N_FFT / 2` weights.
pub(super) fn mel_filters(n_mels: usize) -> Vec<f32> {
    let sample_rate = m::SAMPLE_RATE as f64;
    let bins = 1 + m::N_FFT / 2;
    let hz_to_mel = |hz: f64| {
        if hz < 1000.0 {
            hz / (200.0 / 3.0)
        } else {
            15.0 + (hz / 1000.0).ln() / (6.4f64.ln() / 27.0)
        }
    };
    let mel_to_hz = |mel: f64| {
        if mel < 15.0 {
            mel * (200.0 / 3.0)
        } else {
            1000.0 * ((mel - 15.0) * (6.4f64.ln() / 27.0)).exp()
        }
    };
    let top = hz_to_mel(sample_rate / 2.0);
    let points: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_to_hz(top * i as f64 / (n_mels + 1) as f64))
        .collect();
    let frequencies: Vec<f64> = (0..bins)
        .map(|k| k as f64 * sample_rate / m::N_FFT as f64)
        .collect();
    let mut filters = vec![0f32; n_mels * bins];
    for i in 0..n_mels {
        let (low, center, high) = (points[i], points[i + 1], points[i + 2]);
        let norm = 2.0 / (high - low);
        for (k, frequency) in frequencies.iter().enumerate() {
            let rising = (frequency - low) / (center - low);
            let falling = (high - frequency) / (high - center);
            filters[i * bins + k] = (rising.min(falling).max(0.0) * norm) as f32;
        }
    }
    filters
}

/// Cuts 16 kHz audio into pieces of at most 30 seconds, each ending at the quietest 100 ms of its
/// last 5 seconds, so a word is rarely split between two windows.
pub(super) fn split_at_quiet(samples: &[f32]) -> Vec<&[f32]> {
    const WINDOW: usize = m::N_SAMPLES;
    const SEARCH: usize = 5 * m::SAMPLE_RATE;
    const STEP: usize = m::SAMPLE_RATE / 10;
    let mut pieces = Vec::new();
    let mut start = 0;
    while samples.len() - start > WINDOW {
        let latest = start + WINDOW;
        let mut cut = latest;
        let mut quietest = f32::MAX;
        let mut at = latest - SEARCH;
        while at + STEP <= latest {
            let energy: f32 = samples[at..at + STEP].iter().map(|s| s * s).sum();
            if energy < quietest {
                quietest = energy;
                cut = at + STEP / 2;
            }
            at += STEP;
        }
        pieces.push(&samples[start..cut]);
        start = cut;
    }
    if start < samples.len() {
        pieces.push(&samples[start..]);
    }
    pieces
}

pub(super) struct Whisper {
    model: quantized_model::Whisper,
    config: Config,
    tokenizer: Tokenizer,
    filters: Vec<f32>,
    /// Tokens never written: timestamps, languages and other specials, and the config's list.
    suppressed: Vec<bool>,
    device: Device,
}

/// Whisper's own thresholds for a window that holds no speech.
const NO_SPEECH_PROB: f64 = 0.6;
const LOW_LOGPROB: f64 = -1.0;
/// Text tokens per 30-second window (Whisper's `sample_len`).
const SAMPLE_LEN: usize = 224;
/// Prompt tokens at most (Whisper keeps half its 448-token context for the prompt).
const MAX_PROMPT_TOKENS: usize = 200;

impl Whisper {
    pub(super) fn load(dir: &Path) -> Result<Whisper, String> {
        let device = Device::Cpu;
        let config: Config = serde_json::from_slice(
            &fs::read(dir.join("config.json")).map_err(|error| format!("config.json: {error}"))?,
        )
        .map_err(|error| format!("config.json: {error}"))?;
        let tokenizer = Tokenizer::from_json(
            &fs::read(dir.join("tokenizer.json"))
                .map_err(|error| format!("tokenizer.json: {error}"))?,
        )?;
        let vb = VarBuilder::from_gguf(dir.join(WEIGHTS_FILE), &device)
            .map_err(|error| format!("{WEIGHTS_FILE}: {error}"))?;
        let model = quantized_model::Whisper::load(&vb, config.clone())
            .map_err(|error| format!("Couldn't load Whisper: {error}"))?;
        let filters = mel_filters(config.num_mel_bins);
        let eot = tokenizer
            .token(m::EOT_TOKEN)
            .ok_or("The Whisper vocabulary has no end token.")?;
        let mut suppressed: Vec<bool> = (0..config.vocab_size as u32).map(|id| id > eot).collect();
        for id in &config.suppress_tokens {
            if let Some(entry) = suppressed.get_mut(*id as usize) {
                *entry = true;
            }
        }
        Ok(Whisper {
            model,
            config,
            tokenizer,
            filters,
            suppressed,
            device,
        })
    }

    fn special(&self, piece: &str) -> Result<u32, String> {
        self.tokenizer
            .token(piece)
            .ok_or_else(|| format!("The Whisper vocabulary has no {piece}."))
    }

    /// Transcribes 16 kHz mono audio. `language` is an ISO 639-1 code, or `None` to detect it
    /// from the first window; `prompt` carries the dictionary's words as Whisper's prior text.
    pub(super) fn transcribe(
        &mut self,
        samples: &[f32],
        language: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<String, String> {
        let failed = |error: candle_core::Error| format!("Whisper failed: {error}");
        let sot = self.special(m::SOT_TOKEN)?;
        let eot = self.special(m::EOT_TOKEN)?;
        let transcribe = self.special(m::TRANSCRIBE_TOKEN)?;
        let no_timestamps = self.special(m::NO_TIMESTAMPS_TOKEN)?;
        let no_speech = m::NO_SPEECH_TOKENS
            .iter()
            .find_map(|piece| self.tokenizer.token(piece));
        let start_of_prev = self.tokenizer.token("<|startofprev|>");
        let prompt_tokens: Vec<u32> = match (prompt, start_of_prev) {
            (Some(prompt), Some(start)) if !prompt.trim().is_empty() => {
                let mut tokens = self.tokenizer.encode(&format!(" {}", prompt.trim()));
                tokens.truncate(MAX_PROMPT_TOKENS);
                std::iter::once(start).chain(tokens).collect()
            }
            _ => Vec::new(),
        };
        let mut language_token =
            language.and_then(|code| self.tokenizer.token(&format!("<|{code}|>")));
        let mut text = String::new();
        for piece in split_at_quiet(samples) {
            let mel = m::audio::pcm_to_mel(&self.config, piece, &self.filters);
            let frames = mel.len() / self.config.num_mel_bins;
            let mel = Tensor::from_vec(mel, (1, self.config.num_mel_bins, frames), &self.device)
                .and_then(|mel| mel.narrow(2, 0, m::N_FRAMES.min(frames)))
                .map_err(failed)?;
            let features = self.model.encoder.forward(&mel, true).map_err(failed)?;
            if language_token.is_none() {
                language_token = Some(self.detect_language(&features, sot).map_err(failed)?);
            }
            let mut tokens = prompt_tokens.clone();
            tokens.push(sot);
            tokens.extend(language_token);
            tokens.push(transcribe);
            tokens.push(no_timestamps);
            let prefix = tokens.len();
            let mut sum_logprob = 0f64;
            let mut no_speech_prob = 0f64;
            for step in 0..SAMPLE_LEN {
                let input = Tensor::new(tokens.as_slice(), &self.device)
                    .and_then(|t| t.unsqueeze(0))
                    .map_err(failed)?;
                let hidden = self
                    .model
                    .decoder
                    .forward(&input, &features, step == 0)
                    .map_err(failed)?;
                let (_, length, _) = hidden.dims3().map_err(failed)?;
                if step == 0 {
                    if let Some(no_speech) = no_speech {
                        let at_sot = prefix - 3 - usize::from(language_token.is_some());
                        let logits = self
                            .model
                            .decoder
                            .final_linear(&hidden.i((..1, at_sot..at_sot + 1)).map_err(failed)?)
                            .and_then(|l| l.i(0)?.i(0))
                            .map_err(failed)?;
                        let probs = candle_nn::ops::softmax(&logits, D::Minus1).map_err(failed)?;
                        no_speech_prob = probs
                            .i(no_speech as usize)
                            .and_then(|p| p.to_scalar::<f32>())
                            .map_err(failed)? as f64;
                    }
                }
                let logits = self
                    .model
                    .decoder
                    .final_linear(&hidden.i((..1, length - 1..)).map_err(failed)?)
                    .and_then(|l| l.i(0)?.i(0))
                    .map_err(failed)?;
                let mut logits: Vec<f32> = logits.to_vec1().map_err(failed)?;
                // Only text and the end token: no timestamps, languages or other specials, and
                // never an empty first token.
                for (logit, suppressed) in logits.iter_mut().zip(&self.suppressed) {
                    if *suppressed {
                        *logit = f32::NEG_INFINITY;
                    }
                }
                if step == 0 {
                    if let Some(logit) = logits.get_mut(eot as usize) {
                        *logit = f32::NEG_INFINITY;
                    }
                }
                let (next, best) = logits
                    .iter()
                    .copied()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .unwrap_or((eot as usize, 0.0));
                let log_sum = logits
                    .iter()
                    .filter(|l| l.is_finite())
                    .map(|l| ((l - best) as f64).exp())
                    .sum::<f64>()
                    .ln();
                sum_logprob -= log_sum;
                let next = next as u32;
                if next == eot || tokens.len() > self.config.max_target_positions - 1 {
                    break;
                }
                tokens.push(next);
                if repeating(&tokens[prefix..]) {
                    break;
                }
            }
            let produced = &tokens[prefix..];
            let average = sum_logprob / (produced.len() + 1) as f64;
            if no_speech_prob > NO_SPEECH_PROB && average < LOW_LOGPROB {
                continue;
            }
            let piece_text = self.tokenizer.decode(produced);
            let piece_text = piece_text.trim();
            if !piece_text.is_empty() {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(piece_text);
            }
        }
        Ok(text)
    }

    /// The language token the first window's speech most likely is in.
    fn detect_language(&mut self, features: &Tensor, sot: u32) -> candle_core::Result<u32> {
        let input = Tensor::new(&[sot], &self.device)?.unsqueeze(0)?;
        let hidden = self.model.decoder.forward(&input, features, true)?;
        let logits: Vec<f32> = self
            .model
            .decoder
            .final_linear(&hidden.i((..1, ..1))?)?
            .i(0)?
            .i(0)?
            .to_vec1()?;
        let english = self.tokenizer.token("<|en|>").unwrap_or(sot + 1);
        Ok(super::catalog::WHISPER_LANGUAGES
            .iter()
            .filter_map(|code| self.tokenizer.token(&format!("<|{code}|>")))
            .filter(|id| (*id as usize) < logits.len())
            .max_by(|a, b| logits[*a as usize].total_cmp(&logits[*b as usize]))
            .unwrap_or(english))
    }
}

/// Whether the text ends in the same run of tokens three times over (Whisper's loop failure).
fn repeating(tokens: &[u32]) -> bool {
    (2..=16).any(|run| {
        tokens.len() >= run * 3 && {
            let end = tokens.len();
            let last = &tokens[end - run..];
            last == &tokens[end - 2 * run..end - run]
                && last == &tokens[end - 3 * run..end - 2 * run]
        }
    })
}
