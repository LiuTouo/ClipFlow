use crate::models::{AppConfig, Clip, ClipKind};
use sha2::{Digest, Sha256};

// Clipboard format constants (raw values)
const CF_TEXT: u32 = 1;
const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;

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

    // Priority: Image > Text > FilePaths
    if let Ok(clip) = try_capture_image(&source_exe, &source_title, now) {
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

fn try_capture_image(source_exe: &str, source_title: &str, now: u64) -> Result<Clip, String> {
    use windows::Win32::System::DataExchange::{
        OpenClipboard, CloseClipboard, GetClipboardData,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Memory::{GlobalSize, GlobalLock, GlobalUnlock};

    unsafe {
        if OpenClipboard(HWND(0)).is_err() {
            return Err("Cannot open clipboard".to_string());
        }

        // Try DIB first, then BMP
        let handle = GetClipboardData(CF_DIB)
            .or_else(|_| GetClipboardData(CF_BITMAP))
            .map_err(|_| "No image format".to_string())?;

        let mem_size = GlobalSize(handle);
        let ptr = GlobalLock(handle);
        let dib_data = std::slice::from_raw_parts(ptr as *const u8, mem_size).to_vec();
        let _ = GlobalUnlock(handle);
        let _ = CloseClipboard();

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
            byte_size: mem_size as u64,
        })
    }
}

fn try_capture_file_paths(source_exe: &str, source_title: &str, now: u64) -> Result<Clip, String> {
    use windows::Win32::System::DataExchange::{OpenClipboard, CloseClipboard, GetClipboardData};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::UI::Shell::DROPFILES;

    unsafe {
        if OpenClipboard(HWND(0)).is_err() {
            return Err("Cannot open clipboard".to_string());
        }

        let handle = GetClipboardData(CF_HDROP).map_err(|_| "No HDROP".to_string())?;
        let ptr = GlobalLock(handle);
        let dropfiles = &*(ptr as *const DROPFILES);
        let file_offset = dropfiles.pFiles as usize;
        let base = ptr as usize + file_offset;

        let mut files = Vec::new();
        let mut pos = base;
        loop {
            let pstr = pos as *const u16;
            let mut chars = Vec::new();
            let mut pp = pstr;
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

        let _ = GlobalUnlock(handle);
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
            byte_size: file_list.len() as u64,
        })
    }
}

fn try_capture_text(config: &AppConfig, source_exe: &str, source_title: &str, now: u64) -> Result<Clip, String> {
    use windows::Win32::System::DataExchange::{OpenClipboard, CloseClipboard, GetClipboardData};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    unsafe {
        if OpenClipboard(HWND(0)).is_err() {
            return Err("Cannot open clipboard".to_string());
        }

        let handle = GetClipboardData(CF_UNICODETEXT)
            .or_else(|_| GetClipboardData(CF_TEXT))
            .map_err(|_| "No text".to_string())?;

        let ptr = GlobalLock(handle);
        let mut chars = Vec::new();
        let mut p = ptr as *const u16;
        loop {
            let c = *p;
            if c == 0 { break; }
            chars.push(c);
            p = p.add(1);
        }

        let _ = GlobalUnlock(handle);
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

fn generate_thumbnail(dib_data: &[u8]) -> Result<String, String> {
    use image::GenericImageView;
    use base64::Engine;

    let img = image::load_from_memory(dib_data)
        .or_else(|_| {
            let mut bmp = Vec::with_capacity(14 + dib_data.len());
            bmp.extend_from_slice(b"BM");
            bmp.extend_from_slice(&(dib_data.len() as u32 + 14).to_le_bytes());
            bmp.extend_from_slice(&[0u8; 4]);
            bmp.extend_from_slice(dib_data);
            image::load_from_memory(&bmp)
        })
        .map_err(|e| format!("Image load: {}", e))?;

    let (w, h) = img.dimensions();
    let thumb_w = 200u32;
    let thumb_h = ((h as f64) * (thumb_w as f64 / w as f64)) as u32;
    let thumb_h = thumb_h.max(1);
    let thumb = img.thumbnail(thumb_w, thumb_h);

    let mut buf = std::io::Cursor::new(Vec::new());
    thumb.write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("JPEG encode: {}", e))?;

    Ok(format!("data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(buf.into_inner())))
}

pub fn get_foreground_info() -> (String, String) {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
        };

        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return (String::from("Unknown"), String::new());
        }
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..len as usize]);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let exe = get_process_name(pid).unwrap_or_else(|| "Unknown".to_string());
        (exe, title)
    }
}

fn get_process_name(_pid: u32) -> Option<String> {
    None
}

pub fn write_text_to_clipboard(text: &str) -> Result<(), String> {
    use windows::Win32::System::DataExchange::{OpenClipboard, CloseClipboard, EmptyClipboard, SetClipboardData};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GLOBAL_ALLOC_FLAGS};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    unsafe {
        let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect();
        let bytes = wide.len() * 2;
        let handle = GlobalAlloc(GLOBAL_ALLOC_FLAGS(0x0002), bytes)
            .map_err(|_| "Alloc failed".to_string())?;
        let ptr = GlobalLock(handle);
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
        let _ = GlobalUnlock(handle);

        if OpenClipboard(HWND(0)).is_err() { return Err("Cannot open".to_string()); }
        let _ = EmptyClipboard();
        if SetClipboardData(CF_UNICODETEXT, handle).is_err() {
            let _ = CloseClipboard();
            return Err("SetClipboardData failed".to_string());
        }
        let _ = CloseClipboard();
        Ok(())
    }
}

pub fn write_image_to_clipboard(data: &[u8]) -> Result<(), String> {
    use windows::Win32::System::DataExchange::{OpenClipboard, CloseClipboard, EmptyClipboard, SetClipboardData};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GLOBAL_ALLOC_FLAGS};

    unsafe {
        let handle = GlobalAlloc(GLOBAL_ALLOC_FLAGS(0x0002), data.len())
            .map_err(|_| "Alloc failed".to_string())?;
        let ptr = GlobalLock(handle);
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
        let _ = GlobalUnlock(handle);

        if OpenClipboard(HWND(0)).is_err() { return Err("Cannot open".to_string()); }
        let _ = EmptyClipboard();
        if SetClipboardData(CF_DIB, handle).is_err() {
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
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::keybd_event;
        // Ctrl key down
        keybd_event(17, 0, 0, 0);
        // V key down
        keybd_event(0x56, 0, 0, 0);
        // V key up
        keybd_event(0x56, 0, 2, 0);
        // Ctrl key up
        keybd_event(17, 0, 2, 0);
    }
}
