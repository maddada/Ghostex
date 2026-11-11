use crate::rows::{row, section, Page, Section};

/// Settings › Secrets: the password managers Ghostex reads saved keys and tokens from (1Password
/// through its `op` CLI), and how agents ask for a secret privately. Every row is a UI row with no
/// stored value: gxserver keeps the providers (server/src/secret_providers/).
///
/// CDXC:Secrets 2026-11-02 SEE-ALSO:
/// apps/desktop/src/app/window/settings_modal/tabs/secrets.rs draws these rows; `ghostex secret providers` changes the same list.
pub(crate) fn page() -> Page {
    Page {
        id: "secrets",
        title: "Secrets",
        sections: vec![providers(), requests()],
    }
}

pub(crate) fn providers() -> Section {
    section(
        "providers",
        "Secret providers",
        vec![
            row("secretProviders", "1Password accounts", "The 1Password accounts Ghostex reads keys and tokens from, so you can paste an op://vault/item/field reference instead of the key itself. Needs the 1Password CLI (op) with \"Integrate with 1Password CLI\" turned on in 1Password. Ghostex keeps only the account name; 1Password may ask you to unlock when a key is read."),
            // CDXC:Settings 2026-11-11 DECISION: User (Q2609): the Check a 1Password reference row moves into the 1Password group, under its accounts, instead of a one-row "Check a reference" group of its own.
            row("secretReferenceCheck", "Check a 1Password reference", "Reads an op://vault/item/field reference once to see that it works. Ghostex says only whether it could read it and never shows the value."),
        ],
    )
}

pub(crate) fn requests() -> Section {
    section(
        "requests",
        "Secrets agents ask for",
        vec![
            row("secretRequests", "Private secret requests", "When an agent needs a password, API key or token, it asks in a private card above the chat box instead of in the chat. What you type goes to a private file only that agent's commands can read, deleted after 24 hours; it never appears in the chat or reaches the model. With a 1Password account added, you can answer with an op:// reference instead."),
        ],
    )
}
