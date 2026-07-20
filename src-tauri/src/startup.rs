//! Manage the portable autostart shortcut (`ClipFlow.lnk`) in the current
//! user's `shell:startup` folder. No registry writes — per CONTEXT spec the
//! shortcut points at the exe with the `--hidden` flag.

use std::path::PathBuf;

use windows::core::{Interface, HSTRING};
use windows::Win32::Foundation::S_OK;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile,
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Startup, IShellLinkW, ShellLink, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};

/// Resolve the per-user startup folder (what `shell:startup` expands to).
fn startup_dir() -> Result<PathBuf, String> {
    let pwstr = unsafe { SHGetKnownFolderPath(&FOLDERID_Startup, KF_FLAG_DEFAULT, None) }
        .map_err(|e| format!("Failed to locate shell:startup folder: {}", e))?;
    let path = unsafe { pwstr.to_string() }
        .map_err(|e| format!("Invalid shell:startup path: {}", e))?;
    unsafe { CoTaskMemFree(Some(pwstr.as_ptr() as *const core::ffi::c_void)) };
    Ok(PathBuf::from(path))
}

fn shortcut_path() -> Result<PathBuf, String> {
    Ok(startup_dir()?.join("ClipFlow.lnk"))
}

/// Create or remove the autostart shortcut.
pub fn set_startup(enabled: bool) -> Result<(), String> {
    let lnk = shortcut_path()?;

    if !enabled {
        if lnk.exists() {
            std::fs::remove_file(&lnk)
                .map_err(|e| format!("Failed to remove {}: {}", lnk.display(), e))?;
        }
        return Ok(());
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            return Err(format!("CoInitializeEx failed: {:?}", hr));
        }
        // Balance only the initialization we actually performed.
        let should_uninit = hr == S_OK;

        let result = (|| -> Result<(), String> {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Failed to create ShellLink object: {}", e))?;
            link.SetPath(&HSTRING::from(exe.to_string_lossy().as_ref()))
                .map_err(|e| format!("IShellLinkW::SetPath failed: {}", e))?;
            link.SetArguments(&HSTRING::from("--hidden"))
                .map_err(|e| format!("IShellLinkW::SetArguments failed: {}", e))?;
            link.SetWorkingDirectory(&HSTRING::from(exe_dir.to_string_lossy().as_ref()))
                .map_err(|e| format!("IShellLinkW::SetWorkingDirectory failed: {}", e))?;
            let file: IPersistFile = link
                .cast()
                .map_err(|e| format!("ShellLink does not expose IPersistFile: {}", e))?;
            file.Save(&HSTRING::from(lnk.to_string_lossy().as_ref()), true)
                .map_err(|e| format!("Failed to save {}: {}", lnk.display(), e))?;
            Ok(())
        })();

        if should_uninit {
            CoUninitialize();
        }
        result
    }
}
