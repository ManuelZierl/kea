use super::*;

fn release(tag: &str, prerelease: bool, with_digest: bool) -> GithubRelease {
    GithubRelease {
        tag_name: tag.into(),
        draft: false,
        prerelease,
        assets: vec![GithubAsset {
            name: format!("kea-{tag}-windows-x86_64-setup.exe"),
            browser_download_url: format!(
                "https://github.com/ManuelZierl/kea/releases/download/{tag}/kea-{tag}-windows-x86_64-setup.exe"
            ),
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
        select_update(&current, &releases, Platform::Windows)
            .unwrap()
            .version,
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
        select_update(&current, &releases, Platform::Windows)
            .unwrap()
            .version,
        Version::parse("1.0.1").unwrap()
    );
}

#[test]
fn candidates_without_release_digest_are_rejected() {
    let current = Version::parse("0.0.1-alpha.3").unwrap();
    assert!(select_update(
        &current,
        &[release("v0.0.1-alpha.4", true, false)],
        Platform::Windows
    )
    .is_none());
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
        staged_path: path.clone(),
        expected_sha256,
        replaces: None,
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

#[test]
fn installers_outside_this_repository_are_never_selected() {
    let current = Version::parse("0.0.1").unwrap();
    let mut elsewhere = release("v0.0.2", false, true);
    elsewhere.assets[0].browser_download_url = "https://example.invalid/kea.exe".into();
    assert!(select_update(&current, &[elsewhere], Platform::Windows).is_none());
}

#[test]
fn stale_staged_installers_are_removed_but_nothing_else() {
    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("kea-update-abc.exe");
    std::fs::write(&old, b"old installer").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - STALE_STAGED_AGE * 2)
        .unwrap();
    // Another running Kea may still need the installer it just staged.
    std::fs::write(root.path().join("kea-update-fresh.exe"), b"fresh").unwrap();
    std::fs::write(root.path().join("notes.txt"), b"keep").unwrap();
    remove_staged_in(root.path(), STALE_STAGED_AGE);
    assert!(!old.exists());
    assert!(root.path().join("kea-update-fresh.exe").exists());
    assert!(root.path().join("notes.txt").exists());
}

#[test]
fn download_urls_are_compared_after_parsing() {
    let tag = "v0.0.2";
    let asset = "kea-v0.0.2-windows-x86_64-setup.exe";
    let good = format!("https://github.com/ManuelZierl/kea/releases/download/{tag}/{asset}");
    assert!(is_release_download(&good, tag, asset));
    for bad in [
        format!("https://github.com/ManuelZierl/kea/releases/download/../../../other/repo/releases/download/{tag}/{asset}"),
        format!("https://github.com/ManuelZierl/kea/releases/download/{tag}/%2e%2e/{asset}"),
        format!("https://github.com.evil.example/ManuelZierl/kea/releases/download/{tag}/{asset}"),
        format!("https://user@github.com/ManuelZierl/kea/releases/download/{tag}/{asset}"),
        format!("https://github.com:8443/ManuelZierl/kea/releases/download/{tag}/{asset}"),
        format!("http://github.com/ManuelZierl/kea/releases/download/{tag}/{asset}"),
        format!("{good}?redirect=elsewhere"),
        format!("https://github.com/ManuelZierl/kea/releases/download/v0.0.3/{asset}"),
        format!("https://github.com/ManuelZierl/kea/releases/download/{tag}/other.exe"),
    ] {
        assert!(!is_release_download(&bad, tag, asset), "{bad}");
    }
}

fn linux_release(tag: &str) -> GithubRelease {
    let mut release = release(tag, true, true);
    let name = format!("kea-{tag}-linux-x86_64.tar.gz");
    release.assets.push(GithubAsset {
        browser_download_url: format!(
            "https://github.com/ManuelZierl/kea/releases/download/{tag}/{name}"
        ),
        name,
        digest: Some(format!("sha256:{}", "b".repeat(64))),
        size: 2048,
    });
    release
}

#[test]
fn each_platform_selects_only_its_own_release_asset() {
    let current = Version::parse("0.0.1-alpha.4").unwrap();
    let releases = [linux_release("v0.0.1-alpha.5")];
    let linux = select_update(&current, &releases, Platform::Linux).unwrap();
    assert!(linux
        .asset_url
        .ends_with("/kea-v0.0.1-alpha.5-linux-x86_64.tar.gz"));
    assert_eq!(linux.sha256, "b".repeat(64));
    let windows = select_update(&current, &releases, Platform::Windows).unwrap();
    assert!(windows.asset_url.ends_with("-windows-x86_64-setup.exe"));

    // A release without a Linux archive offers nothing on Linux.
    assert!(select_update(
        &current,
        &[release("v0.0.1-alpha.5", true, true)],
        Platform::Linux
    )
    .is_none());
}

#[cfg(unix)]
mod linux {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    const FAKE_KEA: &[u8] = b"#!/bin/sh\n[ \"$1\" = --help ] && exit 0\nexit 1\n";

    enum Entry<'a> {
        File(&'a str, &'a [u8]),
        Symlink(&'a str, &'a str),
    }

    fn archive(root: &Path, entries: &[Entry<'_>]) -> PathBuf {
        let path = root.join("release.tar.gz");
        let encoder = flate2::write::GzEncoder::new(
            File::create(&path).unwrap(),
            flate2::Compression::fast(),
        );
        let mut builder = tar::Builder::new(encoder);
        for entry in entries {
            let mut header = tar::Header::new_gnu();
            match entry {
                Entry::File(name, contents) => {
                    header.set_entry_type(tar::EntryType::Regular);
                    header.set_size(contents.len() as u64);
                    header.set_mode(0o644);
                    builder.append_data(&mut header, name, *contents).unwrap();
                }
                Entry::Symlink(name, target) => {
                    header.set_entry_type(tar::EntryType::Symlink);
                    header.set_size(0);
                    builder.append_link(&mut header, name, target).unwrap();
                }
            }
        }
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    #[test]
    fn extracts_only_the_root_kea_file_as_an_executable() {
        let root = tempfile::tempdir().unwrap();
        let install = tempfile::tempdir().unwrap();
        let archive = archive(
            root.path(),
            &[
                Entry::File("./README.md", b"readme"),
                Entry::File("./licenses/kea", b"not the binary"),
                Entry::File("./kea", FAKE_KEA),
            ],
        );
        let staged = extract_release_binary(&archive, install.path()).unwrap();
        assert_eq!(std::fs::read(&staged).unwrap(), FAKE_KEA);
        assert_eq!(staged.parent(), Some(install.path()));
        assert!(staged
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(STAGED_BINARY_PREFIX));
        let mode = staged.metadata().unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    /// Regression: the extracted file used to stay open for writing, so running
    /// it failed with "Text file busy".
    #[test]
    fn the_extracted_binary_can_be_probed_immediately() {
        let root = tempfile::tempdir().unwrap();
        let install = tempfile::tempdir().unwrap();
        let archive = archive(root.path(), &[Entry::File("./kea", FAKE_KEA)]);
        let staged = extract_release_binary(&archive, install.path()).unwrap();
        probe_binary_within(&staged, Duration::from_secs(10)).unwrap();
    }

    #[test]
    fn archives_without_a_single_regular_kea_file_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let install = tempfile::tempdir().unwrap();
        for (entries, expected) in [
            (
                vec![Entry::File("./README.md", b"readme")],
                "does not contain",
            ),
            (
                vec![Entry::Symlink("./kea", "/bin/sh")],
                "not a regular file",
            ),
            (
                vec![Entry::File("./kea", FAKE_KEA), Entry::File("kea", FAKE_KEA)],
                "more than one",
            ),
            (vec![Entry::File("./kea", b"")], "implausible size"),
        ] {
            let archive = archive(root.path(), &entries);
            let error = format!(
                "{:#}",
                extract_release_binary(&archive, install.path()).unwrap_err()
            );
            assert!(error.contains(expected), "{error}");
        }
        // Rejected archives leave nothing staged beside the executable.
        assert_eq!(std::fs::read_dir(install.path()).unwrap().count(), 0);
    }

    fn script(root: &Path, name: &str, body: &str) -> PathBuf {
        let path = root.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn probe_requires_a_successful_help_run_and_reports_why_not() {
        let root = tempfile::tempdir().unwrap();
        let good = script(root.path(), "good", "[ \"$1\" = --help ] && exit 0; exit 1");
        probe_binary_within(&good, Duration::from_secs(10)).unwrap();

        let incompatible = script(
            root.path(),
            "incompatible",
            "echo \"version 'GLIBC_9.99' not found\" >&2; exit 1",
        );
        let error = format!(
            "{:#}",
            probe_binary_within(&incompatible, Duration::from_secs(10)).unwrap_err()
        );
        assert!(error.contains("does not run on this system"), "{error}");
        assert!(error.contains("GLIBC_9.99"), "{error}");

        let hung = script(root.path(), "hung", "exec sleep 30");
        let started = std::time::Instant::now();
        let error = format!(
            "{:#}",
            probe_binary_within(&hung, Duration::from_millis(300)).unwrap_err()
        );
        assert!(error.contains("did not answer --help in time"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn only_user_owned_release_installs_may_be_replaced() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("bin").join("kea");
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::write(&installed, FAKE_KEA).unwrap();
        check_linux_install_target(&installed).unwrap();

        for profile in ["release", "debug"] {
            let built = root
                .path()
                .join("checkout/target")
                .join(profile)
                .join("kea");
            std::fs::create_dir_all(built.parent().unwrap()).unwrap();
            std::fs::write(&built, FAKE_KEA).unwrap();
            let error = format!("{:#}", check_linux_install_target(&built).unwrap_err());
            assert!(error.contains("build directory"), "{error}");
        }

        // Root bypasses directory permissions, so only check as a normal user.
        if !nix::unistd::getuid().is_root() {
            let directory = installed.parent().unwrap();
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o555)).unwrap();
            let result = check_linux_install_target(&installed);
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o755)).unwrap();
            let error = format!("{:#}", result.unwrap_err());
            assert!(error.contains("is not writable"), "{error}");
        }
    }

    #[test]
    fn replacement_is_an_atomic_rename_beside_the_target() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("kea");
        std::fs::write(&target, b"old version").unwrap();
        let staged = root.path().join(".kea-update-abc");
        std::fs::write(&staged, b"new version").unwrap();
        replace_executable(&staged, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new version");
        assert!(!staged.exists());

        let elsewhere = tempfile::tempdir().unwrap();
        let foreign = elsewhere.path().join(".kea-update-def");
        std::fs::write(&foreign, b"other").unwrap();
        assert!(replace_executable(&foreign, &target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"new version");
    }

    #[test]
    fn a_changed_staged_binary_is_never_installed() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("kea");
        std::fs::write(&target, b"old version").unwrap();
        let staged_path = root.path().join(".kea-update-abc");
        std::fs::write(&staged_path, b"verified").unwrap();
        let staged = StagedUpdate {
            version: Version::parse("1.0.0").unwrap(),
            expected_sha256: sha256_binary(&staged_path).unwrap(),
            staged_path: staged_path.clone(),
            replaces: Some(target.clone()),
        };
        assert!(staged
            .restart_explanation()
            .contains(&target.display().to_string()));
        verify_staged(&staged).unwrap();

        std::fs::write(&staged_path, b"tampered").unwrap();
        let error = verify_staged(&staged).unwrap_err().to_string();
        assert!(error.contains("changed after download verification"));
        assert_eq!(std::fs::read(&target).unwrap(), b"old version");
    }

    #[test]
    fn stale_staged_binaries_are_removed_but_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(".kea-update-old");
        std::fs::write(&old, b"old").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - STALE_STAGED_AGE * 2)
            .unwrap();
        std::fs::write(root.path().join(".kea-update-fresh"), b"fresh").unwrap();
        std::fs::write(root.path().join("kea"), b"installed").unwrap();
        remove_staged_matching(root.path(), STAGED_BINARY_PREFIX, "", STALE_STAGED_AGE);
        assert!(!old.exists());
        assert!(root.path().join(".kea-update-fresh").exists());
        assert!(root.path().join("kea").exists());
    }
}
