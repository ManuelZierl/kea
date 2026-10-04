use super::*;

fn release(tag: &str, prerelease: bool, with_digest: bool) -> GithubRelease {
    GithubRelease {
        tag_name: tag.into(),
        draft: false,
        prerelease,
        assets: vec![GithubAsset {
            name: format!("kea-{tag}-windows-x86_64-setup.exe"),
            browser_download_url: format!("https://example.invalid/{tag}.exe"),
            digest: with_digest.then(|| {
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .into()
            }),
            size: 1024,
        }],
    }
}

#[test]
fn prerelease_builds_follow_newer_prereleases_and_stable_releases() {
    let current = Version::parse("0.0.1-alpha.3").unwrap();
    let releases = [
        release("v0.0.1-alpha.2", true, true),
        release("v0.0.1-alpha.4", true, true),
        release("v0.0.1", false, true),
    ];
    assert_eq!(
        select_update(&current, &releases).unwrap().version,
        Version::parse("0.0.1").unwrap()
    );
}

#[test]
fn stable_builds_ignore_prerelease_channels() {
    let current = Version::parse("1.0.0").unwrap();
    let releases = [
        release("v1.1.0-alpha.1", true, true),
        release("v1.0.1", false, true),
    ];
    assert_eq!(
        select_update(&current, &releases).unwrap().version,
        Version::parse("1.0.1").unwrap()
    );
}

#[test]
fn candidates_without_release_digest_are_rejected() {
    let current = Version::parse("0.0.1-alpha.3").unwrap();
    assert!(select_update(&current, &[release("v0.0.1-alpha.4", true, false)]).is_none());
}

#[test]
fn digest_parser_is_strict() {
    assert!(parse_sha256("sha256:abc").is_none());
    assert!(parse_sha256(&format!("sha1:{}", "a".repeat(64))).is_none());
    assert_eq!(
        parse_sha256(&format!("sha256:{}", "A".repeat(64))).unwrap(),
        "a".repeat(64)
    );
}
