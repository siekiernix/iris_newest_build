//! Stable widget ids. Every shell sets them as accessibility identifiers
//! (AppKit `setAccessibilityIdentifier`, egui `Id::new`), so one scenario
//! test can drive every view. Never rename one without updating the tests that
//! find controls by it.

/// The main window's toolbar.
pub mod toolbar {
    pub const SHOW_LIBRARY: &str = "toolbar.show-library";
    pub const POWER_ON: &str = "toolbar.power-on";
    pub const SHUT_DOWN_GUEST: &str = "toolbar.shut-down-guest";
    pub const POWER_OFF: &str = "toolbar.power-off";
    pub const RESTART: &str = "toolbar.restart";
    pub const SAVE_STATE: &str = "toolbar.save-state";
    pub const SAVED_STATES: &str = "toolbar.saved-states";
    pub const SCREENSHOT: &str = "toolbar.screenshot";
    pub const INSERT_DISC: &str = "toolbar.insert-disc";
    pub const BROWSE_FILES: &str = "toolbar.browse-files";
    pub const DETACH_DISPLAY: &str = "toolbar.detach-display";
    pub const FULL_SCREEN: &str = "toolbar.full-screen";
    pub const SERIAL_CONSOLE: &str = "toolbar.serial-console";
    pub const MACHINE_SETTINGS: &str = "toolbar.machine-settings";
}

/// The machine library on the left.
pub mod library {
    pub const LIST: &str = "library.list";
    pub const NEW_MACHINE: &str = "library.new-machine";
    pub const NEW_FOLDER: &str = "library.new-folder";
}

/// The status bar under the content.
pub mod status {
    pub const RUN_STATE: &str = "status.run-state";
    pub const EXECUTOR: &str = "status.executor";
    pub const TOAST: &str = "status.toast";
    pub const NETWORK: &str = "status.network";
    pub const SERIAL: &str = "status.serial";
    pub const AUDIO: &str = "status.audio";
    pub const CAMERA: &str = "status.camera";
    pub const CAPTURE: &str = "status.capture";
    pub const SCALE: &str = "status.scale";
    pub const MIPS: &str = "status.mips";
    pub const HZ: &str = "status.hz";

    /// A disk light: `status.disk.<controller>.<id>`.
    pub fn disk(controller: usize, id: u8) -> String {
        format!("status.disk.{controller}.{id}")
    }
}
