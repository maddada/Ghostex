use ghostex_settings_catalog::{availability, built_in_extensions, Platform};

const ROWS: [&str; 3] = ["readAloudVoice", "readAloudRate", "readAloudUnavailable"];

/// The keys Settings > Chat leaves out, as the desktop's Settings store gathers them: rows of a
/// built-in extension that is off, then rows `platform` does not have.
fn hidden(platform: Platform, read_aloud_off: bool) -> Vec<&'static str> {
    let read = move |key: &str| match key {
        "readAloudHidden" => Some(read_aloud_off),
        _ => None,
    };
    let mut keys = built_in_extensions::hidden_setting_keys_with(read);
    keys.extend(availability::hidden_row_keys(platform, |_| None));
    keys
}

fn shown(platform: Platform, read_aloud_off: bool) -> Vec<&'static str> {
    let hidden = hidden(platform, read_aloud_off);
    ROWS.into_iter()
        .filter(|key| !hidden.contains(key))
        .collect()
}

#[test]
fn read_aloud_off_shows_no_rows() {
    assert!(
        shown(Platform::current(), true).is_empty(),
        "Read aloud off"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn read_aloud_on_a_mac_shows_voice_and_speed_without_the_cannot_speak_note() {
    assert_eq!(
        shown(Platform::MacOs, false),
        ["readAloudVoice", "readAloudRate"]
    );
}

#[test]
fn read_aloud_on_linux_and_windows_shows_speed_and_the_note() {
    for platform in [Platform::Linux, Platform::Windows] {
        let hidden = availability::hidden_row_keys(platform, |_| None);
        assert!(hidden.contains(&"readAloudVoice"));
        assert!(!hidden.contains(&"readAloudRate"));
        assert!(!hidden.contains(&"readAloudUnavailable"));
    }
}
