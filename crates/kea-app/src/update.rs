//! Release update discovery, the Windows current-user installer handoff and
//! in-place replacement of a user-owned Linux executable.
//!
//! Kea never trusts a download URL by itself. A candidate asset must belong to a
//! GitHub Release and carry GitHub's SHA-256 asset digest; the staged file is
//! hashed again before the application replaces or installs anything. On Linux
//! the release archive's `kea` binary is extracted next to the running
//! executable and must run `--help` successfully before it is offered; the
//! running executable is only replaced, by an atomic rename, after the user
//! confirms the restart.
use anyhow::{Context as _, Result};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::{
    fs::File,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

const RELEASES_URL: &str = "https://api.github.com/repos/ManuelZierl/kea/releases?per_page=20";
const MAX_INSTALLER_BYTES: u64 = 128 * 1024 * 1024;
/// Upper bound for the executable extracted from a Linux release archive.
const MAX_BINARY_BYTES: u64 = 256 * 1024 * 1024;
/// A release binary that cannot print its usage this quickly cannot run here.
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// Installers are only fetched from this repository's release downloads.
const DOWNLOAD_HOST: &str = "github.com";
const DOWNLOAD_PATH: [&str; 4] = ["ManuelZierl", "kea", "releases", "download"];
/// Another running Kea may still need a recent staged installer.
const STALE_STAGED_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// Staged installers live here, so stale ones can be found and removed.
const STAGING_DIRECTORY: &str = "kea-updates";
/// Prefix of extracted Linux binaries staged beside the running executable.
const STAGED_BINARY_PREFIX: &str = ".kea-update-";

/// Platforms with an in-app update path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Platform {
    /// Silent current-user installer, run by a helper after Kea exits.
    Windows,
    /// Release archive whose `kea` binary replaces the running executable.
    Linux,
}

impl Platform {
    fn current() -> Option<Self> {
        if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Some(Self::Windows)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Self::Linux)
        } else {
            None
        }
    }

    fn asset_name(self, tag: &str) -> String {
        match self {
            Self::Windows => format!("kea-{tag}-windows-x86_64-setup.exe"),
            Self::Linux => format!("kea-{tag}-linux-x86_64.tar.gz"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateInfo {
    pub version: Version,
    asset_url: String,
    sha256: String,
    size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagedUpdate {
    pub version: Version,
    /// The verified Windows installer, or the extracted Linux binary.
    staged_path: PathBuf,
    expected_sha256: String,
    /// Linux: the executable that the staged binary replaces on restart.
    replaces: Option<PathBuf>,
}

impl StagedUpdate {
    /// What confirming the restart will do, for the confirmation dialog.
    pub fn restart_explanation(&self) -> String {
        match &self.replaces {
            Some(target) => format!(
                "Kea will close all terminal processes and discard unsaved transient state, replace {} with the verified release binary, and start the new version. Save anything you want to keep first.",
                target.display()
            ),
            None => "Kea will close all terminal processes and discard unsaved transient state before running the verified current-user installer. Save anything you want to keep first.".into(),
        }
    }
}

/// How the restart went after the staged update was applied.
#[derive(Debug)]
pub enum Applied {
    /// The new version is starting; this instance should close.
    Restarting,
    /// The executable was replaced but the new version could not be started.
    InstalledNotRestarted(String),
}

#[derive(Debug)]
pub enum WorkerResult {
    Checked(std::result::Result<Option<UpdateInfo>, String>),
    Staged(std::result::Result<StagedUpdate, String>),
    Verified(std::result::Result<(), String>),
}

#[derive(Clone, Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
    size: u64,
}

pub fn supported() -> bool {
    Platform::current().is_some()
}

/// One-line description of the update mechanism, for Settings.
pub fn settings_description() -> &'static str {
    match Platform::current() {
        Some(Platform::Linux) => {
            "Linux updates replace this user-owned kea executable after verification and never require elevation."
        }
        _ => "Windows updates use a current-user installer and never require elevation.",
    }
}

pub fn startup_failure_notice() -> Option<String> {
    if Platform::current() != Some(Platform::Windows) {
        return None;
    }
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    let path = PathBuf::from(local_app_data)
        .join("Kea")
        .join("update-failure.log");
    let file = File::open(&path).ok()?;
    let mut message = String::new();
    file.take(16 * 1024).read_to_string(&mut message).ok()?;
    let message = message.trim();
    (!message.is_empty()).then(|| {
        format!(
            "The previous Kea update failed: {message}. Kea remains on the current version; retry the update or run the latest installer manually. Details are retained in {}.",
            path.display()
        )
    })
}

pub fn check_in_background() -> Receiver<WorkerResult> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = check_for_update().map_err(|error| format!("{error:#}"));
        let _ = tx.send(WorkerResult::Checked(result));
    });
    rx
}

