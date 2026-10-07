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
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()
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

#[test]
fn system_powershell_path_is_under_explicit_system_root() {
    let root = Path::new("system-root");
    assert_eq!(
        system_powershell_path(root),
        root.join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe")
    );
}

#[test]
fn failed_install_does_not_fall_back_to_the_previous_executable() {
    let script = install_helper_script(
        42,
        Path::new("C:/Temp/kea-update.exe"),
        Path::new("C:/Users/test/Programs/Kea/kea.exe"),
        &"a".repeat(64),
        Path::new("C:/Users/test/AppData/Local/Kea/update-failure.log"),
    );
    assert!(!script.contains("$fallback"));
    assert!(script.contains("Get-FileHash -InputStream $stream -Algorithm SHA256"));
    assert!(script.contains("[System.IO.FileShare]::Read"));
    assert!(script.contains("$actual -ne $expected"));
    assert!(script.contains("Set-Content -LiteralPath $failure"));
    assert!(script.contains("Retry the update or run the latest installer manually."));
    assert!(script.contains("Start-Process -FilePath $installed"));
    assert!(script.contains("exit 1"));
}

#[test]
fn mutation_after_staging_fails_the_restart_boundary_verification() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("kea-update.exe");
    std::fs::write(&path, b"original installer").unwrap();
    let expected_sha256 = sha256_file(&path).unwrap();
    let staged = StagedUpdate {
        version: Version::parse("1.0.0").unwrap(),
        installer_path: path.clone(),
        expected_sha256,
    };

    std::fs::write(&path, b"mutated installer").unwrap();

    let error = verify_staged(&staged).unwrap_err().to_string();
    assert!(error.contains("changed after download verification"));

    let result = verify_staged_in_background(staged).recv().unwrap();
    match result {
        WorkerResult::Verified(Err(error)) => {
            assert!(error.contains("changed after download verification"));
        }
        other => panic!("unexpected verification result: {other:?}"),
    }
}
