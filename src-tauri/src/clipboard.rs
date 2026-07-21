use crate::models::{AppConfig, Clip, ClipKind};
use sha2::{Digest, Sha256};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    OpenClipboard, CloseClipboard, GetClipboardData, EmptyClipboard, SetClipboardData,
};

const CF_TEXT: u32 = 1;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;

// Raw kernel32 memory functions — HANDLE.0 is isize
extern "system" {
    fn GlobalSize(hMem: isize) -> usize;
    fn GlobalLock(hMem: isize) -> *mut std::ffi::c_void;
    fn GlobalUnlock(hMem: isize) -> i32;
    fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> isize;
}

// Helper: GetClipboardData returns HANDLE, HANDLE.0 is *mut c_void
unsafe fn global_size(h: HANDLE) -> usize { GlobalSize(h.0 as isize) }
unsafe fn global_lock(h: HANDLE) -> *mut std::ffi::c_void { GlobalLock(h.0 as isize) }
unsafe fn global_unlock(h: HANDLE) -> i32 { GlobalUnlock(h.0 as isize) }
unsafe fn global_alloc(flags: u32, bytes: usize) -> isize { GlobalAlloc(flags, bytes) }

pub fn hash_content(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

pub fn capture_clipboard(config: &AppConfig) -> Result<Clip, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let (source_exe, source_title) = get_foreground_info();

    for excluded in &config.exclusion_list {
        if source_exe.to_lowercase() == excluded.to_lowercase() {
            return Err("Source is excluded".to_string());
        }
    }

    if let Ok(clip) = try_capture_image(config, &source_exe, &source_title, now) {
        return Ok(clip);
    }
    if let Ok(clip) = try_capture_file_paths(&source_exe, &source_title, now) {
        return Ok(clip);
    }
    if let Ok(clip) = try_capture_text(config, &source_exe, &source_title, now) {
        return Ok(clip);
    }

    Err("No supported clipboard format".to_string())
}

fn try_capture_image(config: &AppConfig, source_exe: &str, source_title: &str, now: u64) -> Result<Clip, String> {
    unsafe {
        if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
            return Err("Cannot open clipboard".to_string());
        }

        // NOTE: CF_BITMAP is intentionally not used — it returns an HBITMAP
        // (a GDI object handle), not an HGLOBAL memory block, so it cannot be
        // read through GlobalSize/GlobalLock.
        let handle = GetClipboardData(CF_DIB)
            .map_err(|_| "No DIB image on clipboard".to_string())?;

        let mem_size = global_size(handle);
        if mem_size == 0 {
            let _ = CloseClipboard();
            return Err("Empty image data".to_string());
        }
        let ptr = global_lock(handle);
        if ptr.is_null() {
            let _ = CloseClipboard();
            return Err("Cannot lock image data".to_string());
        }
        let dib_data = std::slice::from_raw_parts(ptr as *const u8, mem_size).to_vec();
        global_unlock(handle);
        let _ = CloseClipboard();

        // Enforce the per-image size limit: oversized images are downscaled
        // and re-encoded as 24bpp DIB, so even pinned images stay bounded.
        let limit = (config.image_size_limit_mb as usize) * 1024 * 1024;
        let dib_data = if dib_data.len() > limit {
            match decode_clipboard_image(&dib_data) {
                Ok(img) => downscale_to_limit(&img, limit),
                // Can't process what we can't decode — keep the original bytes.
                Err(_) => dib_data,
            }
        } else {
            dib_data
        };

        let byte_size = dib_data.len() as u64;
        let thumbnail_base64 = generate_thumbnail(&dib_data).unwrap_or_default();
        let content_hash = hash_content(&dib_data);

        Ok(Clip {
            id: Clip::new_id(&content_hash, now),
            kind: ClipKind::Image,
            text_content: None,
            image_data: Some(dib_data),
            thumbnail_base64: if thumbnail_base64.is_empty() { None } else { Some(thumbnail_base64) },
            content_hash,
            preview: String::from("Image"),
            truncated: false,
            source_exe: source_exe.to_string(),
            source_title: source_title.to_string(),
            source_icon: None,
            captured_at: now,
            pinned: false,
            byte_size,
        })
    }
}

