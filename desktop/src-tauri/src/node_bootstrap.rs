//! Node.js bootstrap: detect a usable `node` on the system (≥ MIN_VERSION),
//! and if none is found, download a portable Node.js build from nodejs.org
//! into the app's cache directory and return the path to its `bin/node`.
//!
//! The download is streamed directly to disk with progress callbacks, and the
//! resulting archive is extracted next to the download file. Once extracted, a
//! sentinel file marks the installation complete so subsequent runs skip the
//! download entirely.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Minimum Node.js version required by dsh (matches package.json engines).
const MIN_VERSION: u32 = 22;
/// Specific Node.js LTS version to download when none is found.
/// Pinned to a known-good LTS so the portable build is reproducible.
const DOWNLOAD_VERSION: &str = "v22.19.0";

/// Return the path to a usable `node` binary, downloading a portable build if
/// no system Node.js satisfies the minimum version requirement.
///
/// `system_path` is the $PATH value used when probing for system node.
/// `cache_dir` is where the portable build is stored.
/// `on_progress` receives `(downloaded_bytes, total_bytes)` updates during
/// the download (total is 0 if the content-length header is missing).
pub fn ensure_node<P: AsRef<Path>, F: Fn(u64, u64)>(
    system_path: &str,
    cache_dir: P,
    on_progress: F,
) -> Result<PathBuf, String> {
    // 1. Try system node first.
    if let Some(path) = find_system_node(system_path) {
        return Ok(path);
    }

    // 2. Check for an already-downloaded portable build.
    let cache = cache_dir.as_ref().to_path_buf();
    let install_dir = install_dir(&cache);
    let node_bin = node_bin_path(&install_dir);
    let marker = install_dir.join(".installed");

    if marker.exists() && node_bin.exists() {
        if let Some(path) = verify_node(&node_bin) {
            return Ok(path);
        }
    }

    // 3. Download and extract a portable Node.js build.
    download_portable_node(&cache, &install_dir, &node_bin, &marker, &on_progress)?;

    verify_node(&node_bin).ok_or_else(|| "downloaded node binary failed verification".into())
}

// ---------------------------------------------------------------------------
// System node detection
// ---------------------------------------------------------------------------

/// Check whether a `node` binary on `path` reports major version ≥ MIN_VERSION.
/// Returns the bare "node" name (resolved via PATH at spawn time) if found.
fn find_system_node(path: &str) -> Option<PathBuf> {
    let output = Command::new("node")
        .arg("--version")
        .env("PATH", path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout);
    let major = parse_major_version(&version)?;
    if major >= MIN_VERSION {
        Some(PathBuf::from("node"))
    } else {
        None
    }
}

/// If `binary` exists and reports a major version ≥ MIN_VERSION, return it.
fn verify_node(binary: &Path) -> Option<PathBuf> {
    if !binary.exists() {
        return None;
    }
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout);
    let major = parse_major_version(&version)?;
    if major >= MIN_VERSION {
        Some(binary.to_path_buf())
    } else {
        None
    }
}

fn parse_major_version(version: &str) -> Option<u32> {
    // expects "v22.19.0\n" or "22.19.0"
    let v = version.trim().trim_start_matches('v');
    v.split('.').next()?.parse().ok()
}

fn node_exe_name() -> &'static str {
    if cfg!(windows) { "node.exe" } else { "node" }
}

// ---------------------------------------------------------------------------
// Portable build — download + extract
// ---------------------------------------------------------------------------

fn install_dir(cache: &Path) -> PathBuf {
    cache.join(format!("node-{}", DOWNLOAD_VERSION))
}

fn node_bin_path(install_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        install_dir.join("node.exe")
    } else {
        install_dir.join("bin").join("node")
    }
}

/// Platform segment used in the nodejs.org download URL.
fn platform_arch_dir() -> Result<&'static str, String> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { Ok("darwin-arm64") }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { Ok("darwin-x64") }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { Ok("linux-x64") }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    { Ok("linux-arm64") }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { Ok("win-x64") }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        Err(format!(
            "unsupported platform for automatic Node.js download: {}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH,
        ))
    }
}

fn archive_ext() -> &'static str {
    if cfg!(windows) { "zip" } else { "tar.gz" }
}

fn download_url() -> Result<String, String> {
    let plat = platform_arch_dir()?;
    Ok(format!(
        "https://nodejs.org/dist/{version}/node-{version}-{plat}.{ext}",
        version = DOWNLOAD_VERSION,
        plat = plat,
        ext = archive_ext(),
    ))
}

