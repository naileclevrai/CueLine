//! OS-level real-time tuning. No-ops on non-Windows targets.

/// Raises the system timer resolution to 1 ms for the lifetime of the guard,
/// so `thread::sleep` in the MTC scheduler wakes up on time.
pub struct TimerResolution(());

impl TimerResolution {
    pub fn acquire() -> Self {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Media::timeBeginPeriod(1);
        }
        Self(())
    }
}

impl Drop for TimerResolution {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Media::timeEndPeriod(1);
        }
    }
}

/// Registers the calling thread with MMCSS as a "Pro Audio" task.
pub fn promote_audio_thread() {
    #[cfg(windows)]
    unsafe {
        let name: Vec<u16> = "Pro Audio\0".encode_utf16().collect();
        let mut index = 0u32;
        windows_sys::Win32::System::Threading::AvSetMmThreadCharacteristicsW(name.as_ptr(), &mut index);
    }
}

/// Gives the calling thread the highest scheduling priority.
pub fn promote_timing_thread() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::*;
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
    }
}

/// Asks Windows 11 to round the corners of this thread's top-level windows
/// (the main window is frameless, so DWM would otherwise square them).
pub fn round_window_corners() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::{HWND, LPARAM};
        use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::WindowsAndMessaging::EnumThreadWindows;

        unsafe extern "system" fn apply(hwnd: HWND, _: LPARAM) -> windows_sys::core::BOOL {
            let pref = DWMWCP_ROUND;
            DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, &pref as *const _ as *const _, 4);
            1
        }
        EnumThreadWindows(GetCurrentThreadId(), Some(apply), 0);
    }
}
