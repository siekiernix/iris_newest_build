//! Host services the egui shell uses to carry out effects.

/// Reveal `path` in Finder (select it). Uses `NSWorkspace` rather than spawning
/// `open`, so it works under the App Sandbox (which forbids launching helper
/// processes). Best-effort; a missing path just does nothing. Available on all
/// macOS builds (objc2 is a macOS-wide dependency, not gated on `appstore`).
#[cfg(target_os = "macos")]
pub fn reveal_in_finder(path: &str) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;

    let ns_path = NSString::from_str(path);
    let root = NSString::from_str(""); // empty root → Finder picks the parent
    // SAFETY: `+[NSWorkspace sharedWorkspace]` returns the shared singleton;
    // `-selectFile:inFileViewerRootedAtPath:` takes two NSString* and returns
    // BOOL. AppKit is loaded (this is a GUI app), and both strings are valid.
    unsafe {
        let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let _: bool = msg_send![ws, selectFile: &*ns_path, inFileViewerRootedAtPath: &*root];
    }
}