fn try_capture_file_paths(source_exe: &str, source_title: &str, now: u64) -> Result<Clip, String> {
    use windows::Win32::UI::Shell::DROPFILES;

    unsafe {
        if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
            return Err("Cannot open clipboard".to_string());
        }

        let handle = GetClipboardData(CF_HDROP).map_err(|_| "No HDROP".to_string())?;
        let ptr = global_lock(handle);
        let dropfiles = &*(ptr as *const DROPFILES);
        let file_offset = dropfiles.pFiles as usize;
        let base = ptr as usize + file_offset;

        let mut files = Vec::new();
        let mut pos = base;
        loop {
            let mut chars = Vec::new();
            let mut pp = pos as *const u16;
            loop {
                let c = *pp;
                if c == 0 { break; }
                chars.push(c);
                pp = pp.add(1);
            }
            if chars.is_empty() { break; }
            files.push(String::from_utf16_lossy(&chars));
            pos += (chars.len() + 1) * 2;
        }

        global_unlock(handle);
        let _ = CloseClipboard();

        let file_list = files.join(";");
        let preview_names: Vec<String> = files.iter().take(3)
            .map(|f| std::path::Path::new(f).file_name()
                .unwrap_or_default().to_string_lossy().to_string())
            .collect();
        let preview = preview_names.join(", ");
        let preview = if files.len() > 3 {
            format!("{}, +{} more", preview, files.len() - 3)
        } else { preview };

        let file_list_len = file_list.len() as u64;

        let content_hash = {
            let mut hasher = Sha256::new();
            hasher.update(file_list.as_bytes());
            hex::encode(hasher.finalize())
        };

        Ok(Clip {
            id: Clip::new_id(&content_hash, now),
            kind: ClipKind::FilePaths,
            text_content: Some(file_list),
            image_data: None,
            thumbnail_base64: None,
            content_hash,
            preview,
            truncated: false,
            source_exe: source_exe.to_string(),
            source_title: source_title.to_string(),
            source_icon: None,
            captured_at: now,
            pinned: false,
            byte_size: file_list_len,
        })
    }
}

fn try_capture_text(config: &AppConfig, source_exe: &str, source_title: &str, now: u64) -> Result<Clip, String> {
    unsafe {
        if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
            return Err("Cannot open clipboard".to_string());
        }

        let handle = GetClipboardData(CF_UNICODETEXT)
            .or_else(|_| GetClipboardData(CF_TEXT))
            .map_err(|_| "No text".to_string())?;

        let ptr = global_lock(handle);
        let mut chars = Vec::new();
        let mut p = ptr as *const u16;
        loop {
            let c = *p;
            if c == 0 { break; }
            chars.push(c);
            p = p.add(1);
        }

        global_unlock(handle);
        let _ = CloseClipboard();

        let text = String::from_utf16_lossy(&chars);
        let original_size = text.len() as u64;
        let limit = config.text_size_limit_kb as usize * 1024;

        let (content, truncated) = if text.len() > limit {
            (text[..limit].to_string(), true)
        } else {
            (text.clone(), false)
        };

        let content_hash = {
            let mut hasher = Sha256::new();
            hasher.update(text.as_bytes());
            hex::encode(hasher.finalize())
        };

        let preview_text: String = content.chars().take(200).collect();
        let preview = if truncated {
            format!("{} [Truncated, original {} KB]", preview_text, original_size / 1024)
        } else {
            preview_text
        };

        Ok(Clip {
            id: Clip::new_id(&content_hash, now),
            kind: ClipKind::Text,
            text_content: Some(content),
            image_data: None,
            thumbnail_base64: None,
            content_hash,
            preview,
            truncated,
            source_exe: source_exe.to_string(),
            source_title: source_title.to_string(),
            source_icon: None,
            captured_at: now,
            pinned: false,
            byte_size: original_size,
        })
    }
}

/// Decode raw CF_DIB bytes into a DynamicImage: manual decoder first
/// (24/32bpp BI_RGB / BI_BITFIELDS), BMP-wrap fallback for exotic layouts.
fn decode_clipboard_image(dib_data: &[u8]) -> Result<image::DynamicImage, String> {
    decode_dib(dib_data)
        .map(image::DynamicImage::ImageRgba8)
        .or_else(|_| {
            // Fallback for palette-based or unusually-headed DIBs: wrap with a
            // correct BMP file header and let the image crate decode it.
            let bmp = wrap_dib_as_bmp(dib_data)
                .ok_or_else(|| "unsupported DIB layout".to_string())?;
            image::load_from_memory(&bmp).map_err(|e| format!("BMP decode: {}", e))
        })
}

