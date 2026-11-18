use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;

use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW,
    SHFileOperationW,
};

pub(crate) fn trash(abs: &Path) -> io::Result<()> {
    let text = abs.as_os_str().to_string_lossy();
    let plain = match text.strip_prefix(r"\\?\UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => text.strip_prefix(r"\\?\").unwrap_or(&text).to_string(),
    };
    let from: Vec<u16> = std::ffi::OsStr::new(&plain)
        .encode_wide()
        .chain([0, 0])
        .collect();
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16,
        ..Default::default()
    };
    // SAFETY: `from` is a double NUL terminated list that outlives the call; no window,
    // no name mapping is asked.
    let result = unsafe { SHFileOperationW(&mut operation) };
    if result != 0 {
        return Err(io::Error::other(format!(
            "the Recycle Bin refused {plain}: SHFileOperationW returned {result:#x}"
        )));
    }
    if operation.fAnyOperationsAborted != 0 {
        return Err(io::Error::other(format!(
            "moving {plain} to the Recycle Bin was cancelled"
        )));
    }
    Ok(())
}
