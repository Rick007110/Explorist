//! What a file is: its icon category and the text in the Type column.

use std::borrow::Cow;

/// Icon category. The design has one 16 px outline icon per category,
/// told apart by silhouette; only folders take the accent color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    Folder,
    Code,
    Text,
    Config,
    Lock,
    Pdf,
    Image,
    Vector,
    Archive,
    Audio,
    Video,
    Executable,
    Other,
}

/// Classifies a file by name. `is_dir` wins over the name.
pub fn classify(name: &str, is_dir: bool) -> (FileKind, Cow<'static, str>) {
    if is_dir {
        return (FileKind::Folder, Cow::Borrowed("Folder"));
    }

    let lower = name.to_ascii_lowercase();
    if let Some(known) = by_full_name(&lower) {
        return known;
    }

    let Some(ext) = extension(&lower) else {
        return (FileKind::Other, Cow::Borrowed("File"));
    };
    if let Some((kind, label)) = by_extension(ext) {
        return (kind, Cow::Borrowed(label));
    }
    (
        FileKind::Other,
        Cow::Owned(format!("{} file", ext.to_ascii_uppercase())),
    )
}

/// The extension without the dot, or `None` for dotfiles like `.gitignore`
/// and names without one.
fn extension(lower_name: &str) -> Option<&str> {
    let (stem, ext) = lower_name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then_some(ext)
}

fn by_full_name(lower: &str) -> Option<(FileKind, Cow<'static, str>)> {
    let found = match lower {
        ".gitignore" | ".gitattributes" | ".gitmodules" => (FileKind::Config, "Git config"),
        ".editorconfig" => (FileKind::Config, "EditorConfig"),
        ".env" => (FileKind::Config, "Environment file"),
        "license" | "licence" | "copying" | "authors" | "notice" => (FileKind::Text, "Text file"),
        "makefile" => (FileKind::Code, "Makefile"),
        "dockerfile" => (FileKind::Config, "Dockerfile"),
        "package-lock.json" => (FileKind::Lock, "Lockfile"),
        _ => return None,
    };
    Some((found.0, Cow::Borrowed(found.1)))
}

fn by_extension(ext: &str) -> Option<(FileKind, &'static str)> {
    use FileKind::*;
    Some(match ext {
        "rs" => (Code, "Rust source"),
        "c" | "h" => (Code, "C source"),
        "cpp" | "cc" | "cxx" | "hpp" => (Code, "C++ source"),
        "cs" => (Code, "C# source"),
        "go" => (Code, "Go source"),
        "java" => (Code, "Java source"),
        "kt" => (Code, "Kotlin source"),
        "py" => (Code, "Python script"),
        "js" | "mjs" | "cjs" => (Code, "JavaScript file"),
        "ts" | "mts" | "cts" => (Code, "TypeScript file"),
        "jsx" | "tsx" => (Code, "React component"),
        "html" | "htm" => (Code, "HTML document"),
        "css" | "scss" => (Code, "Stylesheet"),
        "lua" | "luau" => (Code, "Lua script"),
        "sh" | "bash" => (Code, "Shell script"),
        "ps1" => (Code, "PowerShell script"),
        "bat" | "cmd" => (Code, "Batch file"),
        "sql" => (Code, "SQL file"),

        "txt" => (Text, "Text file"),
        "md" | "markdown" => (Text, "Markdown"),
        "log" => (Text, "Log file"),
        "csv" => (Text, "CSV file"),
        "rtf" => (Text, "Rich text"),
        "doc" | "docx" => (Text, "Word document"),
        "xls" | "xlsx" => (Text, "Excel workbook"),
        "ppt" | "pptx" => (Text, "PowerPoint deck"),

        "toml" => (Config, "TOML document"),
        "json" => (Config, "JSON file"),
        "yaml" | "yml" => (Config, "YAML file"),
        "xml" => (Config, "XML document"),
        "ini" | "cfg" | "conf" => (Config, "Settings file"),

        "lock" => (Lock, "Lockfile"),

        "pdf" => (Pdf, "PDF document"),

        "png" => (Image, "PNG image"),
        "jpg" | "jpeg" => (Image, "JPEG image"),
        "gif" => (Image, "GIF image"),
        "webp" => (Image, "WebP image"),
        "bmp" => (Image, "Bitmap image"),
        "ico" => (Image, "Icon"),
        "tif" | "tiff" => (Image, "TIFF image"),
        "heic" => (Image, "HEIC image"),
        "psd" => (Image, "Photoshop document"),

        "svg" => (Vector, "SVG image"),
        "ai" => (Vector, "Illustrator document"),
        "eps" => (Vector, "EPS image"),

        "zip" => (Archive, "ZIP archive"),
        "7z" => (Archive, "7-Zip archive"),
        "rar" => (Archive, "RAR archive"),
        "tar" => (Archive, "TAR archive"),
        "gz" | "tgz" => (Archive, "Gzip archive"),
        "xz" => (Archive, "XZ archive"),
        "iso" => (Archive, "Disc image"),

        "mp3" => (Audio, "MP3 audio"),
        "wav" => (Audio, "WAV audio"),
        "flac" => (Audio, "FLAC audio"),
        "ogg" => (Audio, "Ogg audio"),
        "m4a" => (Audio, "M4A audio"),

        "mp4" => (Video, "MP4 video"),
        "mkv" => (Video, "MKV video"),
        "mov" => (Video, "QuickTime video"),
        "avi" => (Video, "AVI video"),
        "webm" => (Video, "WebM video"),

        "exe" => (Executable, "Application"),
        "msi" => (Executable, "Windows installer"),
        "dll" => (Executable, "Library"),
        "lnk" => (Executable, "Shortcut"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(name: &str) -> String {
        classify(name, false).1.into_owned()
    }

    #[test]
    fn folders_are_folders_whatever_their_name() {
        assert_eq!(classify("src", true), (FileKind::Folder, "Folder".into()));
        assert_eq!(classify("photos.zip", true).0, FileKind::Folder);
    }

    #[test]
    fn labels_match_the_design() {
        assert_eq!(label(".gitignore"), "Git config");
        assert_eq!(label("build.rs"), "Rust source");
        assert_eq!(label("Cargo.lock"), "Lockfile");
        assert_eq!(label("Cargo.toml"), "TOML document");
        assert_eq!(label("design-notes.pdf"), "PDF document");
        assert_eq!(label("hero.png"), "PNG image");
        assert_eq!(label("LICENSE"), "Text file");
        assert_eq!(label("README.md"), "Markdown");
        assert_eq!(label("release-0.4.0.zip"), "ZIP archive");
        assert_eq!(label("logo.svg"), "SVG image");
    }

    #[test]
    fn extensions_are_case_insensitive() {
        assert_eq!(classify("PHOTO.JPG", false).0, FileKind::Image);
        assert_eq!(label("Setup.EXE"), "Application");
    }

    #[test]
    fn unknown_extensions_get_a_generic_label() {
        assert_eq!(
            classify("data.xyz", false),
            (FileKind::Other, "XYZ file".into())
        );
    }

    #[test]
    fn names_without_an_extension_are_plain_files() {
        assert_eq!(label("notes"), "File");
        assert_eq!(label(".hidden"), "File");
        assert_eq!(label("trailing."), "File");
    }
}
