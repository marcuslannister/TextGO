//! Clip-extension ingest listener.
//!
//! A local, owner-only IPC endpoint that lets a third-party clip tool
//! (SnipDo on Windows) push selected text into TextGO. Received text is
//! re-emitted as the `ClipExtension` pseudo-shortcut through the existing
//! `shortcut` event pipeline, so no downstream code changes are needed.
//!
//! The listener is opt-in (off by default) and reachable only by same-user
//! local processes: on Windows it is a named pipe protected by an owner-only
//! security descriptor, which a browser `fetch()` cannot open. macOS parity
//! (a `0600` unix socket) is a later phase; on non-Windows platforms `start`
//! is currently a no-op.

use tauri::AppHandle;

/// Windows named-pipe endpoint for clip ingest.
#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\textgo";

/// Shutdown signal for the running accept loop; `Some` while the listener runs.
#[cfg(windows)]
static CLIP_SHUTDOWN: std::sync::LazyLock<
    std::sync::Mutex<Option<std::sync::Arc<tokio::sync::Notify>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

/// Start the clip-extension ingest listener. No-op if already running.
pub fn start(app: AppHandle) {
    #[cfg(windows)]
    start_windows(app);
    #[cfg(not(windows))]
    {
        // macOS unix-socket support lands in a later phase.
        let _ = app;
    }
}

/// Stop the clip-extension ingest listener. No-op if not running.
pub fn stop() {
    #[cfg(windows)]
    {
        if let Ok(mut guard) = CLIP_SHUTDOWN.lock() {
            if let Some(notify) = guard.take() {
                notify.notify_waiters();
            }
        }
    }
}

/// Register a shutdown handle and spawn the accept loop, unless one is running.
#[cfg(windows)]
fn start_windows(app: AppHandle) {
    use std::sync::Arc;
    use tokio::sync::Notify;

    let shutdown = {
        let Ok(mut guard) = CLIP_SHUTDOWN.lock() else {
            return;
        };
        if guard.is_some() {
            // a listener is already running
            return;
        }
        let notify = Arc::new(Notify::new());
        *guard = Some(notify.clone());
        notify
    };

    tauri::async_runtime::spawn(async move {
        if let Err(error) = accept_loop(app, shutdown.clone()).await {
            log::error!("Clip ingest listener stopped: {error}");
        }
        // clear the handle so a later start() can rebind — but only if it still
        // points to this loop. A fast disable→enable may have already installed
        // a new listener; clobbering its handle would leave that one un-stoppable.
        if let Ok(mut guard) = CLIP_SHUTDOWN.lock() {
            if guard
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &shutdown))
            {
                *guard = None;
            }
        }
    });
}

/// Accept connections until signalled to stop, keeping one server instance
/// waiting ahead of each client so a connection is never refused.
#[cfg(windows)]
async fn accept_loop(
    app: AppHandle,
    shutdown: std::sync::Arc<tokio::sync::Notify>,
) -> Result<(), String> {
    use tokio::net::windows::named_pipe::NamedPipeServer;

    let mut server = create_server(true)?;
    loop {
        tokio::select! {
            biased;
            _ = shutdown.notified() => break,
            result = server.connect() => {
                result.map_err(|e| format!("pipe connect failed: {e}"))?;
            }
        }

        // hand off the connected instance and immediately open the next one
        let connected: NamedPipeServer = server;
        server = create_server(false)?;

        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            handle_connection(app_handle, connected).await;
        });
    }
    Ok(())
}

/// Create one named-pipe server instance guarded by an owner-only descriptor.
#[cfg(windows)]
fn create_server(first: bool) -> Result<tokio::net::windows::named_pipe::NamedPipeServer, String> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let sd = OwnerOnlySd::new()?;
    let mut attrs = sd.security_attributes();

    // SAFETY: `attrs` and the descriptor it points at outlive this call; the
    // kernel copies the security info into the new pipe instance.
    let server = unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .create_with_security_attributes_raw(
                PIPE_NAME,
                &mut attrs as *mut _ as *mut core::ffi::c_void,
            )
    }
    .map_err(|e| format!("failed to create pipe instance: {e}"))?;

    Ok(server)
}

/// Read the full request body from a connected client and re-emit it as the
/// `ClipExtension` pseudo-shortcut. Mirrors `handlers/keyboard.rs`, but emits
/// the received text directly instead of calling `get_selection`.
#[cfg(windows)]
async fn handle_connection(
    app: AppHandle,
    mut server: tokio::net::windows::named_pipe::NamedPipeServer,
) {
    use tauri::Emitter;
    use tokio::io::AsyncReadExt;

    let mut buf = Vec::new();
    match server.read_to_end(&mut buf).await {
        Ok(_) => {
            let text = String::from_utf8_lossy(&buf).into_owned();
            if text.is_empty() {
                return;
            }
            // honor the same gates as keyboard/mouse triggers: skip while
            // shortcut handling is suspended/paused, or when the frontmost
            // app/website is blacklisted.
            use std::sync::atomic::Ordering;
            if crate::SHORTCUT_SUSPEND.load(Ordering::Relaxed)
                || crate::SHORTCUT_PAUSED.load(Ordering::Relaxed)
            {
                return;
            }
            if let Ok(true) = crate::commands::is_blocked(app.clone()) {
                return;
            }
            let event_data = serde_json::json!({
                "shortcut": "ClipExtension",
                "selection": text,
            });
            let _ = app.emit("shortcut", event_data);
        }
        Err(error) => {
            log::warn!("Clip ingest read failed: {error}");
        }
    }
    // dropping `server` disconnects the pipe instance
}

/// A protected, owner-only security descriptor built from the SDDL string
/// `D:P(A;;GA;;;OW)` — a protected DACL granting GENERIC_ALL to the object
/// owner (the user running TextGO) and no one else. Frees the descriptor
/// (allocated by the Win32 SDDL parser via `LocalAlloc`) on drop.
#[cfg(windows)]
struct OwnerOnlySd(windows::Win32::Security::PSECURITY_DESCRIPTOR);

#[cfg(windows)]
impl OwnerOnlySd {
    fn new() -> Result<Self, String> {
        use windows::core::w;
        use windows::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows::Win32::Security::PSECURITY_DESCRIPTOR;

        let mut psd = PSECURITY_DESCRIPTOR(core::ptr::null_mut());
        // SAFETY: writes a freshly allocated descriptor into `psd`; freed in Drop.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                w!("D:P(A;;GA;;;OW)"),
                SDDL_REVISION_1,
                &mut psd,
                None,
            )
        }
        .map_err(|e| format!("failed to build security descriptor: {e}"))?;
        Ok(Self(psd))
    }

    fn security_attributes(&self) -> windows::Win32::Security::SECURITY_ATTRIBUTES {
        windows::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: core::mem::size_of::<windows::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0 .0,
            bInheritHandle: false.into(),
        }
    }
}

#[cfg(windows)]
impl Drop for OwnerOnlySd {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{LocalFree, HLOCAL};
        if !self.0 .0.is_null() {
            // SAFETY: the descriptor was allocated by the SDDL parser via LocalAlloc.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0 .0)));
            }
        }
    }
}
