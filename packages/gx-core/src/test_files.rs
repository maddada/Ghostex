//! Which changed files are tests, by path alone: the Changes view on the computer and the phone
//! hide them on request, and gxserver lists them in its `changesDiff` answer for the phone.

/// Folders whose files are tests or test data, matched against every folder in the path without
/// regard to case (`Tests/` in a Swift package, `src/test/java` in a Java one).
const TEST_FOLDERS: &[&str] = &[
    "test",
    "tests",
    "__tests__",
    "spec",
    "testdata",
    "fixtures",
    "__fixtures__",
    "__mocks__",
    "__snapshots__",
];

/// True when `path` (relative to the project, `/` or `\` separated) is a test file.
///
/// CDXC:Changes 2026-11-07 WHY: the rule reads only the path, the same for every language, and
/// leans towards showing a file: a name has to carry a test marker (`_test.`, `.test.`, `.spec.`,
/// `test_*.py`, `FooTest.java`, `FooTests.swift`) or sit under a test folder. A word like
/// `latest.rs`, `contest.ts` or `testing.md` never hides, because a hidden real source file is
/// worse than a shown test.
pub fn is_test_file(path: &str) -> bool {
    let path = path.replace('\\', "/");
    let mut parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let Some(name) = parts.pop() else {
        return false;
    };
    if parts.iter().any(|folder| {
        let folder = folder.to_ascii_lowercase();
        TEST_FOLDERS.contains(&folder.as_str())
    }) {
        return true;
    }
    test_file_name(name)
}

fn test_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // `foo.test.ts`, `foo.spec.tsx`, `foo.test.d.ts`: a middle part that is `test` or `spec`.
    let segments: Vec<&str> = lower.split('.').collect();
    if segments.len() >= 3
        && segments[1..segments.len() - 1]
            .iter()
            .any(|part| *part == "test" || *part == "spec")
    {
        return true;
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, extension.to_ascii_lowercase()),
        _ => (name, String::new()),
    };
    let lower_stem = stem.to_ascii_lowercase();
    // `foo_test.go`, `foo_tests.rs`, `foo_spec.rb`.
    if ["_test", "_tests", "_spec"]
        .iter()
        .any(|suffix| lower_stem.len() > suffix.len() && lower_stem.ends_with(suffix))
    {
        return true;
    }
    // pytest: `test_foo.py` and its shared `conftest.py`.
    if extension == "py" && (lower_stem.starts_with("test_") || lower_stem == "conftest") {
        return true;
    }
    // `FooTest.java`, `FooTests.swift`, `FooTest.kt`: a capital `Test` after a word, so
    // `Contest.java` and `Latest.swift` stay.
    ["Test", "Tests"].iter().any(|suffix| {
        stem.strip_suffix(suffix).is_some_and(|before| {
            before
                .chars()
                .last()
                .is_some_and(|last| last.is_ascii_alphanumeric())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::is_test_file;

    #[test]
    fn test_names_in_every_language_are_tests() {
        for path in [
            "server/src/git_test.go",
            "packages/find/src/rank_tests.rs",
            "apps/web/src/button.test.tsx",
            "apps/web/src/button.spec.ts",
            "src/types.test.d.ts",
            "lib/user_spec.rb",
            "tool/test_parse.py",
            "tool/conftest.py",
            "app/src/main/java/com/x/UserServiceTest.java",
            "Sources/AppTests/ParserTests.swift",
            "app/src/LoginTest.kt",
        ] {
            assert!(is_test_file(path), "{path} should be a test file");
        }
    }

    #[test]
    fn files_under_test_folders_are_tests() {
        for path in [
            "server/tests/changes.rs",
            "test/helpers.js",
            "src/__tests__/view.tsx",
            "spec/models/user.rb",
            "packages/find/testdata/history.jsonl",
            "apps/web/fixtures/patch.diff",
            "src/test/java/com/x/Util.java",
            "Tests/Helpers.swift",
            "src/__snapshots__/view.tsx.snap",
            "windows\\tests\\path.rs",
        ] {
            assert!(is_test_file(path), "{path} should be a test file");
        }
    }

    #[test]
    fn real_source_stays() {
        for path in [
            "src/latest.rs",
            "src/contest.ts",
            "docs/testing.md",
            "src/Contest.java",
            "Sources/Latest.swift",
            "src/test_utils.rs",
            "src/attest.py",
            "src/specs.ts",
            "packages/test-utils/src/lib.rs",
            "src/inspector.rs",
            "src/Test.java",
            "src/_test.go",
            "README.md",
            "Cargo.toml",
            "tests",
            "",
        ] {
            assert!(!is_test_file(path), "{path} should not be a test file");
        }
    }
}
