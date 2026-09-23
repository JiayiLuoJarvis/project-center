//! Windows 剪贴板写入；非 Windows 返回错误。

/// 将文本写入系统剪贴板（Unicode）。
pub fn set_text(text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        set_text_windows(text)
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        Err("剪贴板仅支持 Windows".into())
    }
}

#[cfg(windows)]
fn set_text_windows(text: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows_sys::Win32::System::DataExchange::{
        CF_UNICODETEXT, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };

    let mut wide: Vec<u16> = OsStr::new(text).encode_wide().collect();
    wide.push(0);
    let bytes = wide.len() * std::mem::size_of::<u16>();

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("无法打开剪贴板".into());
        }
        let _guard = ClipboardGuard;

        if EmptyClipboard() == 0 {
            return Err("无法清空剪贴板".into());
        }

        let handle: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if handle.is_null() {
            return Err("无法分配剪贴板内存".into());
        }
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            return Err("无法锁定剪贴板内存".into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr.cast::<u16>(), wide.len());
        GlobalUnlock(handle);

        let data: HANDLE = SetClipboardData(CF_UNICODETEXT, handle);
        if data.is_null() {
            return Err("无法写入剪贴板".into());
        }
    }
    Ok(())
}

#[cfg(windows)]
struct ClipboardGuard;

#[cfg(windows)]
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::DataExchange::CloseClipboard();
        }
    }
}