/// Re-encode an image as a 24bpp BI_RGB DIB (BITMAPINFOHEADER + bottom-up
/// BGR pixel data, DWORD-aligned rows). Alpha is dropped.
fn encode_dib_24bpp(img: &image::DynamicImage) -> Vec<u8> {
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    let stride = (w * 3 + 3) / 4 * 4;
    let pixel_bytes = stride * h;

    let mut out = Vec::with_capacity(40 + pixel_bytes);
    out.extend_from_slice(&40u32.to_le_bytes());                // biSize
    out.extend_from_slice(&(w as i32).to_le_bytes());           // biWidth
    out.extend_from_slice(&(h as i32).to_le_bytes());           // biHeight (bottom-up)
    out.extend_from_slice(&1u16.to_le_bytes());                 // biPlanes
    out.extend_from_slice(&24u16.to_le_bytes());                // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes());                 // biCompression = BI_RGB
    out.extend_from_slice(&(pixel_bytes as u32).to_le_bytes()); // biSizeImage
    out.extend_from_slice(&2835i32.to_le_bytes());              // biXPelsPerMeter (~72 DPI)
    out.extend_from_slice(&2835i32.to_le_bytes());              // biYPelsPerMeter
    out.extend_from_slice(&0u32.to_le_bytes());                 // biClrUsed
    out.extend_from_slice(&0u32.to_le_bytes());                 // biClrImportant

    let padding = [0u8; 3];
    let pad_len = stride - w * 3;
    let raw = rgb.as_raw();
    for y in (0..h).rev() {
        let row = &raw[y * w * 3..(y + 1) * w * 3];
        for px in row.chunks_exact(3) {
            out.push(px[2]); // B
            out.push(px[1]); // G
            out.push(px[0]); // R
        }
        out.extend_from_slice(&padding[..pad_len]);
    }
    out
}

/// Downscale until the 24bpp DIB encoding fits within `limit` bytes.
fn downscale_to_limit(img: &image::DynamicImage, limit: usize) -> Vec<u8> {
    let first = encode_dib_24bpp(img);
    if first.len() <= limit {
        return first;
    }
    // Estimate a starting scale from the byte ratio (bytes ~ pixels), with margin.
    let mut scale = ((limit as f64 / first.len() as f64).sqrt() * 0.9).max(0.05);
    let mut cur = img.clone();
    for _ in 0..10 {
        let nw = ((img.width() as f64 * scale) as u32).max(1);
        let nh = ((img.height() as f64 * scale) as u32).max(1);
        cur = img.resize(nw, nh, image::imageops::FilterType::Lanczos3);
        let dib = encode_dib_24bpp(&cur);
        if dib.len() <= limit {
            return dib;
        }
        scale *= 0.85;
    }
    encode_dib_24bpp(&cur)
}

fn generate_thumbnail(dib_data: &[u8]) -> Result<String, String> {
    use base64::Engine;
    use image::GenericImageView;
    use image::ImageEncoder;

    let dyn_img = decode_clipboard_image(dib_data)?;

    let (w, h) = dyn_img.dimensions();
    if w == 0 || h == 0 {
        return Err("empty image".to_string());
    }
    let thumb_w = 200u32;
    let thumb_h = (((h as f64) * (thumb_w as f64 / w as f64)) as u32).max(1);
    let thumb = dyn_img.thumbnail(thumb_w, thumb_h).to_rgb8();

    let mut buf = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85)
        .write_image(
            thumb.as_raw(),
            thumb.width(),
            thumb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("JPEG encode: {}", e))?;

    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(buf)
    ))
}