pub fn stage_in_background(update: UpdateInfo) -> Receiver<WorkerResult> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = prepare_update(&update).map_err(|error| format!("{error:#}"));
        let _ = tx.send(WorkerResult::Staged(result));
    });
    rx
}

pub fn verify_staged_in_background(staged: StagedUpdate) -> Receiver<WorkerResult> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = verify_staged(&staged).map_err(|error| format!("{error:#}"));
        let _ = tx.send(WorkerResult::Verified(result));
    });
    rx
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(format!("Kea/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .context("cannot create update HTTP client")
}

/// The installer download may take minutes on a slow link; only connecting is
/// held to the metadata deadline.
fn download_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(format!("Kea/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(30 * 60))
        .build()
        .context("cannot create update download client")
}

fn staging_directory() -> std::path::PathBuf {
    std::env::temp_dir().join(STAGING_DIRECTORY)
}

/// Remove files left by earlier update attempts. Only files older than a day
/// go, so another running Kea keeps the one it staged; one that is still
/// running cannot be deleted on Windows and is skipped. Never touches other
/// files.
pub fn remove_stale_staged_updates() {
    match Platform::current() {
        Some(Platform::Windows) => remove_staged_in(&staging_directory(), STALE_STAGED_AGE),
        Some(Platform::Linux) => {
            remove_staged_matching(
                &staging_directory(),
                "kea-update-",
                ".tar.gz",
                STALE_STAGED_AGE,
            );
            if let Some(directory) = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf))
            {
                remove_staged_matching(&directory, STAGED_BINARY_PREFIX, "", STALE_STAGED_AGE);
            }
        }
        None => {}
    }
}

fn remove_staged_in(directory: &Path, older_than: Duration) {
    remove_staged_matching(directory, "kea-update-", ".exe", older_than);
}

fn remove_staged_matching(directory: &Path, prefix: &str, suffix: &str, older_than: Duration) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with(prefix) && name.ends_with(suffix)) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                now.duration_since(modified)
                    .is_ok_and(|age| age >= older_than)
            });
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Only this repository's release downloads for exactly this tag and asset.
/// Compares the parsed URL, so `..` segments, ports, credentials, queries and
/// look-alike prefixes cannot point elsewhere. Redirects (to GitHub's asset
/// storage) are still followed; integrity rests on the release's SHA-256.
fn is_release_download(url: &str, tag: &str, asset: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(segments) = url.path_segments() else {
        return false;
    };
    let expected = DOWNLOAD_PATH.iter().copied().chain([tag, asset]);
    url.scheme() == "https"
        && url.host_str() == Some(DOWNLOAD_HOST)
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && segments.eq(expected)
}

fn check_for_update() -> Result<Option<UpdateInfo>> {
    let platform = Platform::current().context("self-update is not supported on this platform")?;
    let releases = http_client()?
        .get(RELEASES_URL)
        .send()
        .context("cannot reach GitHub Releases")?
        .error_for_status()
        .context("GitHub Releases returned an error")?
        .json::<Vec<GithubRelease>>()
        .context("cannot read GitHub release metadata")?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .context("Kea's compiled version is not valid semver")?;
    Ok(select_update(&current, &releases, platform))
}

