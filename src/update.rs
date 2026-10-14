//! Checking GitHub for a newer release, and installing it.
//!
//! Releases are built by `.github/workflows/release.yml`, which attaches
//! one download per platform to each tagged release:
//! `explorist-<version>-windows-x86_64.zip`,
//! `explorist-<version>-linux-x86_64.tar.gz` and
//! `explorist-<version>-macos-universal.zip` (holding Explorist.app).

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use serde::Deserialize;

const REPO: &str = "Rick007110/Explorist";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// Largest download accepted.
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

/// A version number: major.minor.patch with an optional pre-release tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    parts: [u64; 3],
    pre: Option<String>,
}

impl Version {
    /// Reads "0.2.0", "v0.2.0" or "1.0.0-beta.1".
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches(['v', 'V']);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre.to_owned())),
            None => (text, None),
        };
        let mut parts = [0u64; 3];
        let mut count = 0;
        for (i, part) in core.split('.').enumerate() {
            if i >= 3 {
                return None;
            }
            parts[i] = part.parse().ok()?;
            count += 1;
        }
        (count > 0).then_some(Self { parts, pre })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // A pre-release comes before its release: 1.0.0-beta < 1.0.0.
        self.parts
            .cmp(&other.parts)
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => std::cmp::Ordering::Equal,
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

/// A published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    /// The download for this platform, if the release has one.
    pub download: Option<String>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// The platform word in download names.
const PLATFORM: &str = if cfg!(windows) {
    "windows"
} else if cfg!(target_os = "macos") {
    "macos"
} else {
    "linux"
};

/// Picks this platform's download from asset names: the platform word, an
/// archive, and the right processor (or a universal build).
fn pick_asset<'a>(
    names: impl Iterator<Item = &'a str>,
    platform: &str,
    arch: &str,
) -> Option<&'a str> {
    let candidates: Vec<&str> = names
        .filter(|n| {
            let n = n.to_lowercase();
            n.contains(platform) && (n.ends_with(".zip") || n.ends_with(".tar.gz"))
        })
        .collect();
    let fits = |n: &&str| {
        let n = n.to_lowercase();
        n.contains(arch) || n.contains("universal")
    };
    candidates.iter().copied().find(fits).or_else(|| {
        // An archive without any processor in its name fits any.
        candidates.iter().copied().find(|n| {
            let n = n.to_lowercase();
            !["x86_64", "aarch64", "arm64"].iter().any(|a| n.contains(a))
        })
    })
}

/// Reads GitHub's "latest release" JSON.
pub fn parse_release(json: &str) -> Result<Release, String> {
    parse_release_for(json, PLATFORM, std::env::consts::ARCH)
}

