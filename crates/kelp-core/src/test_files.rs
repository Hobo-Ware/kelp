const TEST_DIRS: [&str; 7] = [
    "test",
    "tests",
    "__tests__",
    "spec",
    "specs",
    "testdata",
    "__snapshots__",
];

/// Whether a repo-relative path (with `/` separators) looks like a test file.
pub fn is_test_file(path: &str) -> bool {
    let (dirs, name) = path.rsplit_once('/').unwrap_or(("", path));
    if dirs
        .split('/')
        .any(|dir| TEST_DIRS.contains(&dir.to_ascii_lowercase().as_str()))
    {
        return true;
    }
    if name == "conftest.py" {
        return true;
    }
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    let suffixed = |suffix: &str| stem.len() > suffix.len() && stem.ends_with(suffix);
    suffixed("_test")
        || suffixed("_spec")
        || suffixed(".test")
        || suffixed(".spec")
        || (ext == "py" && stem.len() > "test_".len() && stem.starts_with("test_"))
        || (matches!(ext, "java" | "kt" | "swift" | "cs") && suffixed("Test"))
        || (matches!(ext, "swift" | "cs") && suffixed("Tests"))
}

#[cfg(test)]
mod tests {
    use super::is_test_file;

    #[test]
    fn test_directories_match_at_any_depth() {
        for path in [
            "tests/a.rs",
            "crates/kelp-core/tests/round_trip.rs",
            "test/helper.rb",
            "src/__tests__/button.tsx",
            "spec/models/user.rb",
            "app/specs/user.rb",
            "fixtures/testdata/in.json",
            "src/__snapshots__/button.snap",
            "Tests/AppTests/Foo.swift",
        ] {
            assert!(is_test_file(path), "{path}");
        }
    }

    #[test]
    fn test_file_names_match() {
        for path in [
            "pkg/server_test.go",
            "src/user_test.rs",
            "web/button.test.ts",
            "web/button.spec.tsx",
            "lib/user_spec.rb",
            "app/test_models.py",
            "app/conftest.py",
            "src/UserTest.java",
            "src/UserTest.kt",
            "Sources/UserTest.swift",
            "Sources/UserTests.swift",
            "Models/UserTest.cs",
            "Models/UserTests.cs",
            "user_test.go",
        ] {
            assert!(is_test_file(path), "{path}");
        }
    }

    #[test]
    fn near_misses_are_not_tests() {
        for path in [
            "src/latest.rs",
            "src/contest.rs",
            "attestation/mod.rs",
            "src/testing_utils.rs",
            "src/protest.py",
            "src/Latest.java",
            "src/UserTests.java",
            "src/test_utils.rs",
            "docs/tester/readme.md",
            "src/inspect/mod.rs",
            "src/test",
            "test.rs",
            "Test.java",
            ".test.ts",
            "_test.go",
            "src/main.rs",
            "README.md",
        ] {
            assert!(!is_test_file(path), "{path}");
        }
    }
}