fn select_update(
    current: &Version,
    releases: &[GithubRelease],
    platform: Platform,
) -> Option<UpdateInfo> {
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter(|release| !current.pre.is_empty() || !release.prerelease)
        .filter_map(|release| {
            let version_text = release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name);
            let version = Version::parse(version_text).ok()?;
            if version <= *current {
                return None;
            }

            let expected_name = platform.asset_name(&release.tag_name);
            let asset = release
                .assets
                .iter()
                .find(|asset| asset.name == expected_name)?;
            if asset.size == 0
                || asset.size > MAX_INSTALLER_BYTES
                || !is_release_download(
                    &asset.browser_download_url,
                    &release.tag_name,
                    &expected_name,
                )
            {
                return None;
            }
            let sha256 = parse_sha256(asset.digest.as_deref()?)?;
            Some(UpdateInfo {
                version,
                asset_url: asset.browser_download_url.clone(),
                sha256,
                size: asset.size,
            })
        })
        .max_by(|left, right| left.version.cmp(&right.version))
}

fn parse_sha256(value: &str) -> Option<String> {
    let digest = value.strip_prefix("sha256:")?;
    (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| digest.to_ascii_lowercase())
}

fn prepare_update(update: &UpdateInfo) -> Result<StagedUpdate> {
    match Platform::current() {
        Some(Platform::Windows) => prepare_installer(update),
        Some(Platform::Linux) => {
            let target = linux_install_target()?;
            prepare_linux_binary(update, &target)
        }
        None => anyhow::bail!("self-update is not supported on this platform"),
    }
}

/// Download the release asset into the staging folder and check it against the
/// release's SHA-256 before anything uses it.
fn download_verified(update: &UpdateInfo, suffix: &str) -> Result<tempfile::NamedTempFile> {
    anyhow::ensure!(
        update.size <= MAX_INSTALLER_BYTES,
        "update installer exceeds the download limit"
    );

    let mut response = download_client()?
        .get(&update.asset_url)
        .send()
        .context("cannot download the Kea update")?
        .error_for_status()
        .context("the Kea update download returned an error")?;
    if let Some(length) = response.content_length() {
        anyhow::ensure!(
            length <= MAX_INSTALLER_BYTES,
            "update installer exceeds the download limit"
        );
    }

    let directory = staging_directory();
    // Drop abandoned earlier attempts, never another instance's fresh one.
    remove_staged_in(&directory, STALE_STAGED_AGE);
    std::fs::create_dir_all(&directory).context("cannot create the update staging folder")?;
    let mut staged = tempfile::Builder::new()
        .prefix("kea-update-")
        .suffix(suffix)
        .tempfile_in(&directory)
        .context("cannot stage the Kea update")?;
    let copied = std::io::copy(
        &mut response.by_ref().take(MAX_INSTALLER_BYTES + 1),
        staged.as_file_mut(),
    )
    .context("cannot write the staged Kea update")?;
    anyhow::ensure!(
        copied <= MAX_INSTALLER_BYTES,
        "update installer exceeds the download limit"
    );
    staged
        .as_file_mut()
        .flush()
        .context("cannot flush the staged Kea update")?;
    staged
        .as_file()
        .sync_all()
        .context("cannot sync the staged Kea update")?;

    let actual = sha256_file(staged.path())?;
    anyhow::ensure!(
        actual == update.sha256,
        "update digest does not match the GitHub release asset"
    );
    Ok(staged)
}

fn prepare_installer(update: &UpdateInfo) -> Result<StagedUpdate> {
    let staged = download_verified(update, ".exe")?;
    let (file, staged_path) = staged
        .keep()
        .map_err(|error| error.error)
        .context("cannot retain the staged Kea update")?;
    drop(file);
    Ok(StagedUpdate {
        version: update.version.clone(),
        staged_path,
        expected_sha256: update.sha256.clone(),
        replaces: None,
    })
}

/// The running executable, if Kea may replace it in place: a release install
/// owned by this user in a writable directory, never a Cargo build output.
fn linux_install_target() -> Result<PathBuf> {
    let executable = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .context("cannot locate the running Kea executable")?;
    check_linux_install_target(&executable)?;
    Ok(executable)
}

