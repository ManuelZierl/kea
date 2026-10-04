//! Release update discovery and Windows current-user installer handoff.
//!
//! Kea never trusts a download URL by itself. A candidate installer must be an
//! asset of a GitHub Release and carry GitHub's SHA-256 asset digest; the staged
//! installer is hashed again before the application agrees to restart.
use anyhow::{Context as _, Result};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    path::Path,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

const RELEASES_URL: &str =
    "https://api.github.com/repos/ManuelZierl/kea/releases?per_page=20";
const MAX_INSTALLER_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateInfo {
    pub version: Version,
    installer_url: String,
    sha256: String,
    size: u64,
}

#[derive(Debug)]
pub enum WorkerResult {
    Checked(std::result::Result<Option<UpdateInfo>, String>),
    InstallReady(std::result::Result<(), String>),
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
    cfg!(all(target_os = "windows", target_arch = "x86_64"))
}

pub fn check_in_background() -> Receiver<WorkerResult> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = check_for_update().map_err(|error| format!("{error:#}"));
        let _ = tx.send(WorkerResult::Checked(result));
    });
    rx
}

pub fn install_in_background(update: UpdateInfo) -> Receiver<WorkerResult> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = prepare_update(&update).map_err(|error| format!("{error:#}"));
        let _ = tx.send(WorkerResult::InstallReady(result));
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

fn check_for_update() -> Result<Option<UpdateInfo>> {
    anyhow::ensure!(supported(), "self-update is not supported on this platform");
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
    Ok(select_update(&current, &releases))
}

fn select_update(current: &Version, releases: &[GithubRelease]) -> Option<UpdateInfo> {
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

            let expected_name =
                format!("kea-{}-windows-x86_64-setup.exe", release.tag_name);
            let asset = release.assets.iter().find(|asset| asset.name == expected_name)?;
            if asset.size == 0 || asset.size > MAX_INSTALLER_BYTES {
                return None;
            }
            let sha256 = parse_sha256(asset.digest.as_deref()?)?;
            Some(UpdateInfo {
                version,
                installer_url: asset.browser_download_url.clone(),
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

fn prepare_update(update: &UpdateInfo) -> Result<()> {
    anyhow::ensure!(supported(), "self-update is not supported on this platform");
    anyhow::ensure!(
        update.size <= MAX_INSTALLER_BYTES,
        "update installer exceeds the download limit"
    );

    let mut response = http_client()?
        .get(&update.installer_url)
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

    let mut staged = tempfile::Builder::new()
        .prefix("kea-update-")
        .suffix(".exe")
        .tempfile()
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

    let (file, staged_path) = staged
        .keep()
        .map_err(|error| error.error)
        .context("cannot retain the staged Kea update")?;
    drop(file);
    if let Err(error) = spawn_install_helper(&staged_path) {
        let _ = fs::remove_file(&staged_path);
        return Err(error);
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
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

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn spawn_install_helper(staged: &Path) -> Result<()> {
    use std::{os::windows::process::CommandExt as _, process::Command};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let current = std::env::current_exe().context("cannot locate the running Kea executable")?;
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .context("LOCALAPPDATA is unavailable")?;
    let installed = local_app_data.join("Programs").join("Kea").join("kea.exe");
    let script = format!(
        "$ErrorActionPreference='Stop';\
         $parent={};$setup={};$fallback={};$installed={};$ok=$false;\
         Wait-Process -Id $parent -ErrorAction SilentlyContinue;\
         Start-Sleep -Milliseconds 150;\
         try {{$process=Start-Process -FilePath $setup -ArgumentList '/S' -Wait -PassThru;$ok=($process.ExitCode -eq 0)}} catch {{$ok=$false}} finally {{Remove-Item -LiteralPath $setup -Force -ErrorAction SilentlyContinue}};\
         if ($ok -and (Test-Path -LiteralPath $installed)) {{Start-Process -FilePath $installed}} elseif (Test-Path -LiteralPath $fallback) {{Start-Process -FilePath $fallback}}",
        std::process::id(),
        powershell_literal(staged),
        powershell_literal(&current),
        powershell_literal(&installed),
    );

    Command::new("powershell.exe")
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

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
fn spawn_install_helper(_: &Path) -> Result<()> {
    anyhow::bail!("self-update is not supported on this platform")
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn powershell_literal(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

#[cfg(test)]
#[path = "../tests/unit/update.rs"]
mod tests;
