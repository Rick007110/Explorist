//! Thumbnails from the system, for files Explorist can't decode itself:
//! videos above all, and image types like HEIC on Windows and macOS.
//!
//! - Windows: the shell's thumbnails (`IShellItemImageFactory`), the same
//!   ones Explorer shows, from whatever codecs are installed.
//! - macOS: Quick Look, through `qlmanage`.
//! - Linux: a frame from `ffmpegthumbnailer` or `ffmpeg`, when installed.

use std::path::Path;

/// A picture for `path` about `side` pixels across, or `None`.
pub fn thumbnail(path: &Path, side: u32) -> Option<image::RgbaImage> {
    platform::thumbnail(path, side)
}

/// Whether this computer can make video thumbnails at all. When it can't,
/// a missing thumbnail isn't remembered, so installing a tool helps.
pub fn available() -> bool {
    platform::available()
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW, HBITMAP,
    };
    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    use windows_sys::Win32::UI::Shell::{
        SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_THUMBNAILONLY,
    };
    use windows_sys::core::{GUID, HRESULT};

    const IID_ISHELLITEMIMAGEFACTORY: GUID =
        GUID::from_u128(0xbcc18b79_ba16_442f_80c4_8a59c30c463b);

    /// `IShellItemImageFactory`'s methods: IUnknown's three, then GetImage.
    #[repr(C)]
    struct Vtbl {
        _query_interface: usize,
        _add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_image: unsafe extern "system" fn(*mut c_void, SIZE, i32, *mut HBITMAP) -> HRESULT,
    }

    pub fn available() -> bool {
        true
    }

    pub fn thumbnail(path: &Path, side: u32) -> Option<image::RgbaImage> {
        thread_local! {
            // Each worker thread joins COM once; failure means it already had.
            static COM: () = unsafe {
                CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as _);
            };
        }
        COM.with(|_| ());
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut factory: *mut c_void = std::ptr::null_mut();
        // SAFETY: `wide` is a NUL-terminated path; `factory` receives an
        // interface pointer we release below.
        let hr = unsafe {
            SHCreateItemFromParsingName(
                wide.as_ptr(),
                std::ptr::null_mut(),
                &IID_ISHELLITEMIMAGEFACTORY,
                &mut factory,
            )
        };
        if hr < 0 || factory.is_null() {
            return None;
        }
        let size = SIZE {
            cx: side as i32,
            cy: side as i32,
        };
        let mut bitmap: HBITMAP = std::ptr::null_mut();
        // SAFETY: `factory` is a live IShellItemImageFactory, whose vtable
        // starts with IUnknown's methods followed by GetImage.
        let hr = unsafe {
            let vtbl = *(factory as *const *const Vtbl);
            let hr = ((*vtbl).get_image)(
                factory,
                size,
                SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK,
                &mut bitmap,
            );
            ((*vtbl).release)(factory);
            hr
        };
        if hr < 0 || bitmap.is_null() {
            return None;
        }
        let image = pixels(bitmap);
        // SAFETY: we own the bitmap GetImage returned.
        unsafe { DeleteObject(bitmap) };
        image
    }

    /// The bitmap's pixels as RGBA.
    fn pixels(bitmap: HBITMAP) -> Option<image::RgbaImage> {
        // SAFETY: plain GDI calls on a bitmap we own, into buffers sized
        // from its own dimensions.
        unsafe {
            let mut info: BITMAP = std::mem::zeroed();
            if GetObjectW(
                bitmap,
                std::mem::size_of::<BITMAP>() as i32,
                (&mut info as *mut BITMAP).cast(),
            ) == 0
            {
                return None;
            }
            let (w, h) = (info.bmWidth, info.bmHeight.abs());
            if w <= 0 || h <= 0 {
                return None;
            }
            let mut header: BITMAPINFO = std::mem::zeroed();
            header.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                // Negative: rows top to bottom.
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut data = vec![0u8; w as usize * h as usize * 4];
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            let rows = GetDIBits(
                dc,
                bitmap,
                0,
                h as u32,
                data.as_mut_ptr().cast(),
                &mut header,
                DIB_RGB_COLORS,
            );
            DeleteDC(dc);
            if rows == 0 {
                return None;
            }
            super::from_bgra(&mut data);
            image::RgbaImage::from_raw(w as u32, h as u32, data)
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::Path;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub fn available() -> bool {
        true
    }

    /// `qlmanage -t` writes `<name>.png` into a folder of our own.
    pub fn thumbnail(path: &Path, side: u32) -> Option<image::RgbaImage> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let out = std::env::temp_dir().join(format!(
            "explorist-ql-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&out).ok()?;
        let mut command = Command::new("/usr/bin/qlmanage");
        command
            .args(["-t", "-s", &side.to_string(), "-o"])
            .arg(&out)
            .arg(path);
        let made = super::run_quietly(command).and_then(|_| {
            let name = format!("{}.png", path.file_name()?.to_string_lossy());
            image::open(out.join(name)).ok()
        });
        let _ = std::fs::remove_dir_all(&out);
        Some(made?.to_rgba8())
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::OnceLock;

    fn program(name: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|p| p.is_file())
    }

    fn tools() -> &'static (Option<PathBuf>, Option<PathBuf>) {
        static TOOLS: OnceLock<(Option<PathBuf>, Option<PathBuf>)> = OnceLock::new();
        TOOLS.get_or_init(|| (program("ffmpegthumbnailer"), program("ffmpeg")))
    }

    pub fn available() -> bool {
        let (thumbnailer, ffmpeg) = tools();
        thumbnailer.is_some() || ffmpeg.is_some()
    }

    /// A frame as PNG on stdout: ffmpegthumbnailer picks a good one; with
    /// plain ffmpeg the `thumbnail` filter skips black opening frames.
    pub fn thumbnail(path: &Path, side: u32) -> Option<image::RgbaImage> {
        let (thumbnailer, ffmpeg) = tools();
        let png = if let Some(thumbnailer) = thumbnailer {
            let mut command = Command::new(thumbnailer);
            command
                .args(["-c", "png", "-s", &side.to_string(), "-o", "-", "-i"])
                .arg(path);
            super::run_quietly(command)
        } else {
            let mut command = Command::new(ffmpeg.as_ref()?);
            command
                .args(["-v", "error", "-nostdin", "-i"])
                .arg(path)
                .args([
                    "-vf",
                    &format!(
                        "thumbnail=30,scale={side}:{side}:force_original_aspect_ratio=decrease"
                    ),
                    "-frames:v",
                    "1",
                    "-f",
                    "image2pipe",
                    "-c:v",
                    "png",
                    "-",
                ]);
            super::run_quietly(command)
        }?;
        image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .ok()
            .map(|i| i.to_rgba8())
    }
}

/// Runs a helper program and returns what it printed, giving up after a
/// while on files that make it hang.
#[cfg(not(windows))]
fn run_quietly(mut command: std::process::Command) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        stdout.read_to_end(&mut out).ok().map(|_| out)
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader.join().ok().flatten()?;
                return status.success().then_some(out);
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Windows bitmaps are BGRA, with alpha premultiplied or, for many video
/// frames, left at zero. Makes it plain RGBA.
#[cfg_attr(not(windows), allow(dead_code))]
fn from_bgra(data: &mut [u8]) {
    let no_alpha = data.chunks_exact(4).all(|px| px[3] == 0);
    for px in data.chunks_exact_mut(4) {
        px.swap(0, 2);
        if no_alpha {
            px[3] = 255;
        } else if px[3] != 0 && px[3] != 255 {
            let a = u16::from(px[3]);
            for c in &mut px[..3] {
                *c = ((u16::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmaps_become_rgba() {
        // A video frame: no alpha at all.
        let mut frame = vec![10, 20, 30, 0, 40, 50, 60, 0];
        from_bgra(&mut frame);
        assert_eq!(frame, [30, 20, 10, 255, 60, 50, 40, 255]);
        // Premultiplied, half transparent.
        let mut icon = vec![50, 0, 100, 128, 0, 0, 0, 0];
        from_bgra(&mut icon);
        assert_eq!(icon, [199, 0, 100, 128, 0, 0, 0, 0]);
    }
}
