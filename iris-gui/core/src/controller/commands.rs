//! Toolbar and menu commands by widget id: each one's icon and tooltip
//! (D11), whether it can run now and why not, and what it does. Every shell
//! routes its toolbar items and menu items through here, so the meaning of a
//! control is decided once.

use super::app::{AppAction, AppController};
use super::display::{DisplayAction, GuestChord};
use super::icons::Icon;
use super::ids::toolbar as tb;
use super::keys::{Command, KeyMap};
use super::labels::Tip;
use super::serial::SerialAction;
use super::session::SessionAction;
use super::{Controller, Effect, Enabled};

/// Menu item ids (the toolbar's are in `ids::toolbar`).
pub mod menu {
    pub const ABOUT: &str = "menu.about";
    pub const APP_SETTINGS: &str = "menu.app-settings";
    pub const QUIT: &str = "menu.quit";
    pub const NEW_MACHINE: &str = "menu.new-machine";
    pub const NEW_FOLDER: &str = "menu.new-folder";
    pub const DUPLICATE: &str = "menu.duplicate";
    pub const RENAME: &str = "menu.rename";
    pub const DELETE: &str = "menu.delete";
    pub const SHOW_IN_FINDER: &str = "menu.show-in-finder";
    pub const OPEN_CONFIG: &str = "menu.open-config";
    pub const CLOSE: &str = "menu.close";
    pub const CAPTURE: &str = "menu.capture";
    pub const SEND_F11: &str = "menu.send-f11";
    pub const SEND_CAD: &str = "menu.send-ctrl-alt-del";
    pub const SEND_ESC: &str = "menu.send-esc";
    pub const SYNC_DISKS: &str = "menu.sync-disks";
    pub const CHECK_NETWORK: &str = "menu.check-network";
    pub const BENCHMARK: &str = "menu.benchmark";
    pub const CAMERA_TEST: &str = "menu.camera-test";
    pub const DIAGNOSTICS: &str = "menu.diagnostics";
    pub const RESET_NVRAM: &str = "menu.reset-nvram";
    pub const GUIDE: &str = "menu.guide";
    pub const REPORT_ISSUE: &str = "menu.report-issue";
}

/// Commands that change only the shell's own presentation; the shell does them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellCommand {
    ToggleLibrary,
    /// Open the machine settings sheet.
    Settings,
    /// Show the saved-states list.
    SavedStates,
    NewMachine,
    About,
    AppSettings,
    Benchmark,
    Diagnostics,
    CloseWindow,
}

/// What a command id did.
#[derive(Debug, Default)]
pub struct Routed {
    pub effects: Vec<Effect>,
    pub shell: Option<ShellCommand>,
}

/// The icon and tooltip of a toolbar id, with its shortcut.
pub fn tip(id: &str, keys: &KeyMap) -> Option<Tip> {
    let (icon, text, cmd) = match id {
        tb::SHOW_LIBRARY => (Icon::ShowLibrary, t!("tb-show-library"), Some(Command::ShowLibrary)),
        tb::POWER_ON => (Icon::PowerOn, t!("tb-power-on"), Some(Command::PowerOn)),
        tb::SHUT_DOWN_GUEST => (Icon::ShutDownGuest, t!("tb-shut-down"), Some(Command::ShutDownGuest)),
        tb::POWER_OFF => (Icon::PowerOff, t!("tb-power-off"), None),
        tb::RESTART => (Icon::Restart, t!("tb-restart"), None),
        tb::SAVE_STATE => (Icon::SaveState, t!("tb-save-state"), Some(Command::SaveState)),
        tb::SAVED_STATES => (Icon::SavedStates, t!("tb-saved-states"), None),
        tb::SCREENSHOT => (Icon::Screenshot, t!("tb-screenshot"), Some(Command::Screenshot)),
        tb::INSERT_DISC => (Icon::InsertDisc, t!("tb-insert-disc"), None),
        tb::BROWSE_FILES => (Icon::BrowseFiles, t!("tb-browse-files"), Some(Command::BrowseFiles)),
        tb::DETACH_DISPLAY => (Icon::DetachDisplay, t!("tb-detach"), Some(Command::DetachDisplay)),
        tb::FULL_SCREEN => (Icon::FullScreen, t!("tb-full-screen"), Some(Command::FullScreen)),
        tb::SERIAL_CONSOLE => (Icon::SerialConsole, t!("tb-serial-console"), Some(Command::SerialConsole)),
        tb::MACHINE_SETTINGS => (Icon::MachineSettings, t!("tb-settings"), Some(Command::MachineSettings)),
        _ => return None,
    };
    let mut tip = Tip::new(icon, text);
    if let Some(c) = cmd {
        tip.shortcut = keys.chord_text(c);
    }
    Some(tip)
}

