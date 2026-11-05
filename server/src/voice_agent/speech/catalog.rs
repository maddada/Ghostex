//! The local models Dictate offers: what each one is, the files a download fetches (pinned to one
//! Hugging Face revision and checked against their SHA-256) and what they become on disk.
//!
//! CDXC:Dictate 2026-11-05 DECISION:
//! User (Q1258, Q1274): Parakeet TDT 0.6B v3 is preselected, Whisper large-v3-turbo is listed for the 99 languages Parakeet lacks, and Whisper base (0.15 GB) is there for slow or small computers. No longer list of models.

/// One file a download fetches.
pub(crate) struct RemoteFile {
    pub(crate) name: &'static str,
    pub(crate) bytes: u64,
    pub(crate) sha256: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    /// Run by telmo from its pack (`model.pack`).
    Parakeet,
    /// Run by candle from 8-bit weights (`model-q8_0.gguf`, `config.json`, `tokenizer.json`).
    Whisper,
}

pub(crate) struct ModelSpec {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) short_title: &'static str,
    pub(crate) family: Family,
    pub(crate) languages_label: &'static str,
    pub(crate) speed_label: &'static str,
    repo: &'static str,
    revision: &'static str,
    pub(crate) files: &'static [RemoteFile],
    /// The size once prepared, before it is on disk to measure.
    pub(crate) disk_bytes_estimate: u64,
    pub(crate) attribution: &'static str,
}

impl ModelSpec {
    pub(crate) fn url(&self, file: &RemoteFile) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, file.name
        )
    }

    pub(crate) fn download_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }

    pub(crate) fn languages(&self) -> &'static [&'static str] {
        match self.family {
            Family::Parakeet => PARAKEET_LANGUAGES,
            Family::Whisper => WHISPER_LANGUAGES,
        }
    }
}

pub(crate) const PARAKEET_ID: &str = "parakeet-tdt-0.6b-v3";

/// In the order clients list them.
pub(crate) const MODELS: [ModelSpec; 3] = [
    ModelSpec {
        id: PARAKEET_ID,
        title: "Parakeet TDT 0.6B v3",
        short_title: "Parakeet v3",
        family: Family::Parakeet,
        languages_label: "English + 24 European languages",
        speed_label: "Fastest",
        repo: "nvidia/parakeet-tdt-0.6b-v3",
        revision: "541d1f99c6b0c3cd0b11a95167540bb8edefd82b",
        files: &[
            RemoteFile {
                name: "parakeet-tdt-0.6b-v3.q8_0.gguf",
                bytes: 713_975_456,
                sha256: "e3880d0aaaaf2c308ea2c35016b2b895c423eb3fda924c1b463d1c19b7f4d32e",
            },
            RemoteFile {
                name: "tokenizer.json",
                bytes: 1_159_960,
                sha256: "bd321b096832a3f270bd3b2a88823957920f1a5c5ada71114a26ea729d0cbe91",
            },
        ],
        disk_bytes_estimate: 723_000_000,
        attribution: "Parakeet TDT 0.6B v3 by NVIDIA, CC BY 4.0",
    },
    ModelSpec {
        id: "whisper-large-v3-turbo",
        title: "Whisper large-v3-turbo",
        short_title: "Whisper turbo",
        family: Family::Whisper,
        languages_label: "99 languages",
        speed_label: "Slower",
        repo: "openai/whisper-large-v3-turbo",
        revision: "41f01f3fe87f28c78e2fbf8b568835947dd65ed9",
        files: &[
            RemoteFile {
                name: "model.safetensors",
                bytes: 1_617_824_864,
                sha256: "542566a422ae4f3fd23f1ba11add198fca01bbf82e66e6a2857b3f608b1eb9d1",
            },
            RemoteFile {
                name: "config.json",
                bytes: 1_256,
                sha256: "c5b526b3e3cd64cd8940dabb45e8ba726629e22d8ed389c29b552f9140daf04a",
            },
            RemoteFile {
                name: "tokenizer.json",
                bytes: 2_710_337,
                sha256: "297b13372ac43916285644fb9687add3cc62ee2a1adb60da3dc25cc94c1871fd",
            },
        ],
        disk_bytes_estimate: 880_000_000,
        attribution: "Whisper large-v3-turbo by OpenAI, MIT",
    },
    ModelSpec {
        id: "whisper-base",
        title: "Whisper base",
        short_title: "Whisper base",
        family: Family::Whisper,
        languages_label: "99 languages",
        speed_label: "Small and quick",
        repo: "openai/whisper-base",
        revision: "e37978b90ca9030d5170a5c07aadb050351a65bb",
        files: &[
            RemoteFile {
                name: "model.safetensors",
                bytes: 290_403_936,
                sha256: "07cadb9f25677c8d50df603e66a98fbd842cce45047139baeb16e6219a1e807b",
            },
            RemoteFile {
                name: "config.json",
                bytes: 1_983,
                sha256: "a153c53883a6799b6f056b4a8d1a515c9926d03994682ba88a7616618d7da0c1",
            },
            RemoteFile {
                name: "tokenizer.json",
                bytes: 2_480_466,
                sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566",
            },
        ],
        disk_bytes_estimate: 85_000_000,
        attribution: "Whisper base by OpenAI, Apache 2.0",
    },
];

pub(crate) fn model(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|model| model.id == id)
}

const PARAKEET_LANGUAGES: &[&str] = &[
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt",
    "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
];

/// Whisper's languages, in the order of its language tokens (`<|en|>` is the first).
pub(crate) const WHISPER_LANGUAGES: &[&str] = &[
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it",
    "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn",
    "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
    "ha", "ba", "jw", "su", "yue",
];

/// The languages a client's Language picker offers: Parakeet's, with their English names. Whisper
/// takes any code in [`WHISPER_LANGUAGES`] too, which `setDictateSettings` accepts.
pub(crate) const LANGUAGE_LABELS: &[(&str, &str)] = &[
    ("auto", "Auto-detect"),
    ("en", "English"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("ru", "Russian"),
    ("uk", "Ukrainian"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("fi", "Finnish"),
    ("cs", "Czech"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("hr", "Croatian"),
    ("bg", "Bulgarian"),
    ("ro", "Romanian"),
    ("hu", "Hungarian"),
    ("el", "Greek"),
    ("et", "Estonian"),
    ("lv", "Latvian"),
    ("lt", "Lithuanian"),
    ("mt", "Maltese"),
    ("zh", "Chinese (Whisper)"),
    ("ja", "Japanese (Whisper)"),
    ("ko", "Korean (Whisper)"),
    ("ar", "Arabic (Whisper)"),
    ("hi", "Hindi (Whisper)"),
    ("tr", "Turkish (Whisper)"),
    ("he", "Hebrew (Whisper)"),
    ("vi", "Vietnamese (Whisper)"),
    ("id", "Indonesian (Whisper)"),
    ("th", "Thai (Whisper)"),
];
