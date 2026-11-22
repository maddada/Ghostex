//! The hold-to-talk turn rules (`talk.rs`): presses, releases, cancels, Mute and dropped links.

use std::time::{Duration, Instant};

use super::talk::{Cancel, Press, Release, Talk, MIN_HOLD_AUDIO_MS, SILENCE_AFTER_HOLD};

fn speak(talk: &mut Talk, ms: u64) -> bool {
    talk.passes_audio(ms)
}

#[test]
fn push_to_talk_commits_a_held_turn() {
    let mut talk = Talk::new(true);
    assert!(
        !speak(&mut talk, 100),
        "nothing is heard before Talk is held"
    );
    assert_eq!(talk.press(), Press::Talk);
    assert!(speak(&mut talk, MIN_HOLD_AUDIO_MS));
    assert_eq!(talk.release(Instant::now()), Release::Commit);
    assert!(!speak(&mut talk, 100), "nothing is heard after the release");
}

#[test]
fn a_tap_is_too_short_and_sends_nothing() {
    let mut talk = Talk::new(true);
    assert_eq!(talk.press(), Press::Talk);
    speak(&mut talk, MIN_HOLD_AUDIO_MS - 20);
    assert_eq!(
        talk.release(Instant::now()),
        Release::TooShort { clear: true }
    );
    // A Regular call's tap has no buffer of its own to drop.
    let mut regular = Talk::new(false);
    regular.set_muted(true);
    assert_eq!(regular.press(), Press::Unmute);
    assert_eq!(
        regular.release(Instant::now()),
        Release::TooShort { clear: false }
    );
}

#[test]
fn a_second_press_or_a_stray_release_changes_nothing() {
    let mut talk = Talk::new(true);
    assert_eq!(talk.release(Instant::now()), Release::Ignored);
    assert_eq!(talk.press(), Press::Talk);
    assert_eq!(talk.press(), Press::Ignored, "key repeat");
    assert!(talk.held());
}

#[test]
fn talk_cancel_drops_the_turn() {
    let mut talk = Talk::new(true);
    assert_eq!(talk.cancel(), Cancel::Ignored);
    talk.press();
    speak(&mut talk, 1_500);
    assert_eq!(talk.cancel(), Cancel::Cleared);
    assert!(!talk.held());
    assert_eq!(
        talk.release(Instant::now()),
        Release::Ignored,
        "the release after a cancel sends nothing"
    );
    assert!(!speak(&mut talk, 100));
}

#[test]
fn a_muted_push_to_talk_call_refuses_the_press() {
    let mut talk = Talk::new(true);
    talk.set_muted(true);
    assert_eq!(talk.press(), Press::RefusedMuted);
    assert!(!talk.held());
    assert!(!speak(&mut talk, 500), "no audio gets through while muted");
    talk.set_muted(false);
    assert_eq!(talk.press(), Press::Talk);
}

#[test]
fn muting_mid_hold_cancels_a_push_to_talk_turn() {
    let mut talk = Talk::new(true);
    talk.press();
    speak(&mut talk, 800);
    assert_eq!(talk.set_muted(true), Cancel::Cleared);
    assert!(!talk.held());
    assert!(!speak(&mut talk, 100));
}

#[test]
fn a_dropped_link_cancels_a_push_to_talk_hold() {
    let mut talk = Talk::new(true);
    assert_eq!(talk.detached(Instant::now()), None);
    talk.press();
    speak(&mut talk, 2_000);
    assert_eq!(
        talk.detached(Instant::now()),
        Some(Release::TooShort { clear: true })
    );
    assert!(
        !talk.held(),
        "gxserver no longer hears you after the link drops"
    );
}

#[test]
fn a_new_client_starts_unmuted_and_not_holding() {
    let mut talk = Talk::new(true);
    talk.set_muted(true);
    talk.attached();
    assert_eq!(talk.press(), Press::Talk);
    talk.attached();
    assert!(!talk.held());
}

#[test]
fn regular_calls_hold_to_unmute_only_while_muted() {
    let mut talk = Talk::new(false);
    assert!(speak(&mut talk, 100), "a Regular call listens hands-free");
    assert_eq!(
        talk.press(),
        Press::Ignored,
        "unmuted: holding adds nothing"
    );
    talk.set_muted(true);
    assert!(!speak(&mut talk, 100));
    assert_eq!(talk.press(), Press::Unmute);
    assert!(speak(&mut talk, 600), "the hold unmutes");
    let now = Instant::now();
    assert_eq!(talk.release(now), Release::Unmuted);
    assert!(!speak(&mut talk, 100), "muted again after the hold");
}

#[test]
fn silence_follows_a_muted_regular_hold_until_the_turn_ends() {
    let mut talk = Talk::new(false);
    talk.set_muted(true);
    talk.press();
    speak(&mut talk, 900);
    let now = Instant::now();
    talk.release(now);
    assert!(talk.wants_silence(now + Duration::from_millis(100)));
    talk.speech_stopped();
    assert!(!talk.wants_silence(now + Duration::from_millis(200)));

    talk.press();
    speak(&mut talk, 900);
    talk.release(now);
    assert!(
        !talk.wants_silence(now + SILENCE_AFTER_HOLD),
        "silence stops by itself"
    );

    talk.press();
    speak(&mut talk, 900);
    talk.release(now);
    talk.set_muted(false);
    assert!(
        !talk.wants_silence(now + Duration::from_millis(100)),
        "the microphone speaks again"
    );
}

#[test]
fn a_dropped_link_ends_a_regular_hold_like_a_release() {
    let mut talk = Talk::new(false);
    talk.set_muted(true);
    talk.press();
    speak(&mut talk, 900);
    let now = Instant::now();
    assert_eq!(talk.detached(now), Some(Release::Unmuted));
    assert!(talk.wants_silence(now + Duration::from_millis(100)));
}
