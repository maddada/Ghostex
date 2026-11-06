//! Every computer's short name: at most two characters, drawn on the sidebar's machine pills.
//!
//! CDXC:RemoteMachines 2026-11-06 DECISION:
//! User: "i really hate these pills showing local and windows. i want each machine to get 2 chars max, i wanna be able to set the short names in settings for each machine. also we should show this name on unified sidebar project headers when group by proj is enabled". A computer's pill reads its short name: the one set in Settings > Remote (`remoteMachines[].shortName`, and `localMachineShortName` for this computer), else one made from its name: the first letters of its first two words ("Mac Studio" is MS), or of a one-word name its first letter and the next consonant ("Windows" is WN), uppercased. A made name never repeats a set one or an earlier made one: it tries the first letter with each later letter of the name, then with 2 to 9. The full name stays in the pill's tooltip.

/// The longest short name, in characters.
pub const MACHINE_SHORT_NAME_MAX: usize = 2;

/// One computer as [`machine_short_names`] reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachineNameInput {
    pub id: String,
    /// The name the user gave it (or this computer's own name).
    pub name: String,
    /// The short name set in Settings, when one is.
    pub short_name: Option<String>,
}

/// A short name as typed: trimmed, its first two characters, empty when none.
pub fn clean_machine_short_name(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .take(MACHINE_SHORT_NAME_MAX)
        .collect()
}

fn words(name: &str) -> Vec<Vec<char>> {
    name.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().collect())
        .collect()
}

fn is_vowel(ch: char) -> bool {
    matches!(ch.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

fn upper(chars: impl IntoIterator<Item = char>) -> String {
    chars.into_iter().flat_map(char::to_uppercase).collect()
}

/// The short names to try for `name`, best first.
fn candidates(name: &str) -> Vec<String> {
    let words = words(name);
    let Some(first_word) = words.first() else {
        return vec!["PC".to_string()];
    };
    let first = first_word[0];
    let mut tries = Vec::new();
    if let Some(second) = words.get(1) {
        tries.push(upper([first, second[0]]));
    }
    let rest: Vec<char> = first_word[1..]
        .iter()
        .chain(words.iter().skip(1).flatten())
        .copied()
        .collect();
    if let Some(consonant) = rest.iter().find(|ch| ch.is_alphabetic() && !is_vowel(**ch)) {
        tries.push(upper([first, *consonant]));
    }
    tries.extend(rest.iter().map(|ch| upper([first, *ch])));
    tries.extend(('2'..='9').map(|digit| upper([first, digit])));
    if first_word.len() == 1 && words.len() == 1 {
        tries.insert(0, upper([first]));
    }
    tries
}

/// Every computer's short name by machine id (`local` for this one), from this computer's name
/// and the saved settings: `localMachineShortName` and `remoteMachines` (normalized, so every
/// saved machine takes part, shown in the sidebar or not, and turning one on renames no other).
pub fn short_names_by_machine_id(
    local_name: &str,
    settings: &serde_json::Map<String, serde_json::Value>,
) -> std::collections::HashMap<String, String> {
    let text = |value: Option<&serde_json::Value>| {
        value
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let mut machines = vec![MachineNameInput {
        id: crate::sidebar_view::LOCAL_MACHINE_ID.to_string(),
        name: local_name.to_string(),
        short_name: text(settings.get("localMachineShortName")),
    }];
    for machine in
        crate::sidebar_actions::normalize_remote_machine_settings(settings.get("remoteMachines"))
    {
        machines.push(MachineNameInput {
            id: text(machine.get("id")).unwrap_or_default(),
            name: text(machine.get("name")).unwrap_or_default(),
            short_name: text(machine.get("shortName")),
        });
    }
    let short_names = machine_short_names(&machines);
    machines
        .into_iter()
        .zip(short_names)
        .map(|(machine, short_name)| (machine.id, short_name))
        .collect()
}

/// Every computer's short name, in `machines`' order: its own when set, else a made one no other
/// computer has.
pub fn machine_short_names(machines: &[MachineNameInput]) -> Vec<String> {
    let set: Vec<Option<String>> = machines
        .iter()
        .map(|machine| {
            machine
                .short_name
                .as_deref()
                .map(clean_machine_short_name)
                .filter(|short| !short.is_empty())
        })
        .collect();
    let mut taken: Vec<String> = set
        .iter()
        .flatten()
        .map(|short| short.to_uppercase())
        .collect();
    machines
        .iter()
        .zip(set)
        .map(|(machine, set)| {
            if let Some(set) = set {
                return set;
            }
            let tries = candidates(&machine.name);
            let made = tries
                .iter()
                .find(|short| !taken.contains(&short.to_uppercase()))
                .or(tries.first())
                .cloned()
                .unwrap_or_default();
            taken.push(made.to_uppercase());
            made
        })
        .collect()
}