/// Decode a raw DIB (as stored on the clipboard for CF_DIB) into RGBA pixels.
/// Handles the common cases: BITMAPINFOHEADER-or-later, 24/32 bpp, BI_RGB or
/// BI_BITFIELDS. Alpha is honored only when a mask explicitly defines it —
/// 32-bit BI_RGB sources often leave the alpha byte zeroed.
fn decode_dib(dib: &[u8]) -> Result<image::RgbaImage, String> {
    if dib.len() < 40 {
        return Err("DIB too small".to_string());
    }
    let header_size = u32::from_le_bytes(dib[0..4].try_into().unwrap()) as usize;
    if header_size < 40 || dib.len() < header_size {
        return Err("unsupported DIB header".to_string());
    }
    let width = i32::from_le_bytes(dib[4..8].try_into().unwrap());
    let height_raw = i32::from_le_bytes(dib[8..12].try_into().unwrap());
    let bpp = u16::from_le_bytes(dib[14..16].try_into().unwrap()) as usize;
    let compression = u32::from_le_bytes(dib[16..20].try_into().unwrap());

    if width <= 0 || height_raw == 0 {
        return Err("bad dimensions".to_string());
    }
    let width = width as usize;
    let height = height_raw.unsigned_abs() as usize;
    let top_down = height_raw < 0;
    if bpp != 24 && bpp != 32 {
        return Err(format!("unsupported bpp {}", bpp));
    }

    // Channel masks and the offset where pixel data begins.
    let (r_mask, g_mask, b_mask, a_mask, pixel_start) = match compression {
        0 => (0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF, 0u32, header_size), // BI_RGB
        3 => {
            // BI_BITFIELDS: masks live at offset 40 — inside the header for
            // V4+ (header_size >= 108), right after it for a 40-byte header.
            if dib.len() < 52 {
                return Err("missing bitfield masks".to_string());
            }
            let r = u32::from_le_bytes(dib[40..44].try_into().unwrap());
            let g = u32::from_le_bytes(dib[44..48].try_into().unwrap());
            let b = u32::from_le_bytes(dib[48..52].try_into().unwrap());
            if header_size == 40 {
                (r, g, b, 0u32, 52)
            } else if header_size >= 108 {
                let a = u32::from_le_bytes(dib[52..56].try_into().unwrap());
                (r, g, b, a, header_size)
            } else {
                return Err("unsupported DIB header size".to_string());
            }
        }
        c => return Err(format!("unsupported compression {}", c)),
    };

    let bytes_per_px = bpp / 8;
    let stride = (width * bpp + 31) / 32 * 4; // rows are DWORD-aligned
    if dib.len() < pixel_start + stride * height {
        return Err("truncated pixel data".to_string());
    }

    let channel = |px: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 255;
        }
        let shift = mask.trailing_zeros();
        let max = mask >> shift;
        (((px & mask) >> shift) * 255 / max) as u8
    };

    let mut buf = vec![0u8; width * height * 4];
    for y in 0..height {
        let src_row = if top_down { y } else { height - 1 - y };
        let row_off = pixel_start + src_row * stride;
        for x in 0..width {
            let off = row_off + x * bytes_per_px;
            let px = if bytes_per_px == 4 {
                u32::from_le_bytes(dib[off..off + 4].try_into().unwrap())
            } else {
                (dib[off] as u32) | ((dib[off + 1] as u32) << 8) | ((dib[off + 2] as u32) << 16)
            };
            let dst = (y * width + x) * 4;
            buf[dst] = channel(px, r_mask);
            buf[dst + 1] = channel(px, g_mask);
            buf[dst + 2] = channel(px, b_mask);
            buf[dst + 3] = channel(px, a_mask);
        }
    }

    image::RgbaImage::from_raw(width as u32, height as u32, buf)
        .ok_or_else(|| "failed to build image".to_string())
}

