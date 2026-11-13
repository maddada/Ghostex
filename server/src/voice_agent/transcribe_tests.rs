//! What each provider is asked, and the sentence every failure
//! leaves on the recording.

use super::provider::Provider;
use super::recordings::Problem;
use super::transcribe::{multipart_body, request_fields, status_problem, Outcome};

fn refused(provider: Provider, status: u16, body: &str) -> (bool, Problem) {
    match status_problem(provider, status, body) {
        Outcome::Final(problem) => (false, problem),
        Outcome::Transient(problem) => (true, problem),
        Outcome::Text(_) => panic!("a refusal read as text"),
    }
}

#[test]
fn openai_gets_its_transcription_model_and_the_name_hint() {
    let (url, fields) = request_fields(Provider::OpenAi, Some("en"), &["Ghostex".to_string()]);
    assert_eq!(url, "https://api.openai.com/v1/audio/transcriptions");
    let field = |name: &str| {
        fields
            .iter()
            .find(|(field, _)| *field == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(field("model"), Some("gpt-4o-transcribe"));
    assert_eq!(field("response_format"), Some("json"));
    assert_eq!(field("language"), Some("en"));
    assert_eq!(field("prompt"), Some("Names you may hear: Ghostex."));
    assert_eq!(field("keyterm"), None);
}

#[test]
fn xai_gets_its_keyterms_and_no_model() {
    let (url, fields) = request_fields(Provider::Xai, None, &["Ghostex".to_string()]);
    assert_eq!(url, "https://api.x.ai/v1/stt");
    assert_eq!(fields, vec![("keyterm", "Ghostex".to_string())]);
}

#[test]
fn the_file_is_the_last_part() {
    let body = multipart_body(
        "b",
        &[("model", "gpt-4o-transcribe".to_string())],
        "x.m4a",
        b"AUDIO",
    );
    let text = String::from_utf8(body).unwrap();
    assert_eq!(
        text,
        "--b\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\ngpt-4o-transcribe\r\n\
         --b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.m4a\"\r\nContent-Type: audio/mp4\r\n\r\nAUDIO\r\n--b--\r\n"
    );
}

#[test]
fn a_refused_key_says_which_provider_and_where_to_check_it() {
    for provider in [Provider::Xai, Provider::OpenAi] {
        for status in [401, 403] {
            let (retried, problem) = refused(provider, status, "");
            assert!(!retried);
            assert_eq!(problem.code, "badKey");
            assert_eq!(problem.provider.as_deref(), Some(provider.id()));
            assert_eq!(
                problem.message,
                format!(
                    "{} refused the API key. Check it in Settings > Dictation, then transcribe again.",
                    provider.label()
                )
            );
        }
    }
}

#[test]
fn a_rate_limit_is_retried_and_keeps_the_providers_words() {
    let (retried, problem) = refused(
        Provider::Xai,
        429,
        r#"{"error":{"message":"Too many requests, slow down."}}"#,
    );
    assert!(retried);
    assert_eq!(problem.code, "rateLimited");
    assert_eq!(
        problem.message,
        "xAI is limiting requests right now. Wait a moment, then transcribe again."
    );
    assert_eq!(
        problem.detail.as_deref(),
        Some("Too many requests, slow down.")
    );
}

#[test]
fn xai_payment_required_is_retried_and_never_claims_no_credit() {
    let (retried, problem) = refused(Provider::Xai, 402, "");
    assert!(retried);
    assert_eq!(problem.code, "paymentRequired");
    assert!(!problem.message.contains("out of credit"));
    assert!(problem.message.contains("console.x.ai"));
    assert_eq!(problem.detail.as_deref(), Some("xAI answered HTTP 402."));
}

#[test]
fn openai_insufficient_quota_reads_as_payment_required_and_stops() {
    let (retried, problem) = refused(
        Provider::OpenAi,
        429,
        r#"{"error":{"message":"You exceeded your current quota.","type":"insufficient_quota","code":"insufficient_quota"}}"#,
    );
    assert!(!retried);
    assert_eq!(problem.code, "paymentRequired");
    assert!(problem.message.contains("platform.openai.com"));
    assert_eq!(
        problem.detail.as_deref(),
        Some("You exceeded your current quota.")
    );
    // An ordinary OpenAI rate limit still waits and retries.
    let (retried, problem) = refused(Provider::OpenAi, 429, "");
    assert!(retried);
    assert_eq!(problem.code, "rateLimited");
}

#[test]
fn audio_the_provider_cannot_take_is_final() {
    for status in [400, 415, 422] {
        let (retried, problem) = refused(Provider::OpenAi, status, "");
        assert!(!retried);
        assert_eq!(problem.code, "badAudio");
        assert_eq!(
            problem.message,
            "OpenAI couldn't read the recording. Transcribe again; if it keeps failing, record it again."
        );
    }
    let (_, problem) = refused(Provider::Xai, 413, "");
    assert_eq!(problem.code, "badAudio");
    assert!(problem.message.contains("too long for xAI"));
}

#[test]
fn a_provider_outage_is_retried() {
    let (retried, problem) = refused(Provider::Xai, 503, "<html>Service Unavailable</html>");
    assert!(retried);
    assert_eq!(problem.code, "providerUnavailable");
    assert_eq!(
        problem.message,
        "xAI isn't answering right now. Wait a moment, then transcribe again."
    );
    // An HTML error page is not shown as the provider's words.
    assert_eq!(problem.detail.as_deref(), Some("xAI answered HTTP 503."));
}

#[test]
fn a_problem_without_provider_or_detail_keeps_the_old_shape() {
    let problem = Problem {
        code: "audioMissing".to_string(),
        message: "gone".to_string(),
        provider: None,
        detail: None,
    };
    assert_eq!(
        serde_json::to_value(&problem).unwrap(),
        serde_json::json!({"code": "audioMissing", "message": "gone"})
    );
    let old: Problem =
        serde_json::from_value(serde_json::json!({"code": "noKey", "message": "x"})).unwrap();
    assert_eq!(old.provider, None);
}
