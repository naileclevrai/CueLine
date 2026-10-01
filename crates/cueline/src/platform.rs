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
