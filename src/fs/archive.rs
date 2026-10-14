//! Making and unpacking archives: Compress to .zip, and Extract for .zip,
//! .tar, .tar.gz and .tgz.
//!
//! Both report progress through a callback that can also stop them.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Called with bytes handled so far and the item's name; return false to
/// stop.
pub type Progress<'a> = &'a mut dyn FnMut(u64, &str) -> bool;

/// Whether Extract works on this file.
pub fn is_archive(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    [".zip", ".tar", ".tar.gz", ".tgz"]
        .iter()
        .any(|ext| name.ends_with(ext))
}

/// The name without its archive extension: "photos.tar.gz" → "photos".
pub fn stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();
    for ext in [".tar.gz", ".tgz", ".tar", ".zip"] {
        if lower.ends_with(ext) && lower.len() > ext.len() {
            return name[..name.len() - ext.len()].to_owned();
        }
    }
    name
}

/// The archive's name for these items: "notes.zip" for one item
/// ("notes.txt" or a folder "notes"), "Archive.zip" for several.
pub fn zip_name(sources: &[PathBuf]) -> String {
    match sources {
        [one] => {
            let stem = if one.is_dir() {
                one.file_name().map(|n| n.to_string_lossy().into_owned())
            } else {
                one.file_stem().map(|n| n.to_string_lossy().into_owned())
            };
            format!("{}.zip", stem.unwrap_or_else(|| "Archive".to_owned()))
        }
        _ => "Archive.zip".to_owned(),
    }
}

/// Total size of the files that would go into the archive.
pub fn size_of(sources: &[PathBuf]) -> u64 {
    fn walk(path: &Path) -> u64 {
        match fs::symlink_metadata(path) {
            Ok(m) if m.is_dir() => fs::read_dir(path)
                .map(|d| d.flatten().map(|e| walk(&e.path())).sum())
                .unwrap_or(0),
            Ok(m) => m.len(),
            Err(_) => 0,
        }
    }
    sources.iter().map(|s| walk(s)).sum()
}

/// Zips `sources` (files or folders) into `archive`, each at the top level
/// under its own name. A stopped or failed archive is removed.
pub fn compress(sources: &[PathBuf], archive: &Path, progress: Progress) -> io::Result<()> {
    // create_new: never write over a file that's already there.
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(archive)?;
    let result = write_zip(sources, file, progress);
    if result.is_err() {
        let _ = fs::remove_file(archive);
    }
    result
}

fn write_zip(sources: &[PathBuf], file: fs::File, progress: Progress) -> io::Result<()> {
    let mut zip = zip::ZipWriter::new(io::BufWriter::new(file));
    let mut done = 0u64;
    for source in sources {
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        add(&mut zip, source, &name, &mut done, progress)?;
    }
    zip.finish().map_err(io::Error::other)?;
    Ok(())
}

fn add<W: Write + io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    path: &Path,
    name: &str,
    done: &mut u64,
    progress: Progress,
) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(meta.len() > u32::MAX as u64);
    // Keep permissions, so programs stay runnable after extracting.
    #[cfg(unix)]
    let options = {
        use std::os::unix::fs::PermissionsExt;
        options.unix_permissions(meta.permissions().mode() & 0o7777)
    };
    if meta.is_dir() {
        zip.add_directory(format!("{name}/"), options)
            .map_err(io::Error::other)?;
        let mut children: Vec<_> = fs::read_dir(path)?.flatten().collect();
        children.sort_by_key(|c| c.file_name());
        for child in children {
            let child_name = format!("{name}/{}", child.file_name().to_string_lossy());
            add(zip, &child.path(), &child_name, done, progress)?;
        }
        return Ok(());
    }
    if meta.file_type().is_symlink() {
        // Archive what the link points at, like Explorer and Finder.
        if fs::metadata(path).is_ok_and(|m| m.is_dir()) {
            return Ok(());
        }
    }
    zip.start_file(name, options).map_err(io::Error::other)?;
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        zip.write_all(&buf[..n])?;
        *done += n as u64;
        if !progress(*done, name) {
            return Err(io::ErrorKind::Interrupted.into());
        }
    }
    Ok(())
}

/// Total unpacked size, for progress. Zero when unknown (tar.gz).
pub fn unpacked_size(archive: &Path) -> u64 {
    if !archive.to_string_lossy().to_lowercase().ends_with(".zip") {
        return 0;
    }
    let Ok(file) = fs::File::open(archive) else {
        return 0;
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return 0;
    };
    (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.size()))
        .sum()
}

/// Unpacks `archive` into `dest`, which is created and mustn't exist yet.
/// Entries that would land outside `dest` are skipped. A stopped or
/// failed extraction is removed.
pub fn extract(archive: &Path, dest: &Path, progress: Progress) -> io::Result<()> {
    fs::create_dir(dest)?;
    let lower = archive.to_string_lossy().to_lowercase();
    let result = if lower.ends_with(".zip") {
        extract_zip(archive, dest, progress)
    } else {
        let file = io::BufReader::new(fs::File::open(archive)?);
        if lower.ends_with(".tar") {
            extract_tar(tar::Archive::new(file), dest, progress)
        } else {
            extract_tar(
                tar::Archive::new(flate2::read::GzDecoder::new(file)),
                dest,
                progress,
            )
        }
    };
    if result.is_err() {
        let _ = fs::remove_dir_all(dest);
    }
    result
}