fn parse_release_for(json: &str, platform: &str, arch: &str) -> Result<Release, String> {
    let api: ApiRelease = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if api.draft || api.prerelease {
        return Err("not a final release".to_owned());
    }
    let picked = pick_asset(api.assets.iter().map(|a| a.name.as_str()), platform, arch);
    let download = picked.and_then(|name| {
        api.assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
    });
    Ok(Release {
        version: api.tag_name.trim_start_matches(['v', 'V']).to_owned(),
        notes: api.body.unwrap_or_default(),
        page: api.html_url,
        download,
    })
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckResult {
    /// A newer version is out.
    Available(Release),
    UpToDate,
    /// No releases could be reached (none yet, or the repository is private).
    NoReleases,
    Failed(String),
}

fn agent() -> ureq::Agent {
    let provider = if cfg!(windows) {
        ureq::tls::TlsProvider::NativeTls
    } else {
        ureq::tls::TlsProvider::Rustls
    };
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(provider)
                // Trust what the system trusts, so company proxies work.
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

/// Asks GitHub for the latest release and compares it with this version.
pub fn check() -> CheckResult {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = agent()
        .get(&url)
        .header("User-Agent", &format!("Explorist/{CURRENT}"))
        .header("Accept", "application/vnd.github+json")
        .call();
    let mut response = match response {
        Ok(r) => r,
        Err(err) => return CheckResult::Failed(err.to_string()),
    };
    match response.status().as_u16() {
        200 => {}
        404 => return CheckResult::NoReleases,
        403 | 429 => {
            return CheckResult::Failed(
                "GitHub's rate limit was reached; try again later".to_owned(),
            );
        }
        code => return CheckResult::Failed(format!("GitHub answered {code}")),
    }
    let body = match response.body_mut().read_to_string() {
        Ok(body) => body,
        Err(err) => return CheckResult::Failed(err.to_string()),
    };
    compare(parse_release(&body), CURRENT)
}

fn compare(release: Result<Release, String>, current: &str) -> CheckResult {
    let release = match release {
        Ok(r) => r,
        Err(err) => return CheckResult::Failed(err),
    };
    match (Version::parse(&release.version), Version::parse(current)) {
        (Some(latest), Some(mine)) if latest > mine => CheckResult::Available(release),
        (Some(_), Some(_)) => CheckResult::UpToDate,
        _ => CheckResult::Failed(format!("can’t read version “{}”", release.version)),
    }
}

/// Runs [`check`] on a background thread.
pub fn check_in_background(wake: impl Fn() + Send + 'static) -> Receiver<CheckResult> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("explorist-update-check".into())
        .spawn(move || {
            if tx.send(check()).is_ok() {
                wake();
            }
        })
        .ok();
    rx
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstallEvent {
    /// Bytes downloaded and expected total (0 if unknown).
    Progress(u64, u64),
    /// Installed; restart to use it.
    Done,
    Failed(String),
}

/// Downloads the archive at `url`, takes the program out of it and puts it
/// in place of the running one.
pub fn install_in_background(
    url: String,
    wake: impl Fn() + Send + 'static,
) -> Receiver<InstallEvent> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("explorist-update-install".into())
        .spawn(move || {
            let progress = |done, total| {
                let _ = tx.send(InstallEvent::Progress(done, total));
                wake();
            };
            let event = match install(&url, &progress) {
                Ok(()) => InstallEvent::Done,
                Err(err) => InstallEvent::Failed(err.to_string()),
            };
            let _ = tx.send(event);
            wake();
        })
        .ok();
    rx
}

fn install(url: &str, progress: &dyn Fn(u64, u64)) -> io::Result<()> {
    let mut response = agent()
        .get(url)
        .header("User-Agent", &format!("Explorist/{CURRENT}"))
        .call()
        .map_err(io::Error::other)?;
    if response.status().as_u16() != 200 {
        return Err(io::Error::other(format!(
            "download answered {}",
            response.status()
        )));
    }
    let total = response.body().content_length().unwrap_or(0);
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .reader();
    let mut zip_bytes = Vec::with_capacity(total as usize);
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        zip_bytes.extend_from_slice(&chunk[..n]);
        progress(zip_bytes.len() as u64, total);
    }

    let dir = std::env::temp_dir().join(format!("explorist-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let exe = if url.to_lowercase().ends_with(".tar.gz") {
        extract_from_tar_gz(&zip_bytes, &dir)?
    } else {
        extract_exe(&zip_bytes, &dir)?
    };
    self_replace::self_replace(&exe)?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// The program's file name inside an archive.
const BINARY: &str = if cfg!(windows) {
    "explorist.exe"
} else {
    "explorist"
};

/// Whether an archive path is the program: `.../explorist.exe` on Windows,
/// `.../explorist` elsewhere (including inside `Explorist.app`).
fn is_binary(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.eq_ignore_ascii_case(BINARY)
}

/// Writes the zip's program into `dir`.
fn extract_exe(zip_bytes: &[u8], dir: &Path) -> io::Result<PathBuf> {
    let mut archive = zip::ZipArchive::new(io::Cursor::new(zip_bytes)).map_err(io::Error::other)?;
    let index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index(i)
                .is_ok_and(|f| f.name().is_ok_and(|n| is_binary(&n)))
        })
        .ok_or_else(|| io::Error::other(format!("the download has no {BINARY}")))?;
    let mut file = archive.by_index(index).map_err(io::Error::other)?;
    write_binary(&mut file, dir)
}

/// Writes the .tar.gz's program into `dir`.
fn extract_from_tar_gz(bytes: &[u8], dir: &Path) -> io::Result<PathBuf> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        if entry.header().entry_type().is_file() && is_binary(&path) {
            return write_binary(&mut entry, dir);
        }
    }
    Err(io::Error::other(format!("the download has no {BINARY}")))
}