/// The command id a key-map command runs.
pub fn id_of(cmd: Command) -> &'static str {
    match cmd {
        Command::NewMachine => menu::NEW_MACHINE,
        Command::NewFolder => menu::NEW_FOLDER,
        Command::AppSettings => menu::APP_SETTINGS,
        Command::Close => menu::CLOSE,
        Command::Quit => menu::QUIT,
        Command::ShowLibrary => tb::SHOW_LIBRARY,
        Command::FullScreen => tb::FULL_SCREEN,
        Command::DetachDisplay => tb::DETACH_DISPLAY,
        Command::SerialConsole => tb::SERIAL_CONSOLE,
        Command::PowerOn => tb::POWER_ON,
        Command::ShutDownGuest => tb::SHUT_DOWN_GUEST,
        Command::SaveState => tb::SAVE_STATE,
        Command::Screenshot => tb::SCREENSHOT,
        Command::CaptureInput => menu::CAPTURE,
        Command::MachineSettings => tb::MACHINE_SETTINGS,
        Command::BrowseFiles => tb::BROWSE_FILES,
    }
}

fn session_of(id: &str) -> Option<SessionAction> {
    Some(match id {
        tb::POWER_ON => SessionAction::PowerOn,
        tb::SHUT_DOWN_GUEST => SessionAction::ShutDownGuest,
        tb::POWER_OFF => SessionAction::PowerOff,
        tb::RESTART => SessionAction::Restart,
        tb::SAVE_STATE => SessionAction::AskSaveState,
        tb::SCREENSHOT => SessionAction::Screenshot,
        menu::SYNC_DISKS => SessionAction::SynchronizeDisks,
        menu::RESET_NVRAM => SessionAction::ResetNvram,
        _ => return None,
    })
}

impl AppController {
    /// Whether the command `id` can run now, and if not why.
    pub fn command_enabled(&self, id: &str) -> Enabled {
        use super::ids::item;
        if let Some(c) = self.light_command(id) {
            return self.command_enabled(&c);
        }
        if id.starts_with(item::BROWSE_DISK) {
            return self.command_enabled(tb::BROWSE_FILES);
        }
        if id.starts_with(item::INSERT_DISC) {
            return self.command_enabled(tb::INSERT_DISC);
        }
        if let Some(name) = id.strip_prefix(item::RESTORE_STATE) {
            return self.session_enabled(&SessionAction::RestoreState(name.into()));
        }
        if let Some(a) = session_of(id) {
            return self.session_enabled(&a);
        }
        let running = self.session().is_running();
        let machine = || Enabled::when(self.selected().is_some(), || t!("toast-no-machine"));
        let need_running = || Enabled::when(running, || t!("session-needs-running"));
        match id {
            tb::INSERT_DISC => {
                let cd = self.config().scsi.values().any(|d| d.is_cdrom());
                Enabled::when(cd, || t!("tb-no-cdrom"))
            }
            tb::BROWSE_FILES => {
                if running {
                    // The core's read hook is not on this branch yet.
                    Enabled::No(t!("browser-unavailable-running"))
                } else {
                    machine()
                }
            }
            tb::DETACH_DISPLAY | tb::FULL_SCREEN | menu::SEND_F11 | menu::SEND_CAD | menu::SEND_ESC | menu::CAPTURE => {
                need_running()
            }
            tb::SERIAL_CONSOLE | menu::CHECK_NETWORK => need_running(),
            tb::MACHINE_SETTINGS | menu::SHOW_IN_FINDER | menu::OPEN_CONFIG => machine(),
            menu::DUPLICATE | menu::RENAME | menu::DELETE => match machine() {
                Enabled::Yes => self.can_change_machine(),
                no => no,
            },
            menu::NEW_MACHINE => self.can_change_machine(),
            menu::CAMERA_TEST => need_running(),
            _ => Enabled::Yes,
        }
    }

