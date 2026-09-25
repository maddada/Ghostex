//! Which drawing a picker row uses.
//!
//! Port of the two key functions in
//! `packages/shared/session-chat-presentation/model-picker-artwork.ts`. Only the keys move: the
//! artwork itself stays a build-time constant of each renderer (`model-picker-artwork.json`, read
//! by `apps/desktop/src/app/native_chat/`), so the document carries the key and never the SVG.

/// `modelPickerArtworkKey`.
pub fn model_picker_artwork_key(model: &str, standard: bool) -> &'static str {
    if standard {
        return "model-standard";
    }
    if model.contains("astra") {
        return "model-gpt-6-astra";
    }
    if model.contains("sol") {
        return "model-gpt-5.6-sol";
    }
    if model.contains("terra") {
        return "model-gpt-5.6-terra";
    }
    if model.contains("luna") {
        return "model-gpt-5.6-luna";
    }
    match model {
        "fable" => "model-fable",
        "opus[1m]" => "model-opus[1m]",
        "opus" => "model-opus",
        "sonnet" => "model-sonnet",
        _ => "model-haiku",
    }
}

/// `modelPickerEffortArtworkKey`.
pub fn model_picker_effort_artwork_key(effort: &str) -> &'static str {
    match effort {
        "none" => "effort-none",
        "minimal" => "effort-minimal",
        "low" => "effort-low",
        "medium" => "effort-medium",
        "high" => "effort-high",
        "xhigh" => "effort-xhigh",
        "max" => "effort-max",
        _ => "effort-ultra",
    }
}