#[cfg(unix)]
fn check_linux_install_target(executable: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt as _;

    let in_build_output = executable
        .ancestors()
        .skip(1)
        .zip(executable.ancestors().skip(2))
        .any(|(profile, parent)| {
            parent.file_name().is_some_and(|name| name == "target")
                && profile
                    .file_name()
                    .is_some_and(|name| name == "debug" || name == "release")
        });
    anyhow::ensure!(
        !in_build_output,
        "Kea is running from a build directory ({}); update the checkout and rebuild, or install the release binary (for example to ~/.local/bin)",
        executable.display()
    );
    let metadata = executable
        .metadata()
        .context("cannot inspect the running Kea executable")?;
    anyhow::ensure!(
        metadata.uid() == nix::unistd::getuid().as_raw(),
        "{} belongs to another user; update it with the tool that installed it",
        executable.display()
    );
    let directory = executable
        .parent()
        .context("the Kea executable has no parent directory")?;
    anyhow::ensure!(
        nix::unistd::access(directory, nix::unistd::AccessFlags::W_OK).is_ok(),
        "{} is not writable; update Kea with the tool that installed it",
        directory.display()
    );
    Ok(())
}

#[cfg(not(unix))]
fn check_linux_install_target(_: &Path) -> Result<()> {
    anyhow::bail!("self-update is not supported on this platform")
}

fn prepare_linux_binary(update: &UpdateInfo, target: &Path) -> Result<StagedUpdate> {
    let archive = download_verified(update, ".tar.gz")?;
    let directory = target
        .parent()
        .context("the Kea executable has no parent directory")?;
    // Staged beside the target, so the restart is a same-filesystem rename.
    let staged = extract_release_binary(archive.path(), directory)?;
    drop(archive);
    probe_binary(&staged)?;
    let expected_sha256 = sha256_binary(&staged)?;
    let staged_path = staged
        .keep()
        .context("cannot retain the staged Kea update")?;
    Ok(StagedUpdate {
        version: update.version.clone(),
        staged_path,
        expected_sha256,
        replaces: Some(target.to_path_buf()),
    })
}

/// Extract exactly one regular `kea` file from the archive root. Links, devices,
/// nested paths and duplicates are never written. The returned path has no open
/// handle, because Linux refuses to execute a file that is open for writing.
fn extract_release_binary(archive: &Path, directory: &Path) -> Result<tempfile::TempPath> {
    let file = File::open(archive).context("cannot reopen the downloaded release archive")?;
    let mut entries = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let mut staged = None;
    for entry in entries
        .entries()
        .context("cannot read the release archive")?
    {
        let mut entry = entry.context("cannot read the release archive")?;
        let path = entry
            .path()
            .context("the release archive has an unreadable path")?
            .into_owned();
        let name = path.strip_prefix(".").unwrap_or(&path);
        if name != Path::new("kea") {
            continue;
        }
        anyhow::ensure!(
            entry.header().entry_type().is_file(),
            "the release archive's kea entry is not a regular file"
        );
        anyhow::ensure!(
            staged.is_none(),
            "the release archive contains more than one kea executable"
        );
        let size = entry
            .header()
            .size()
            .context("the release archive's kea entry has no size")?;
        anyhow::ensure!(
            size > 0 && size <= MAX_BINARY_BYTES,
            "the release archive's kea executable has an implausible size"
        );
        let mut output = tempfile::Builder::new()
            .prefix(STAGED_BINARY_PREFIX)
            .tempfile_in(directory)
            .with_context(|| format!("cannot stage the update in {}", directory.display()))?;
        let copied = std::io::copy(
            &mut entry.by_ref().take(MAX_BINARY_BYTES + 1),
            output.as_file_mut(),
        )
        .context("cannot extract the Kea executable")?;
        anyhow::ensure!(
            copied == size,
            "the release archive's kea executable is truncated"
        );
        output
            .as_file()
            .sync_all()
            .context("cannot sync the extracted Kea executable")?;
        staged = Some(output);
    }
    let staged = staged
        .context("the release archive does not contain a kea executable")?
        .into_temp_path();
    set_executable(&staged)?;
    Ok(staged)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .context("cannot mark the extracted Kea executable as executable")
}

