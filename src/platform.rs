//! Windows integration that is not UI: one running copy, and starting
//! with Windows.

/// Only one copy of the app runs. Starting it again asks the running copy
/// to show its window, then exits.
pub mod single_instance {
    #[cfg(windows)]
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0},
        System::Threading::{
            CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent,
            WaitForSingleObject,
        },
    };

    pub enum Instance {
        /// We are the running copy. `on_show` runs when another copy starts.
        First(Listener),
        /// Another copy is running and has been asked to show itself.
        Second,
    }

    pub struct Listener {
        #[cfg(windows)]
        event: usize,
        #[cfg(windows)]
        _mutex: usize,
    }

    #[cfg(windows)]
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// `scope` keeps separate databases (for example a test copy) apart.
    #[cfg(windows)]
    pub fn acquire(scope: &str) -> Instance {
        let mutex_name = wide(&format!("Local\\Quiet.Instance.{scope}"));
        let event_name = wide(&format!("Local\\Quiet.Show.{scope}"));
        unsafe {
            let mutex: HANDLE = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let event = OpenEventW(EVENT_MODIFY_STATE, 0, event_name.as_ptr());
                if !event.is_null() {
                    SetEvent(event);
                    CloseHandle(event);
                }
                return Instance::Second;
            }
            // A fixed name the installer can check to ask you to quit first.
            let installer_mutex = wide("QuietAppRunning");
            CreateMutexW(std::ptr::null(), 0, installer_mutex.as_ptr());
            let event = CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr());
            Instance::First(Listener { event: event as usize, _mutex: mutex as usize })
        }
    }

    #[cfg(not(windows))]
    pub fn acquire(_scope: &str) -> Instance {
        Instance::First(Listener {})
    }

    impl Listener {
        /// Waits on a background thread; `on_show` must hand off to the UI.
        pub fn listen(self, on_show: impl Fn() + Send + 'static) {
            #[cfg(windows)]
            std::thread::spawn(move || {
                let event = self.event as HANDLE;
                while unsafe { WaitForSingleObject(event, INFINITE) } == WAIT_OBJECT_0 {
                    on_show();
                }
            });
            #[cfg(not(windows))]
            let _ = on_show;
        }
    }
}

/// Start with Windows, through the per-user Run key. No admin rights.
pub mod autostart {
    /// Started by Windows at sign-in: stay in the tray, no window.
    pub const BACKGROUND_ARG: &str = "--background";

    #[cfg(windows)]
    fn launcher() -> anyhow::Result<auto_launch::AutoLaunch> {
        let exe = std::env::current_exe()?;
        Ok(auto_launch::AutoLaunchBuilder::new()
            .set_app_name("Quiet")
            .set_app_path(&exe.to_string_lossy())
            .set_args(&[BACKGROUND_ARG])
            .set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser)
            .build()?)
    }

    #[cfg(windows)]
    pub fn is_enabled() -> bool {
        launcher().and_then(|l| Ok(l.is_enabled()?)).unwrap_or(false)
    }

    #[cfg(windows)]
    pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
        let launcher = launcher()?;
        if enabled { launcher.enable()? } else { launcher.disable()? }
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn is_enabled() -> bool {
        false
    }

    #[cfg(not(windows))]
    pub fn set_enabled(_: bool) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Whether Windows will show toast notifications at all. When the user
/// has switched them off, reminders cannot ring, and Settings says so.
pub fn notifications_enabled() -> bool {
    #[cfg(windows)]
    {
        windows_registry::CURRENT_USER
            .open(r"Software\Microsoft\Windows\CurrentVersion\PushNotifications")
            .and_then(|key| key.get_u32("ToastEnabled"))
            .map(|value| value != 0)
            .unwrap_or(true)
    }
    #[cfg(not(windows))]
    true
}
