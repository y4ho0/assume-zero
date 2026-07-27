use std::{env, fs, path::Path};

fn read(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {path:?}: {error}"))
}

#[test]
fn release_metadata_is_consistent() {
    let manifest: toml::Value =
        toml::from_str(&read("Cargo.toml")).expect("Cargo.toml must be valid TOML");
    let version = manifest["package"]["version"]
        .as_str()
        .expect("package.version must be a string");
    let expected_tag = format!("v{version}");

    let changelog = read("CHANGELOG.md");
    let latest_formal_version = changelog
        .lines()
        .filter_map(|line| line.strip_prefix("## ["))
        .filter(|line| !line.starts_with("Unreleased]"))
        .find_map(|line| line.split_once(']').map(|(version, _)| version))
        .expect("CHANGELOG must contain a formal release");
    assert_eq!(
        latest_formal_version, version,
        "latest CHANGELOG release must match package.version"
    );
    assert!(
        changelog.contains(&format!(
            "[{version}]: https://github.com/y4ho0/assume-zero/releases/tag/{expected_tag}"
        )),
        "CHANGELOG release link must match package.version"
    );

    let readme = read("README.md");
    let readme_tags: Vec<_> = readme
        .lines()
        .filter_map(|line| line.trim().strip_prefix("--tag "))
        .map(|tag| tag.trim_end_matches(" \\"))
        .collect();
    assert!(
        !readme_tags.is_empty(),
        "README must contain a versioned --tag install"
    );
    assert!(
        readme_tags.iter().all(|tag| *tag == expected_tag),
        "every README install tag must match package.version"
    );

    for revision in readme
        .lines()
        .filter_map(|line| line.trim().strip_prefix("--rev "))
        .map(|revision| revision.trim_end_matches(" \\"))
    {
        assert!(
            revision.len() == 40
                && revision
                    .chars()
                    .all(|character| character.is_ascii_hexdigit()),
            "README --rev values must be full 40-character Git commit SHAs"
        );
    }

    if let Ok(release_tag) = env::var("ASSUMEZERO_RELEASE_TAG") {
        assert_eq!(
            release_tag, expected_tag,
            "release workflow tag must match package.version"
        );
    }
}
