use crate::error::AppError;
use crate::handlers;
use tauri::AppHandle;

/// Enable or disable the clip-extension ingest listener.
///
/// When enabled, a local owner-only listener accepts selected text pushed by a
/// third-party clip tool and re-emits it as the `ClipExtension` pseudo-shortcut.
/// Off by default; disabling drops the listener.
#[tauri::command]
pub fn set_clip_extension_enabled(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    if enabled {
        handlers::clip::start(app);
    } else {
        handlers::clip::stop();
    }
    Ok(())
}