    /// The command a status-bar light's id stands for: a disk opens the
    /// browser, a CD-ROM asks for a disc, network opens the check, serial the
    /// console, capture toggles capture. `None` for other ids and for lights
    /// that only show something.
    pub fn light_command(&self, id: &str) -> Option<String> {
        use super::ids::item;
        use super::session::LightTarget as L;
        if !id.starts_with("status.") {
            return None;
        }
        let bar = self.status_bar();
        let light = bar.lights.iter().find(|l| l.id == id)?;
        Some(match light.target {
            L::Disk { key } => item::id(item::BROWSE_DISK, key),
            L::CdRom { key } => item::id(item::INSERT_DISC, key),
            L::Network => menu::CHECK_NETWORK.to_string(),
            L::Serial => tb::SERIAL_CONSOLE.to_string(),
            L::Capture => menu::CAPTURE.to_string(),
            L::Audio | L::Scale => return None,
        })
    }

    /// Run the command `id` (a toolbar item, a menu item or a key-map chord).
    pub fn command(&mut self, id: &str) -> Routed {
        if let Some(c) = self.light_command(id) {
            return self.command(&c);
        }
        let mut r = Routed::default();
        if let Enabled::No(why) = self.command_enabled(id) {
            r.effects.push(Effect::Toast(why));
            return r;
        }
        if let Some(a) = session_of(id) {
            r.effects = self.apply(AppAction::Session(a));
            return r;
        }
        use super::ids::item;
        let selected = self.selected().map(str::to_string);
        let display = |a: DisplayAction| AppAction::Display(a);
        let per_item = if let Some(n) = id.strip_prefix(item::RESTORE_STATE) {
            Some(SessionAction::RestoreState(n.into()))
        } else if let Some(n) = id.strip_prefix(item::RENAME_STATE) {
            Some(SessionAction::RenameState(n.into()))
        } else if let Some(n) = id.strip_prefix(item::DELETE_STATE) {
            Some(SessionAction::DeleteState(n.into()))
        } else {
            None
        };
        if let Some(a) = per_item {
            r.effects = self.apply(AppAction::Session(a));
            return r;
        }
        if let Some(key) = id.strip_prefix(item::BROWSE_DISK).and_then(|k| k.parse().ok()) {
            r.effects = self.apply(AppAction::OpenBrowser { key });
            return r;
        }
        if let Some(key) = id.strip_prefix(item::INSERT_DISC).and_then(|k| k.parse().ok()) {
            r.effects = self.apply(AppAction::Session(SessionAction::InsertDisc { key }));
            return r;
        }
        let action = match id {
            tb::SHOW_LIBRARY => {
                r.shell = Some(ShellCommand::ToggleLibrary);
                None
            }
            tb::SAVED_STATES => {
                r.shell = Some(ShellCommand::SavedStates);
                None
            }
            tb::INSERT_DISC => {
                let key = self.config().scsi.iter().find(|(_, d)| d.is_cdrom()).map(|(k, _)| *k);
                key.map(|key| AppAction::Session(SessionAction::InsertDisc { key }))
            }
            tb::BROWSE_FILES => Some(AppAction::BrowseFiles),
            tb::DETACH_DISPLAY => Some(display(DisplayAction::ToggleDetached)),
            tb::FULL_SCREEN => Some(display(DisplayAction::ToggleFullScreen)),
            tb::SERIAL_CONSOLE => Some(AppAction::Serial(SerialAction::Open)),
            tb::MACHINE_SETTINGS => {
                r.shell = Some(ShellCommand::Settings);
                None
            }
            menu::CAPTURE => Some(display(DisplayAction::Capture)),
            menu::SEND_F11 => Some(display(DisplayAction::Send(GuestChord::F11))),
            menu::SEND_CAD => Some(display(DisplayAction::Send(GuestChord::CtrlAltDel))),
            menu::SEND_ESC => Some(display(DisplayAction::Send(GuestChord::Escape))),
            menu::CHECK_NETWORK => Some(AppAction::Network(super::network::NetworkAction::OpenCheck)),
            menu::CAMERA_TEST => Some(AppAction::Camera(super::camera::CameraAction::Open)),
            menu::NEW_FOLDER => Some(AppAction::NewFolder),
            menu::DUPLICATE => selected.map(|machine| AppAction::Duplicate { machine }),
            menu::RENAME => selected.map(|machine| AppAction::Rename { machine }),
            menu::DELETE => selected.map(|machine| AppAction::Delete { machine }),
            menu::SHOW_IN_FINDER => selected.map(|machine| AppAction::ShowInFileManager { machine }),
            menu::OPEN_CONFIG => selected.map(|machine| AppAction::OpenConfigFile { machine }),
            menu::QUIT => Some(AppAction::RequestQuit),
            menu::GUIDE => {
                r.effects.push(Effect::OpenUrl("https://github.com/techomancer/iris/blob/main/iris-gui-README.md".into()));
                None
            }
            menu::REPORT_ISSUE => {
                r.effects.push(Effect::OpenUrl("https://github.com/techomancer/iris/issues".into()));
                None
            }
            menu::NEW_MACHINE => {
                r.shell = Some(ShellCommand::NewMachine);
                None
            }
            menu::ABOUT => {
                r.shell = Some(ShellCommand::About);
                None
            }
            menu::APP_SETTINGS => {
                r.shell = Some(ShellCommand::AppSettings);
                None
            }
            menu::BENCHMARK => {
                r.shell = Some(ShellCommand::Benchmark);
                None
            }
            menu::DIAGNOSTICS => {
                r.shell = Some(ShellCommand::Diagnostics);
                None
            }
            menu::CLOSE => {
                r.shell = Some(ShellCommand::CloseWindow);
                None
            }
            _ => None,
        };
        if let Some(a) = action {
            r.effects.extend(self.apply(a));
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_toolbar_id_has_a_tip_and_shortcuts_show() {
        let keys = KeyMap::for_platform(true);
        for id in [
            tb::SHOW_LIBRARY, tb::POWER_ON, tb::SHUT_DOWN_GUEST, tb::POWER_OFF, tb::RESTART, tb::SAVE_STATE,
            tb::SAVED_STATES, tb::SCREENSHOT, tb::INSERT_DISC, tb::BROWSE_FILES, tb::DETACH_DISPLAY,
            tb::FULL_SCREEN, tb::SERIAL_CONSOLE, tb::MACHINE_SETTINGS,
        ] {
            let t = tip(id, &keys).unwrap_or_else(|| panic!("{id}"));
            assert!(!t.tooltip.is_empty());
        }
        assert_eq!(tip(tb::SCREENSHOT, &keys).unwrap().shortcut, "Shift+Cmd+S");
    }

    #[test]
    fn every_key_command_maps_to_an_id() {
        for c in Command::ALL {
            assert!(!id_of(*c).is_empty());
        }
    }
}
