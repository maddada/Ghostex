//! Hiding test files in the Changes view: the filter that moves them out of the list and back in
//! their old places. The rule that names a test file is gx-core's `test_files`, which gxserver
//! uses for the phone too.

use super::model::{ChangedFile, ChangesData};

pub(crate) use ghostex_gx_core::test_files::is_test_file;

impl ChangesData {
    /// Moves every test file out of `files` (remembering where each was) or puts them all back in
    /// their old places.
    pub(crate) fn set_test_files_hidden(&mut self, hide: bool) {
        for (index, file) in std::mem::take(&mut self.hidden_tests) {
            let index = index.min(self.files.len());
            self.files.insert(index, file);
        }
        if !hide {
            return;
        }
        let files = std::mem::take(&mut self.files);
        for (index, file) in files.into_iter().enumerate() {
            if is_test_file(&file.path) {
                self.hidden_tests.push((index, file));
            } else {
                self.files.push(file);
            }
        }
    }

    pub(crate) fn hidden_test_count(&self) -> usize {
        self.hidden_tests.len()
    }

    /// Every file in the diff, shown or hidden.
    pub(crate) fn all_files(&self) -> impl Iterator<Item = &ChangedFile> {
        self.files
            .iter()
            .chain(self.hidden_tests.iter().map(|(_, file)| file))
    }
}

/// "3 test files hidden".
pub(crate) fn hidden_tests_label(count: usize) -> String {
    format!(
        "{count} test {} hidden",
        if count == 1 { "file" } else { "files" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::native_changes::model::FileStatus;

    fn file(path: &str) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            status: FileStatus::Modified,
            binary: false,
            additions: 2,
            deletions: 1,
            hunks: Vec::new(),
            uncommitted: true,
        }
    }

    fn paths(data: &ChangesData) -> Vec<&str> {
        data.files.iter().map(|file| file.path.as_str()).collect()
    }

    #[test]
    fn hiding_takes_tests_out_of_the_list_and_counts() {
        let mut data = ChangesData {
            files: vec![
                file("src/a.rs"),
                file("src/a_test.rs"),
                file("src/b.rs"),
                file("tests/c.rs"),
            ],
            ..ChangesData::default()
        };
        data.set_test_files_hidden(true);
        assert_eq!(paths(&data), ["src/a.rs", "src/b.rs"]);
        assert_eq!(data.hidden_test_count(), 2);
        assert_eq!(data.additions(), 4);
        assert_eq!(data.all_files().count(), 4);
        assert_eq!(hidden_tests_label(2), "2 test files hidden");
        assert_eq!(hidden_tests_label(1), "1 test file hidden");
    }

    #[test]
    fn showing_puts_tests_back_in_their_places() {
        let original = [
            "tests/a.rs",
            "src/a.rs",
            "src/a_test.rs",
            "src/b.rs",
            "x.spec.ts",
        ];
        let mut data = ChangesData {
            files: original.iter().map(|path| file(path)).collect(),
            ..ChangesData::default()
        };
        data.set_test_files_hidden(true);
        assert_eq!(paths(&data), ["src/a.rs", "src/b.rs"]);
        // Hiding twice changes nothing.
        data.set_test_files_hidden(true);
        assert_eq!(paths(&data), ["src/a.rs", "src/b.rs"]);
        data.set_test_files_hidden(false);
        assert_eq!(paths(&data), original);
        assert_eq!(data.hidden_test_count(), 0);
    }

    #[test]
    fn a_diff_of_only_tests_hides_everything() {
        let mut data = ChangesData {
            files: vec![file("a_test.go"), file("b_test.go")],
            ..ChangesData::default()
        };
        data.set_test_files_hidden(true);
        assert!(data.files.is_empty());
        assert_eq!(data.hidden_test_count(), 2);
    }
}