#[cfg(not(unix))]
fn set_executable(_: &Path) -> Result<()> {
    Ok(())
}

/// Run the extracted binary's `--help`, which exits before any window or shell
/// starts. A build that cannot load here (for example, one needing a newer
/// glibc) fails now instead of after the running executable was replaced.
fn probe_binary(path: &Path) -> Result<()> {
    probe_binary_within(path, PROBE_TIMEOUT)
}

fn probe_binary_within(path: &Path, timeout: Duration) -> Result<()> {
    use std::process::{Command, Stdio};

    // A terminal spawned on another thread may briefly hold an inherited write
    // descriptor between its fork and exec; retry instead of failing the update.
    let mut attempts = 0;
    let mut child = loop {
        let spawned = Command::new(path)
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        match spawned {
            Err(error) if is_text_file_busy(&error) && attempts < 10 => {
                attempts += 1;
                thread::sleep(Duration::from_millis(50));
            }
            spawned => break spawned.context("cannot run the downloaded Kea executable")?,
        }
    };
    let stderr = child.stderr.take().map(|stderr| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.take(4096).read_to_string(&mut text);
            text
        })
    });
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .context("cannot wait for the downloaded Kea executable")?
        {
            break Some(status);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let stderr = stderr
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    let detail = stderr.trim();
    match status {
        Some(status) if status.success() => Ok(()),
        Some(status) => anyhow::bail!(
            "the downloaded Kea executable does not run on this system ({status}){}{}",
            if detail.is_empty() { "" } else { ": " },
            detail
        ),
        None => anyhow::bail!("the downloaded Kea executable did not answer --help in time"),
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    sha256_bounded(path, MAX_INSTALLER_BYTES)
}

fn sha256_binary(path: &Path) -> Result<String> {
    sha256_bounded(path, MAX_BINARY_BYTES)
}

fn sha256_bounded(path: &Path, limit: u64) -> Result<String> {
    let metadata = path
        .metadata()
        .context("cannot inspect the staged Kea update")?;
    anyhow::ensure!(
        metadata.len() <= limit,
        "update installer exceeds the download limit"
    );
    let mut file = File::open(path).context("cannot reopen the staged Kea update")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .context("cannot hash the staged Kea update")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn verify_staged(staged: &StagedUpdate) -> Result<()> {
    anyhow::ensure!(
        staged.staged_path.is_file(),
        "the staged update installer is no longer available"
    );
    let actual = if staged.replaces.is_some() {
        sha256_binary(&staged.staged_path)?
    } else {
        sha256_file(&staged.staged_path)?
    };
    anyhow::ensure!(
        actual == staged.expected_sha256,
        "the staged update changed after download verification"
    );
    Ok(())
}

/// Apply a staged update that `verify_staged` just accepted.
pub fn launch_staged_update(staged: &StagedUpdate) -> Result<Applied> {
    anyhow::ensure!(supported(), "self-update is not supported on this platform");
    anyhow::ensure!(
        staged.staged_path.is_file(),
        "the staged update installer is no longer available"
    );
    match &staged.replaces {
        Some(target) => {
            replace_executable(&staged.staged_path, target)?;
            Ok(match restart_installed(target) {
                Ok(()) => Applied::Restarting,
                Err(error) => Applied::InstalledNotRestarted(format!("{error:#}")),
            })
        }
        None => {
            spawn_install_helper(&staged.staged_path, &staged.expected_sha256)?;
            Ok(Applied::Restarting)
        }
    }
}

#[cfg(unix)]
fn is_text_file_busy(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(nix::errno::Errno::ETXTBSY as i32)
}

#[cfg(not(unix))]
fn is_text_file_busy(_: &std::io::Error) -> bool {
    false
}

/// Atomically swap the staged binary into place. The running process keeps its
/// already-open executable; a failed rename leaves the old version untouched.
fn replace_executable(staged: &Path, target: &Path) -> Result<()> {
    anyhow::ensure!(
        staged.parent() == target.parent(),
        "the staged update is not beside the Kea executable"
    );
    std::fs::rename(staged, target).with_context(|| format!("cannot replace {}", target.display()))
}

/// Start the newly installed executable in its own process group so it
/// outlives this instance.
#[cfg(unix)]
fn restart_installed(target: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt as _;
    use std::process::{Command, Stdio};

    Command::new(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .with_context(|| format!("cannot start {}", target.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn restart_installed(_: &Path) -> Result<()> {
    anyhow::bail!("self-update is not supported on this platform")
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn spawn_install_helper(staged: &Path, expected_sha256: &str) -> Result<()> {
    use std::{os::windows::process::CommandExt as _, process::Command};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .context("LOCALAPPDATA is unavailable")?;
    let installed = local_app_data.join("Programs").join("Kea").join("kea.exe");
    let failure_log = local_app_data.join("Kea").join("update-failure.log");
    let powershell = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .context("SystemRoot is unavailable")?;
    anyhow::ensure!(
        powershell.is_absolute(),
        "SystemRoot is not an absolute path"
    );
    let powershell = system_powershell_path(&powershell);
    anyhow::ensure!(
        powershell.is_file(),
        "the system PowerShell executable is unavailable"
    );
    let script = install_helper_script(
        std::process::id(),
        staged,
        &installed,
        expected_sha256,
        &failure_log,
    );

    Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("cannot start the Windows update helper")?;
    Ok(())
}

#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
fn system_powershell_path(system_root: &Path) -> PathBuf {
    system_root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
}

#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
fn install_helper_script(
    parent: u32,
    staged: &Path,
    installed: &Path,
    expected_sha256: &str,
    failure_log: &Path,
) -> String {
    format!(
        "$ErrorActionPreference='Stop';\
         $parent={};$setup={};$installed={};$expected={};$failure={};$stream=$null;$process=$null;$ok=$false;$errorMessage='update helper failed';\
         Wait-Process -Id $parent -ErrorAction SilentlyContinue;\
         Start-Sleep -Milliseconds 150;\
         try {{$stream=[System.IO.File]::Open($setup,[System.IO.FileMode]::Open,[System.IO.FileAccess]::Read,[System.IO.FileShare]::Read);\
              if ($stream.Length -gt 134217728) {{throw 'update installer exceeds the download limit'}};\
              $actual=(Get-FileHash -InputStream $stream -Algorithm SHA256).Hash.ToLowerInvariant();\
              if ($actual -ne $expected) {{throw 'update installer digest mismatch'}};\
              $stream.Position=0;\
              $process=Start-Process -FilePath $setup -ArgumentList '/S' -Wait -PassThru;$ok=($process.ExitCode -eq 0);if (!$ok) {{$errorMessage='installer returned a failure exit code'}}}} catch {{$errorMessage=$_.Exception.Message;$ok=$false}} finally {{if ($null -ne $stream) {{$stream.Dispose()}};Remove-Item -LiteralPath $setup -Force -ErrorAction SilentlyContinue}};\
         if ($ok -and (Test-Path -LiteralPath $installed)) {{Remove-Item -LiteralPath $failure -Force -ErrorAction SilentlyContinue;Start-Process -FilePath $installed; exit 0}};\
         try {{$directory=Split-Path -Parent $failure;New-Item -ItemType Directory -Force -Path $directory | Out-Null;Set-Content -LiteralPath $failure -Value ('Kea update failed: ' + $errorMessage + '. Retry the update or run the latest installer manually.') -Encoding UTF8}} catch {{}};\
         exit 1",
        parent,
        powershell_literal(staged),
        powershell_literal(installed),
        powershell_string(expected_sha256),
        powershell_literal(failure_log),
    )
}

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
fn spawn_install_helper(_: &Path, _: &str) -> Result<()> {
    anyhow::bail!("self-update is not supported on this platform")
}

#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
fn powershell_literal(path: &Path) -> String {
    powershell_string(&path.to_string_lossy())
}

#[cfg(any(test, all(target_os = "windows", target_arch = "x86_64")))]
fn powershell_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
#[path = "../tests/unit/update.rs"]
mod tests;