fn extract_zip(archive: &Path, dest: &Path, progress: Progress) -> io::Result<()> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?).map_err(io::Error::other)?;
    let mut done = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(io::Error::other)?;
        // enclosed_name drops names like "../x" or "/etc/x".
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let target = dest.join(&relative);
        let name = relative.to_string_lossy().into_owned();
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&target)?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            done += n as u64;
            if !progress(done, &name) {
                return Err(io::ErrorKind::Interrupted.into());
            }
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o7777))?;
        }
    }
    Ok(())
}

fn extract_tar<R: Read>(
    mut tar: tar::Archive<R>,
    dest: &Path,
    progress: Progress,
) -> io::Result<()> {
    let mut done = 0u64;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        done += entry.size();
        // unpack_in refuses paths that would leave `dest`.
        entry.unpack_in(dest)?;
        if !progress(done, &name) {
            return Err(io::ErrorKind::Interrupted.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(root: &Path) {
        fs::create_dir_all(root.join("notes/sub")).unwrap();
        fs::write(root.join("notes/a.txt"), "alpha").unwrap();
        fs::write(root.join("notes/sub/b.txt"), "beta").unwrap();
        fs::write(root.join("single.txt"), "solo").unwrap();
    }

    fn keep_going() -> impl FnMut(u64, &str) -> bool {
        |_, _| true
    }

    #[test]
    fn names() {
        let dir = tempfile::tempdir().unwrap();
        tree(dir.path());
        assert_eq!(zip_name(&[dir.path().join("notes")]), "notes.zip");
        assert_eq!(zip_name(&[dir.path().join("single.txt")]), "single.zip");
        assert_eq!(
            zip_name(&[dir.path().join("notes"), dir.path().join("single.txt")]),
            "Archive.zip"
        );
        assert_eq!(stem(Path::new("photos.tar.gz")), "photos");
        assert_eq!(stem(Path::new("Backup.ZIP")), "Backup");
        assert_eq!(stem(Path::new("x.tgz")), "x");
        assert!(is_archive(Path::new("a.TAR.GZ")));
        assert!(!is_archive(Path::new("a.txt")));
    }

    #[test]
    fn compress_then_extract_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        tree(dir.path());
        let sources = [dir.path().join("notes"), dir.path().join("single.txt")];
        let archive = dir.path().join("Archive.zip");
        let mut seen = 0;
        compress(&sources, &archive, &mut |done, _| {
            seen = done;
            true
        })
        .unwrap();
        assert_eq!(seen, size_of(&sources));
        assert_eq!(unpacked_size(&archive), 13);

        let out = dir.path().join("Archive");
        extract(&archive, &out, &mut keep_going()).unwrap();
        let read = |p: &str| fs::read_to_string(out.join(p)).unwrap();
        assert_eq!(read("notes/a.txt"), "alpha");
        assert_eq!(read("notes/sub/b.txt"), "beta");
        assert_eq!(read("single.txt"), "solo");
    }

    #[test]
    fn stopping_removes_the_partial_result() {
        let dir = tempfile::tempdir().unwrap();
        tree(dir.path());
        let archive = dir.path().join("notes.zip");
        let err = compress(&[dir.path().join("notes")], &archive, &mut |_, _| false).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert!(!archive.exists());

        compress(&[dir.path().join("notes")], &archive, &mut keep_going()).unwrap();
        let out = dir.path().join("out");
        assert!(extract(&archive, &out, &mut |_, _| false).is_err());
        assert!(!out.exists());
    }

    #[test]
    fn existing_archives_are_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        tree(dir.path());
        let archive = dir.path().join("single.zip");
        fs::write(&archive, "keep me").unwrap();
        assert!(
            compress(
                &[dir.path().join("single.txt")],
                &archive,
                &mut keep_going()
            )
            .is_err()
        );
        // The failed attempt mustn't delete what was already there.
        assert_eq!(fs::read_to_string(&archive).unwrap(), "keep me");
    }

    #[test]
    fn tarballs_extract() {
        let dir = tempfile::tempdir().unwrap();
        tree(dir.path());
        let tgz = dir.path().join("notes.tar.gz");
        {
            let file = fs::File::create(&tgz).unwrap();
            let gz = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            let mut tar = tar::Builder::new(gz);
            tar.append_dir_all("notes", dir.path().join("notes"))
                .unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let out = dir.path().join("notes-out");
        extract(&tgz, &out, &mut keep_going()).unwrap();
        assert_eq!(
            fs::read_to_string(out.join("notes/sub/b.txt")).unwrap(),
            "beta"
        );
    }

    #[test]
    fn zip_entries_cannot_escape() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.zip");
        {
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("../escaped.txt", options).unwrap();
            zip.write_all(b"x").unwrap();
            zip.start_file("fine.txt", options).unwrap();
            zip.write_all(b"y").unwrap();
            zip.finish().unwrap();
        }
        let out = dir.path().join("out");
        extract(&archive, &out, &mut keep_going()).unwrap();
        assert!(!dir.path().join("escaped.txt").exists());
        assert!(out.join("fine.txt").exists());
    }
}
