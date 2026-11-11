//! Operating system integration that is not UI: one running copy, starting
//! at sign-in, and a few words that differ per system.

/// What the system calls signing in, and where Quiet waits afterwards.
#[cfg(windows)]
pub const AUTOSTART_TITLE: &str = "Start with Windows";
#[cfg(target_os = "macos")]
pub const AUTOSTART_TITLE: &str = "Open at login";
#[cfg(target_os = "linux")]
pub const AUTOSTART_TITLE: &str = "Start when you log in";

#[cfg(target_os = "macos")]
pub const AUTOSTART_CAPTION: &str =
    "Quiet waits in the menu bar after you log in. Closing the window keeps it there.";
#[cfg(not(target_os = "macos"))]
pub const AUTOSTART_CAPTION: &str =
    "Quiet waits in the tray after you sign in. Closing the window keeps it there.";

/// The macOS bundle identifier, also used for notifications there.
#[cfg(target_os = "macos")]
pub const BUNDLE_ID: &str = "io.github.stanleykinkelaar.quiet";

/// Asks the running copy for something. Also what `--capture` sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Show,
    Capture,
}

/// Starts Quiet straight into quick capture, or asks the running copy to
/// open it. Bind a key to `quiet --capture` where global shortcuts belong to
/// the desktop (Wayland, so Hyprland, GNOME, KDE).
pub const CAPTURE_ARG: &str = "--capture";

/// Only one copy of the app runs. Starting it again hands the request to
/// the running copy, then exits.
pub mod single_instance {
    use super::Request;

    pub enum Instance {
        /// We are the running copy.
        First(Listener),
        /// Another copy is running and has been handed the request.
        Second,
    }

    /// Waits on a background thread; `on_request` must hand off to the UI.
    pub struct Listener {
        #[cfg(windows)]
        events: [(usize, Request); 2],
        #[cfg(windows)]
        _mutex: usize,
        #[cfg(unix)]
        socket: Option<std::os::unix::net::UnixListener>,
    }

    #[cfg(windows)]
    mod imp {
        use super::{Instance, Listener, Request};
        use windows_sys::Win32::{
            Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0},
            System::Threading::{
                CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent,
                WaitForSingleObject,
            },
        };

        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(Some(0)).collect()
        }

        fn event_name(scope: &str, request: Request) -> Vec<u16> {
            let kind = match request {
                Request::Show => "Show",
                Request::Capture => "Capture",
            };
            wide(&format!("Local\\Quiet.{kind}.{scope}"))
        }

        pub fn acquire(scope: &str, request: Request) -> Instance {
            let mutex_name = wide(&format!("Local\\Quiet.Instance.{scope}"));
            unsafe {
                let mutex: HANDLE = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let name = event_name(scope, request);
                    let event = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
                    if !event.is_null() {
                        SetEvent(event);
                        CloseHandle(event);
                    }
                    return Instance::Second;
                }
                // A fixed name the installer can check to ask you to quit first.
                let installer_mutex = wide("QuietAppRunning");
                CreateMutexW(std::ptr::null(), 0, installer_mutex.as_ptr());
                let create = |request| {
                    let name = event_name(scope, request);
                    (CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) as usize, request)
                };
                Instance::First(Listener {
                    events: [create(Request::Show), create(Request::Capture)],
                    _mutex: mutex as usize,
                })
            }
        }

        impl Listener {
            pub fn listen(self, on_request: impl Fn(Request) + Send + Sync + 'static) {
                let on_request = std::sync::Arc::new(on_request);
                for (event, request) in self.events {
                    let on_request = on_request.clone();
                    std::thread::spawn(move || {
                        let event = event as HANDLE;
                        while unsafe { WaitForSingleObject(event, INFINITE) } == WAIT_OBJECT_0 {
                            on_request(request);
                        }
                    });
                }
            }
        }
    }

    /// A Unix socket in the user's runtime directory. Connecting to it
    /// means a copy is running; a socket nobody answers on is left over
    /// from a crash and gets replaced.
    #[cfg(unix)]
    mod imp {
        use super::{Instance, Listener, Request};
        use std::io::{Read as _, Write as _};
        use std::os::unix::net::{UnixListener, UnixStream};
        use std::path::PathBuf;

        fn socket_path(scope: &str) -> PathBuf {
            let dir = std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .filter(|dir| dir.is_dir())
                .unwrap_or_else(std::env::temp_dir);
            dir.join(format!("quiet-{scope}.sock"))
        }

        pub fn acquire(scope: &str, request: Request) -> Instance {
            let path = socket_path(scope);
            if let Ok(mut stream) = UnixStream::connect(&path) {
                let message: &[u8] = match request {
                    Request::Show => b"show",
                    Request::Capture => b"capture",
                };
                stream.write_all(message).ok();
                return Instance::Second;
            }
            std::fs::remove_file(&path).ok();
            match UnixListener::bind(&path) {
                Ok(socket) => Instance::First(Listener { socket: Some(socket) }),
                Err(err) => {
                    eprintln!("quiet: single instance socket {}: {err}", path.display());
                    Instance::First(Listener { socket: None })
                }
            }
        }

        impl Listener {
            pub fn listen(self, on_request: impl Fn(Request) + Send + Sync + 'static) {
                let Some(socket) = self.socket else { return };
                std::thread::spawn(move || {
                    for mut stream in socket.incoming().flatten() {
                        let mut message = String::new();
                        stream.read_to_string(&mut message).ok();
                        on_request(match message.trim() {
                            "capture" => Request::Capture,
                            _ => Request::Show,
                        });
                    }
                });
            }
        }
    }

    pub use imp::acquire;
}

/// Start at sign-in: the Run key on Windows, a LaunchAgent on macOS, an
/// XDG autostart entry on Linux. No admin rights anywhere.
pub mod autostart {
    /// Started at sign-in: stay in the tray, no window.
    pub const BACKGROUND_ARG: &str = "--background";

    fn launcher() -> anyhow::Result<auto_launch::AutoLaunch> {
        let exe = std::env::current_exe()?;
        let mut builder = auto_launch::AutoLaunchBuilder::new();
        builder.set_app_name("Quiet").set_app_path(&exe.to_string_lossy()).set_args(&[BACKGROUND_ARG]);
        #[cfg(windows)]
        builder.set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);
        #[cfg(target_os = "macos")]
        builder
            .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent)
            .set_bundle_identifiers(&[super::BUNDLE_ID]);
        #[cfg(target_os = "linux")]
        builder.set_linux_launch_mode(auto_launch::LinuxLaunchMode::XdgAutostart);
        Ok(builder.build()?)
    }

    pub fn is_enabled() -> bool {
        launcher().and_then(|l| Ok(l.is_enabled()?)).unwrap_or(false)
    }

    pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
        let launcher = launcher()?;
        if enabled { launcher.enable()? } else { launcher.disable()? }
        Ok(())
    }
}

/// Whether Windows will show toast notifications at all. When the user
/// has switched them off, reminders cannot ring, and Settings says so.
/// Other systems give no reliable answer, so they report true.
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