fn write_binary(from: &mut impl Read, dir: &Path) -> io::Result<PathBuf> {
    let target = dir.join(BINARY);
    let mut out = std::fs::File::create(&target)?;
    io::copy(from, &mut out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(target)
}

/// Starts the (updated) program again with the same arguments.
pub fn restart() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn versions_compare() {
        assert!(v("0.2.0") > v("0.1.9"));
        assert!(v("v1.0.0") > v("0.99.99"));
        assert!(v("1.0.0") > v("1.0.0-beta.2"));
        assert!(v("1.0.0-beta.2") > v("1.0.0-beta.1"));
        assert!(v("1.10.0") > v("1.9.0"));
        assert_eq!(v("1.2"), v("1.2.0"));
        assert!(Version::parse("latest").is_none());
        assert!(Version::parse("1.2.3.4").is_none());
    }

    const JSON: &str = r#"{
        "tag_name": "v0.2.0",
        "html_url": "https://github.com/Rick007110/Explorist/releases/tag/v0.2.0",
        "body": "Tabs and git status",
        "draft": false,
        "prerelease": false,
        "assets": [
            {"name": "explorist-0.2.0-windows-x86_64.zip", "browser_download_url": "https://example.com/win.zip"},
            {"name": "checksums.txt", "browser_download_url": "https://example.com/sums"}
        ]
    }"#;

    #[test]
    fn reads_release_json() {
        let release = parse_release_for(JSON, "windows", "x86_64").unwrap();
        assert_eq!(release.version, "0.2.0");
        assert_eq!(release.notes, "Tabs and git status");
        assert_eq!(
            release.download.as_deref(),
            Some("https://example.com/win.zip")
        );
    }

    #[test]
    fn picks_the_download_for_each_platform() {
        let names = [
            "explorist-0.2.0-windows-x86_64.zip",
            "explorist-0.2.0-linux-x86_64.tar.gz",
            "explorist-0.2.0-linux-aarch64.tar.gz",
            "explorist-0.2.0-macos-universal.zip",
            "checksums.txt",
        ];
        let pick = |platform, arch| pick_asset(names.iter().copied(), platform, arch);
        assert_eq!(pick("windows", "x86_64"), Some(names[0]));
        assert_eq!(pick("linux", "x86_64"), Some(names[1]));
        assert_eq!(pick("linux", "aarch64"), Some(names[2]));
        assert_eq!(pick("macos", "aarch64"), Some(names[3]));
        assert_eq!(pick("macos", "x86_64"), Some(names[3]));
        assert_eq!(
            pick("windows", "aarch64"),
            None,
            "no build for that processor"
        );
        assert_eq!(
            parse_release_for(JSON, "linux", "x86_64").unwrap().download,
            None
        );
    }

    #[test]
    fn newer_releases_are_offered() {
        assert!(matches!(
            compare(parse_release_for(JSON, "windows", "x86_64"), "0.1.0"),
            CheckResult::Available(_)
        ));
        assert_eq!(compare(parse_release(JSON), "0.2.0"), CheckResult::UpToDate);
        assert_eq!(compare(parse_release(JSON), "0.3.0"), CheckResult::UpToDate);
    }

    #[test]
    fn prereleases_are_ignored() {
        let json = JSON.replace(r#""prerelease": false"#, r#""prerelease": true"#);
        assert!(matches!(
            compare(parse_release(&json), "0.1.0"),
            CheckResult::Failed(_)
        ));
    }

    #[test]
    fn extracts_from_tar_gz() {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        {
            let mut tar = tar::Builder::new(&mut gz);
            let data = b"\x7fELF fake";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, format!("explorist-0.2.0/{BINARY}"), &data[..])
                .unwrap();
            tar.finish().unwrap();
        }
        let bytes = gz.finish().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let exe = extract_from_tar_gz(&bytes, dir.path()).unwrap();
        assert_eq!(std::fs::read(exe).unwrap(), b"\x7fELF fake");
    }

    #[test]
    fn extracts_the_exe() {
        let mut buf = io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("README.txt", opts).unwrap();
            io::Write::write_all(&mut zip, b"hi").unwrap();
            zip.start_file(format!("Explorist.app/Contents/MacOS/{BINARY}"), opts)
                .unwrap();
            io::Write::write_all(&mut zip, b"MZ fake exe").unwrap();
            zip.finish().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let exe = extract_exe(buf.get_ref(), dir.path()).unwrap();
        assert_eq!(std::fs::read(exe).unwrap(), b"MZ fake exe");
    }
}