/// Wrap a DIB in a proper 14-byte BMP file header so generic decoders can
/// read it. Computes the real pixel-data offset (header + masks + palette).
fn wrap_dib_as_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < 12 {
        return None;
    }
    let header_size = u32::from_le_bytes(dib[0..4].try_into().ok()?) as usize;
    if header_size < 12 || dib.len() < header_size {
        return None;
    }

    let mut extra = 0usize; // bytes between header end and pixel data
    if header_size == 12 {
        // BITMAPCOREHEADER: 3-byte palette entries for <= 8 bpp
        let bpp = u16::from_le_bytes(dib[10..12].try_into().ok()?) as usize;
        if bpp <= 8 {
            extra = (1usize << bpp) * 3;
        }
    } else {
        if dib.len() < 40 {
            return None;
        }
        let bpp = u16::from_le_bytes(dib[14..16].try_into().ok()?) as usize;
        let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
        let clr_used = u32::from_le_bytes(dib[32..36].try_into().ok()?) as usize;
        if header_size == 40 {
            if compression == 3 {
                extra += 12; // BI_BITFIELDS masks follow the header
            } else if compression == 6 {
                extra += 16; // BI_ALPHABITFIELDS
            }
        }
        if bpp <= 8 {
            let colors = if clr_used > 0 { clr_used } else { 1usize << bpp };
            extra += colors * 4;
        }
    }

    if dib.len() < header_size + extra {
        return None;
    }
    let pixel_offset = 14 + header_size + extra;

    let mut bmp = Vec::with_capacity(14 + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
    bmp.extend_from_slice(&[0u8; 4]); // reserved
    bmp.extend_from_slice(&(pixel_offset as u32).to_le_bytes());
    bmp.extend_from_slice(dib);
    Some(bmp)
}

pub fn get_foreground_info() -> (String, String) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return (String::from("Unknown"), String::new());
    }
    unsafe {
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..len as usize]);

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let exe = if pid == 0 {
            String::from("Unknown")
        } else {
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(process) => {
                    let mut path_buf = [0u16; 512];
                    let mut size = path_buf.len() as u32;
                    let name = if QueryFullProcessImageNameW(
                        process,
                        PROCESS_NAME_WIN32,
                        windows::core::PWSTR::from_raw(path_buf.as_mut_ptr()),
                        &mut size,
                    )
                    .is_ok()
                    {
                        let full = String::from_utf16_lossy(&path_buf[..size as usize]);
                        std::path::Path::new(&full)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| String::from("Unknown"))
                    } else {
                        String::from("Unknown")
                    };
                    let _ = CloseHandle(process);
                    name
                }
                Err(_) => String::from("Unknown"),
            }
        };
        (exe, title)
    }
}

pub fn write_text_to_clipboard(text: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    unsafe {
        let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect();
        let bytes = wide.len() * 2;
        let hmem = global_alloc(0x0002, bytes);
        if hmem == 0 { return Err("Alloc failed".to_string()); }
        let ptr = GlobalLock(hmem);
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
        GlobalUnlock(hmem);

        if OpenClipboard(HWND(std::ptr::null_mut())).is_err() { return Err("Cannot open".to_string()); }
        let _ = EmptyClipboard();
        if SetClipboardData(CF_UNICODETEXT, HANDLE(hmem as *mut std::ffi::c_void)).is_err() {
            let _ = CloseClipboard();
            return Err("SetClipboardData failed".to_string());
        }
        let _ = CloseClipboard();
        Ok(())
    }
}

pub fn write_image_to_clipboard(data: &[u8]) -> Result<(), String> {
    unsafe {
        let hmem = GlobalAlloc(0x0002, data.len());
        if hmem == 0 { return Err("Alloc failed".to_string()); }
        let ptr = GlobalLock(hmem);
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
        GlobalUnlock(hmem);

        if OpenClipboard(HWND(std::ptr::null_mut())).is_err() { return Err("Cannot open".to_string()); }
        let _ = EmptyClipboard();
        if SetClipboardData(CF_DIB, HANDLE(hmem as *mut std::ffi::c_void)).is_err() {
            let _ = CloseClipboard();
            return Err("SetClipboardData failed".to_string());
        }
        let _ = CloseClipboard();
        Ok(())
    }
}

pub fn write_file_paths_to_clipboard(paths_str: &str) -> Result<(), String> {
    let paths: Vec<&str> = paths_str.split(';').collect();
    write_text_to_clipboard(&paths.join("\n"))
}

pub fn simulate_ctrl_v() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYBD_EVENT_FLAGS, VK_CONTROL,
    };

    unsafe {
        keybd_event(VK_CONTROL.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(0x56, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(0x56, 0, KEYBD_EVENT_FLAGS(2), 0);
        keybd_event(VK_CONTROL.0 as u8, 0, KEYBD_EVENT_FLAGS(2), 0);
    }
}