fn download_portable_node<F: Fn(u64, u64)>(
    cache: &Path,
    install_dir: &Path,
    _node_bin: &Path,
    marker: &Path,
    on_progress: &F,
) -> Result<(), String> {
    let url = download_url()?;
    std::fs::create_dir_all(cache)
        .map_err(|e| format!("failed to create cache dir: {e}"))?;

    let archive_path = cache.join(format!("node-{}.{}", DOWNLOAD_VERSION, archive_ext()));

    // Download with progress.
    download_file(&url, &archive_path, on_progress)?;

    // Extract.
    extract_archive(&archive_path, install_dir)?;

    // Clean up archive to save space.
    let _ = std::fs::remove_file(&archive_path);

    // Write marker so we skip the download next run.
    std::fs::write(marker, DOWNLOAD_VERSION)
        .map_err(|e| format!("failed to write install marker: {e}"))?;

    Ok(())
}

fn download_file<F: Fn(u64, u64)>(
    url: &str,
    dest: &Path,
    on_progress: &F,
) -> Result<(), String> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;

    let total: u64 = resp
        .header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest)
        .map_err(|e| format!("failed to create download file: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    loop {
        use std::io::Read;
        let n = reader.read(&mut buf).map_err(|e| format!("download read error: {e}"))?;
        if n == 0 { break; }
        use std::io::Write;
        file.write_all(&buf[..n])
            .map_err(|e| format!("download write error: {e}"))?;
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    file.flush().ok();
    Ok(())
}

fn extract_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        extract_zip(archive, dest)
    }
    #[cfg(not(windows))]
    {
        extract_tar_gz(archive, dest)
    }
}

#[cfg(not(windows))]
fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    use std::fs::File;
    use flate2::read::GzDecoder;
    use tar::Archive;

    let file = File::open(archive).map_err(|e| format!("open archive: {e}"))?;
    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);

    // The official tarball has a top-level directory like `node-v22.19.0-darwin-arm64/`.
    // We want its *contents* at `dest`, not the directory itself.
    let tmp_parent = dest
        .parent()
        .ok_or_else(|| "invalid install dest path".to_string())?
        .to_path_buf();
    std::fs::create_dir_all(&tmp_parent)
        .map_err(|e| format!("create extract parent: {e}"))?;

    // Remove any existing partial install.
    if dest.exists() {
        let _ = std::fs::remove_dir_all(dest);
    }

    let temp_extract = tmp_parent.join(format!(".node-extract-{}", std::process::id()));
    if temp_extract.exists() {
        let _ = std::fs::remove_dir_all(&temp_extract);
    }

    archive
        .unpack(&temp_extract)
        .map_err(|e| format!("extract archive: {e}"))?;

    // Find the top-level directory inside the tarball.
    let entries = std::fs::read_dir(&temp_extract)
        .map_err(|e| format!("read extracted dir: {e}"))?;
    let mut top_dir: Option<PathBuf> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            top_dir = Some(path);
            break;
        }
    }
    let top_dir = top_dir.ok_or_else(|| "no top-level directory in node tarball".to_string())?;

    std::fs::rename(&top_dir, dest)
        .or_else(|_| {
            // Fallback: copy (cross-filesystem rename may fail)
            copy_dir_all(&top_dir, dest)
        })
        .map_err(|e| format!("move extracted node: {e}"))?;

    let _ = std::fs::remove_dir_all(&temp_extract);
    Ok(())
}

#[cfg(windows)]
fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    use std::fs::File;
    use std::io::BufReader;
    use zip::ZipArchive;

    let file = File::open(archive).map_err(|e| format!("open zip: {e}"))?;
    let mut zip = ZipArchive::new(BufReader::new(file))
        .map_err(|e| format!("read zip: {e}"))?;

    if dest.exists() {
        let _ = std::fs::remove_dir_all(dest);
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("create dest: {e}"))?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("zip entry: {e}"))?;
        let name = entry.name().to_string();
        // Strip the top-level directory (e.g. "node-v22.19.0-win-x64/").
        let rel: &str = match name.find('/') {
            Some(pos) => &name[pos + 1..],
            None => continue, // skip the top-level dir entry itself
        };
        if rel.is_empty() { continue; }

        let out_path = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("create dir {}: {e}", rel))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            use std::io::Write;
            let mut out = File::create(&out_path)
                .map_err(|e| format!("create file {}: {e}", rel))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("write file {}: {e}", rel))?;
        }
    }
    Ok(())
}

/// Recursively copy a directory. Used as fallback when rename fails
/// (e.g. cross-filesystem).
#[cfg(not(windows))]
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}
