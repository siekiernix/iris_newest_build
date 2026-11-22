//! The application: the machine library, the selected machine and its
//! in-memory config, autosave, app preferences, and quitting.
//!
//! Every shell owns one `AppController`. It owns the [`SessionController`] that
//! runs the selected machine, so selecting, renaming or deleting a machine can
//! wait for the worker to be idle.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iris::config::MachineConfig;

use super::profile::{traits, Family, MachineIcon};
use super::session::SessionController;

use super::{Answer, ConfirmSpec, Controller, Effect, Enabled, TextSpec};
use crate::model::job::Notify;
use crate::model::machines;
use crate::model::settings::{GuiSettings, LibraryFolder};

/// How long the config must sit unedited before it is written.
pub const AUTOSAVE_AFTER: Duration = Duration::from_millis(600);

pub const RENAME_MACHINE: &str = "app.rename-machine";
pub const DELETE_MACHINE: &str = "app.delete-machine";
pub const NEW_FOLDER: &str = "app.new-folder";
pub const RENAME_FOLDER: &str = "app.rename-folder";

/// What the user asked the application to do.
#[derive(Debug, Clone)]
pub enum AppAction {
    /// Select a machine in the library; it becomes the working directory.
    Select(String),
    /// Create and select a machine (the new-machine dialog's result).
    CreateMachine { name: String, config: Box<MachineConfig> },
    /// Ask for a new name for `machine`.
    Rename { machine: String },
    /// Copy `machine`; small files are copied, disk images shared.
    Duplicate { machine: String },
    /// Ask, then remove `machine` from the library (its disks stay).
    Delete { machine: String },
    /// Ask for a folder name and add an empty library folder.
    NewFolder,
    RenameFolder { folder: String },
    /// Remove a folder; its machines go back to their family groups.
    DeleteFolder { folder: String },
    /// Put `machine` in `folder`, or back under its family with `None`.
    MoveToFolder { machine: String, folder: Option<String> },
    ToggleFolder { folder: String },
    /// Show the machine's folder in the host's file manager.
    ShowInFileManager { machine: String },
    /// Open the machine's TOML in the user's editor.
    OpenConfigFile { machine: String },
    /// Choose the interface language (empty follows the system).
    SetLanguage(String),
    /// The application-wide UI zoom.
    SetUiScale(f32),
    /// The application-wide guest display scale (D22).
    SetDisplayScale(f32),
    /// The selected machine's in-memory config was edited.
    ConfigEdited,
    /// Write pending edits now (before an action that reads the file).
    Flush,
    /// The user asked to quit (window close, menu, Cmd+Q).
    RequestQuit,
    /// The serial console window.
    Serial(super::serial::SerialAction),
    /// A settings edit on the selected machine.
    Settings(super::settings::SettingsAction),
    /// A display action: capture, keys and pointer, full screen, scaling.
    Display(super::display::DisplayAction),
    /// A session action: power, saved states, discs, disks.
    Session(super::session::SessionAction),
    Answer(Answer),
}

/// A row in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRow {
    pub name: String,
    /// The machine and board, as "Indy · IP24".
    pub subtitle: String,
    pub icon: MachineIcon,
    pub selected: bool,
    /// Shown only for a running machine or one with a problem.
    pub dot: Option<RowDot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowDot {
    Running,
    /// Something will stop it from starting; the text says what.
    Problem(String),
}

/// Which group a library section is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GroupKey {
    Folder(String),
    Family(Family),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryGroup {
    pub key: GroupKey,
    pub title: String,
    pub collapsed: bool,
    pub rows: Vec<LibraryRow>,
}

/// The library, the selected machine and the app preferences.
pub struct AppController {
    prefs: GuiSettings,
    cfg: MachineConfig,
    dirty_since: Option<Instant>,
    session: SessionController,
    display: super::display::DisplayController,
    settings: super::settings::SettingsController,
    serial: super::serial::SerialConsoleController,
    notify: Arc<dyn Notify>,
    /// Problems found the last time the library was checked, by machine.
    problems: std::collections::BTreeMap<String, String>,
    /// The machine a rename or delete prompt is about.
    pending_machine: Option<String>,
    pending_folder: Option<String>,
    /// Quit once the disk synchronisation the quit started has finished.
    quit_after_sync: bool,
    /// Effects produced while constructing, handed out by the first tick.
    startup: Vec<Effect>,
    /// Session events for the shell (display, serial console) since it last asked.
    session_events: Vec<super::session::SessionEvent>,
}

impl AppController {
    /// Take over `prefs` (already loaded) and select its active machine.
    pub fn new(mut prefs: GuiSettings, notify: Arc<dyn Notify>) -> Self {
        let mut cfg = MachineConfig::default();
        let mut startup: Vec<Effect> = Vec::new();
        if let Some(name) = prefs.active_machine.clone() {
            match prefs.machines.get(&name) {
                Some(stored) => cfg = stored.clone(),
                None => prefs.active_machine = None,
            }
        }
        if prefs.active_machine.is_none() {
            prefs.active_machine = prefs.machines.keys().next().cloned();
            if let Some(name) = &prefs.active_machine {
                cfg = prefs.machines[name].clone();
            }
        }
        let dir = match prefs.active_machine.as_deref() {
            Some(name) => prefs.machine_dir(name),
            None => prefs.root().map(std::path::Path::to_path_buf),
        };
        let mut errors = prefs.errors.clone();
        if let Err(e) = dir.and_then(|dir| machines::enter(&dir)) {
            errors.push(e);
        }
        if !errors.is_empty() {
            startup.push(Effect::Toast(errors.join("\n")));
        }
        let mut app = Self {
            prefs,
            cfg,
            dirty_since: None,
            session: SessionController::spawn(notify.clone()),
            display: Default::default(),
            settings: Default::default(),
            serial: Default::default(),
            notify,
            problems: Default::default(),
            pending_machine: None,
            pending_folder: None,
            quit_after_sync: false,
            startup,
            session_events: Vec::new(),
        };
        app.check_library();
        app
    }

    // ---- read accessors -----------------------------------------------------

    pub fn prefs(&self) -> &GuiSettings {
        &self.prefs
    }

    /// The selected machine's name.
    pub fn selected(&self) -> Option<&str> {
        self.prefs.active_machine.as_deref()
    }

    /// The selected machine's in-memory config (what the settings show).
    pub fn config(&self) -> &MachineConfig {
        &self.cfg
    }

    /// The config for the settings editor to change in place; follow every
    /// change with [`AppAction::ConfigEdited`]. Goes away when the settings
    /// form is driven by the settings controller.
    #[doc(hidden)]
    pub fn config_mut(&mut self) -> &mut MachineConfig {
        &mut self.cfg
    }

    #[doc(hidden)]
    pub fn prefs_mut(&mut self) -> &mut GuiSettings {
        &mut self.prefs
    }

    /// Edits not yet written to the machine's TOML.
    pub fn is_dirty(&self) -> bool {
        self.dirty_since.is_some()
    }

    /// No machine exists yet: the shell opens the new-machine dialog.
    pub fn is_empty(&self) -> bool {
        self.prefs.machines.is_empty()
    }

    pub fn session(&self) -> &SessionController {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut SessionController {
        &mut self.session
    }

    pub fn serial(&self) -> &super::serial::SerialConsoleController {
        &self.serial
    }

    /// The selected machine's settings, as sections of fields.
    pub fn settings_sections(&self) -> Vec<super::settings::Section> {
        self.settings.sections(&self.cfg, self.session.is_running())
    }

    pub fn display(&self) -> &super::display::DisplayController {
        &self.display
    }

    /// A message over the display: why there is no picture.
    pub fn display_overlay(&self) -> Option<String> {
        use super::session::RunState;
        match self.session.run_state() {
            RunState::PoweredOff => Some(t!("fb-powered-off")),
            RunState::Starting => Some(t!("status-starting")),
            RunState::Stopped | RunState::Running | RunState::Halted => None,
        }
    }

    pub fn notify(&self) -> Arc<dyn Notify> {
        self.notify.clone()
    }

    /// The selected machine's folder.
    pub fn machine_dir(&self) -> Option<PathBuf> {
        self.selected().and_then(|n| self.prefs.machine_dir(n).ok())
    }

    /// Whether the library can change now (select, rename, delete, create):
    /// settings writable, the worker idle.
    pub fn can_change_machine(&self) -> Enabled {
        if let Err(e) = self.prefs.ensure_writable() {
            return Enabled::No(e);
        }
        Enabled::when(self.session.is_idle(), || t!("toast-switch-machine-busy"))
    }

    /// The library: user folders first, then the families that have machines.
    pub fn library(&self) -> Vec<LibraryGroup> {
        let running = self.session.is_running();
        let row = |name: &str| -> Option<LibraryRow> {
            let cfg = self.prefs.machines.get(name)?;
            let t = traits(cfg.machine.profile);
            let selected = self.selected() == Some(name);
            let dot = if selected && running {
                Some(RowDot::Running)
            } else {
                self.problems.get(name).map(|p| RowDot::Problem(p.clone()))
            };
            Some(LibraryRow {
                name: name.to_string(),
                subtitle: t!("library-row-subtitle", model = t.model, board = t.board),
                icon: t.icon,
                selected,
                dot,
            })
        };
        let mut groups = Vec::new();
        let mut placed = std::collections::BTreeSet::new();
        for f in &self.prefs.library_folders {
            let rows: Vec<_> = f.machines.iter().filter_map(|m| row(m)).collect();
            placed.extend(rows.iter().map(|r| r.name.clone()));
            groups.push(LibraryGroup {
                key: GroupKey::Folder(f.name.clone()),
                title: f.name.clone(),
                collapsed: f.collapsed,
                rows,
            });
        }
        for &family in Family::ALL {
            let rows: Vec<_> = self
                .prefs
                .machines
                .iter()
                .filter(|(n, c)| !placed.contains(*n) && traits(c.machine.profile).family == family)
                .filter_map(|(n, _)| row(n))
                .collect();
            if !rows.is_empty() {
                groups.push(LibraryGroup { key: GroupKey::Family(family), title: family.title(), collapsed: false, rows });
            }
        }
        groups
    }

    /// The folder `machine` is in, if any.
    pub fn folder_of(&self, machine: &str) -> Option<&str> {
        self.prefs
            .library_folders
            .iter()
            .find(|f| f.machines.iter().any(|m| m == machine))
            .map(|f| f.name.as_str())
    }

    // ---- internals ------------------------------------------------------------

    fn mark_dirty(&mut self) -> Vec<Effect> {
        let mut fx = Vec::new();
        if let Some(dir) = self.machine_dir() {
            if let Err(e) = machines::make_relative(&mut self.cfg, &dir) {
                fx.push(Effect::Toast(e));
            }
        }
        self.dirty_since = Some(Instant::now());
        fx
    }

    /// Write the selected machine's config. On failure the edit is kept.
    fn flush(&mut self, fx: &mut Vec<Effect>) -> bool {
        let Some(name) = self.prefs.active_machine.clone() else { return false };
        if let Err(e) = self.prefs.save_machine(&name, &mut self.cfg) {
            fx.push(Effect::Toast(t!("toast-autosave-failed", error = e)));
            return false;
        }
        self.dirty_since = None;
        true
    }

    fn guard(&self, fx: &mut Vec<Effect>) -> bool {
        match self.can_change_machine() {
            Enabled::Yes => true,
            Enabled::No(why) => {
                fx.push(Effect::Toast(why));
                false
            }
        }
    }

    fn save_prefs(&mut self, fx: &mut Vec<Effect>) {
        if let Err(e) = self.prefs.save() {
            fx.push(Effect::Toast(t!("toast-settings-save-failed", error = e)));
        }
    }

    /// Note which machines will not start as they are (a missing disk image),
    /// for the library's problem dots.
    fn check_library(&mut self) {
        self.problems.clear();
        let Ok(root) = self.prefs.root().map(|r| r.to_path_buf()) else { return };
        for (name, cfg) in &self.prefs.machines {
            let Ok(dir) = machines::directory(&root, name) else { continue };
            let readable = |p: &str| {
                iris::http_disk::is_url(p) || machines::resolve(&dir, p).is_file()
            };
            if let Some(m) = crate::model::summary::missing_disks(cfg, readable).first() {
                self.problems.insert(name.clone(), t!("library-problem-missing-disk", disk = m.name.as_str()));
            } else if let Err(e) = cfg.validate() {
                self.problems.insert(name.clone(), e.to_string());
            }
        }
    }

    fn select(&mut self, name: &str, fx: &mut Vec<Effect>) {
        if !self.guard(fx) {
            return;
        }
        if self.is_dirty() && !self.flush(fx) {
            return;
        }
        let result = (|| -> Result<MachineConfig, String> {
            let mut cfg = machines::read(&self.prefs.machine_path(name)?)?;
            let dir = self.prefs.machine_dir(name)?;
            machines::make_relative(&mut cfg, &dir)?;
            machines::enter(&dir)?;
            Ok(cfg)
        })();
        match result {
            Ok(cfg) => {
                self.cfg = cfg;
                self.prefs.machines.insert(name.into(), self.cfg.clone());
                self.prefs.active_machine = Some(name.into());
                match self.prefs.save() {
                    Err(e) => fx.push(Effect::Toast(t!("toast-settings-save-failed", error = e))),
                    Ok(()) => fx.push(Effect::Toast(t!("toast-machine-loaded", name = name))),
                }
            }
            Err(e) => fx.push(Effect::Toast(t!("toast-machine-load-failed", error = e))),
        }
    }

    fn create(&mut self, name: &str, mut cfg: MachineConfig, fx: &mut Vec<Effect>) {
        if !self.guard(fx) {
            return;
        }
        if let Err(e) = machines::validate_name(name) {
            fx.push(Effect::Toast(e));
            return;
        }
        if self.is_dirty() && !self.flush(fx) {
            return;
        }
        let name = self.prefs.unique_name(name);
        match self.prefs.save_machine(&name, &mut cfg) {
            Ok(()) => {
                self.select(&name, fx);
                fx.push(Effect::Toast(t!("toast-machine-created", name = &name)));
            }
            Err(e) => fx.push(Effect::Toast(t!("toast-machine-create-failed", error = e))),
        }
        self.check_library();
    }

    fn rename(&mut self, old: &str, new: &str, fx: &mut Vec<Effect>) {
        if !self.guard(fx) {
            return;
        }
        let new = new.trim();
        if new == old {
            return;
        }
        if let Err(e) = machines::validate_name(new) {
            fx.push(Effect::Toast(e));
            return;
        }
        let active = self.selected() == Some(old);
        if active && self.is_dirty() && !self.flush(fx) {
            return;
        }
        let name = self.prefs.unique_name(new);
        // The folder may be the working directory, and Windows won't rename that.
        let result = self.prefs.root().map(|r| r.to_path_buf()).and_then(|root| {
            std::env::set_current_dir(&root).map_err(|e| e.to_string())?;
            machines::rename(&root, old, &name)
        });
        if let Err(e) = result {
            fx.push(Effect::Toast(t!("toast-rename-failed", error = e)));
            if let Err(e) = self.machine_dir().ok_or_default().and_then(|d| machines::enter(&d)) {
                fx.push(Effect::Toast(e));
            }
            return;
        }
        if let Some(cfg) = self.prefs.machines.remove(old) {
            self.prefs.machines.insert(name.clone(), if active { self.cfg.clone() } else { cfg });
        }
        for f in &mut self.prefs.library_folders {
            for m in &mut f.machines {
                if m == old {
                    *m = name.clone();
                }
            }
        }
        if active {
            self.prefs.active_machine = Some(name.clone());
        }
        if let Err(e) = self.machine_dir().ok_or_default().and_then(|d| machines::enter(&d)) {
            fx.push(Effect::Toast(e));
            return;
        }
        match self.prefs.save() {
            Err(e) => fx.push(Effect::Toast(e)),
            Ok(()) => fx.push(Effect::Toast(t!("toast-renamed", name = &name))),
        }
        self.check_library();
    }

    fn delete(&mut self, name: &str, fx: &mut Vec<Effect>) {
        if !self.guard(fx) {
            return;
        }
        let result = self.prefs.machine_path(name).and_then(|p| std::fs::remove_file(p).map_err(|e| e.to_string()));
        if let Err(e) = result {
            fx.push(Effect::Toast(t!("toast-delete-failed", error = e)));
            return;
        }
        self.prefs.machines.remove(name);
        for f in &mut self.prefs.library_folders {
            f.machines.retain(|m| m != name);
        }
        if self.selected() == Some(name) {
            self.dirty_since = None;
            self.prefs.active_machine = None;
            if let Some(next) = self.prefs.machines.keys().next().cloned() {
                self.select(&next, fx);
            } else {
                self.cfg = MachineConfig::default();
                if let Err(e) = self.prefs.root().and_then(machines::enter) {
                    fx.push(Effect::Toast(e));
                }
                self.save_prefs(fx);
            }
        } else {
            self.save_prefs(fx);
        }
        fx.push(Effect::Toast(t!("toast-deleted", name = name)));
        self.check_library();
    }

    fn duplicate(&mut self, name: &str, fx: &mut Vec<Effect>) {
        if !self.guard(fx) {
            return;
        }
        if self.selected() == Some(name) && self.is_dirty() && !self.flush(fx) {
            return;
        }
        let copy = self.prefs.unique_name(name);
        let result = self.prefs.root().map(|r| r.to_path_buf()).and_then(|root| machines::duplicate(&root, name, &copy));
        match result {
            Ok((cfg, shared)) => {
                self.prefs.machines.insert(copy.clone(), cfg);
                if let Some(folder) = self.folder_of(name).map(str::to_string) {
                    if let Some(f) = self.prefs.library_folders.iter_mut().find(|f| f.name == folder) {
                        f.machines.push(copy.clone());
                    }
                }
                self.save_prefs(fx);
                fx.push(Effect::Toast(if shared.is_empty() {
                    t!("toast-duplicated", name = &copy)
                } else {
                    t!("toast-duplicated-shared", name = &copy, count = shared.len())
                }));
            }
            Err(e) => fx.push(Effect::Toast(t!("toast-duplicate-failed", error = e))),
        }
        self.check_library();
    }

    fn folder_names(&self) -> Vec<String> {
        self.prefs.library_folders.iter().map(|f| f.name.clone()).collect()
    }

    fn ask_folder_name(&self, purpose: &'static str, title: String, initial: String) -> Effect {
        Effect::AskText(TextSpec {
            purpose,
            title,
            prompt: t!("library-folder-name"),
            initial: initial.clone(),
            ok_label: t!("common-ok"),
            taken: self.folder_names().into_iter().filter(|n| *n != initial).collect(),
            error: None,
            validator: folder_name_rule,
        })
    }

    fn answer(&mut self, answer: Answer, fx: &mut Vec<Effect>) {
        match answer {
            Answer::Text { purpose: RENAME_MACHINE, text } => {
                if let (Some(old), Some(new)) = (self.pending_machine.take(), text) {
                    self.rename(&old, &new, fx);
                }
            }
            Answer::Confirmed { purpose: DELETE_MACHINE, yes } => {
                if let (Some(name), true) = (self.pending_machine.take(), yes) {
                    self.delete(&name, fx);
                }
            }
            Answer::Text { purpose: NEW_FOLDER, text: Some(name) } => {
                let name = name.trim().to_string();
                if folder_name_rule_with(&self.folder_names(), &name).is_ok() {
                    self.prefs.library_folders.push(LibraryFolder { name, machines: Vec::new(), collapsed: false });
                    self.save_prefs(fx);
                }
            }
            Answer::Text { purpose: RENAME_FOLDER, text: Some(name) } => {
                let name = name.trim().to_string();
                let Some(old) = self.pending_folder.take() else { return };
                let others: Vec<_> = self.folder_names().into_iter().filter(|n| *n != old).collect();
                if folder_name_rule_with(&others, &name).is_ok() {
                    if let Some(f) = self.prefs.library_folders.iter_mut().find(|f| f.name == old) {
                        f.name = name;
                    }
                    self.save_prefs(fx);
                }
            }
            Answer::Text { purpose: RENAME_FOLDER, text: None } => self.pending_folder = None,
            // Cancelled prompts and answers to someone else's question.
            Answer::Text { .. } | Answer::Confirmed { .. } | Answer::Picked { .. } | Answer::Chose { .. } => {}
        }
    }

    fn request_quit(&mut self, fx: &mut Vec<Effect>) {
        if self.session.syncing.is_some() {
            // Still folding disks: quit when that ends.
            self.quit_after_sync = true;
            return;
        }
        if self.session.emu.has_pending_chd_sync() && !self.quit_after_sync {
            // Fold CHD diffs back into their bases first; quit on SyncDone.
            self.quit_after_sync = true;
            self.session.syncing = Some(super::session::SyncProgress { disk: 0, total: 0, fraction: 0.0 });
            self.session.emu.send(crate::model::handle::Cmd::SyncDisks);
            return;
        }
        if self.is_dirty() {
            self.flush(fx);
        } else {
            self.save_prefs(fx);
        }
        fx.push(Effect::Quit);
    }

    /// Whether the app is waiting for a disk sync before it quits.
    pub fn quitting(&self) -> bool {
        self.quit_after_sync
    }

    /// The last step of quitting, once the shell's window is going away: stop
    /// the machine and join the worker. Bounded by the worker's stop timeout.
    pub fn shutdown(&mut self) {
        if self.is_dirty() {
            let mut fx = Vec::new();
            self.flush(&mut fx);
        } else {
            let _ = self.prefs.save();
        }
        self.session.emu.shutdown();
    }
}

trait OkOrDefault<T> {
    fn ok_or_default(self) -> Result<T, String>;
}

impl<T> OkOrDefault<T> for Option<T> {
    fn ok_or_default(self) -> Result<T, String> {
        self.ok_or_else(|| t!("toast-no-machine"))
    }
}

fn folder_name_rule_with(taken: &[String], name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(String::new());
    }
    if taken.iter().any(|t| t.eq_ignore_ascii_case(name)) {
        return Err(t!("library-folder-taken"));
    }
    Ok(())
}

fn folder_name_rule(spec: &TextSpec, text: &str) -> Result<(), String> {
    folder_name_rule_with(&spec.taken, text)
}

fn machine_name_rule(spec: &TextSpec, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() || text == spec.initial {
        return Err(String::new());
    }
    machines::validate_name(text)
}

impl Controller for AppController {
    type Action = AppAction;

    fn apply(&mut self, action: AppAction) -> Vec<Effect> {
        let mut fx = Vec::new();
        match action {
            AppAction::Select(name) => self.select(&name, &mut fx),
            AppAction::CreateMachine { name, config } => self.create(&name, *config, &mut fx),
            AppAction::Rename { machine } => {
                if self.guard(&mut fx) {
                    fx.push(Effect::AskText(TextSpec {
                        purpose: RENAME_MACHINE,
                        title: t!("modal-rename-title"),
                        prompt: t!("modal-rename-label"),
                        initial: machine.clone(),
                        ok_label: t!("modal-rename-confirm"),
                        taken: Vec::new(),
                        error: None,
                        validator: machine_name_rule,
                    }));
                    self.pending_machine = Some(machine);
                }
            }
            AppAction::Duplicate { machine } => self.duplicate(&machine, &mut fx),
            AppAction::Delete { machine } => {
                if self.guard(&mut fx) {
                    fx.push(Effect::Confirm(ConfirmSpec {
                        purpose: DELETE_MACHINE,
                        title: t!("library-delete-title", name = &machine),
                        message: t!("library-delete-message"),
                        confirm_label: t!("library-delete-ok"),
                        destructive: true,
                    }));
                    self.pending_machine = Some(machine);
                }
            }
            AppAction::NewFolder => fx.push(self.ask_folder_name(NEW_FOLDER, t!("library-new-folder"), String::new())),
            AppAction::RenameFolder { folder } => {
                fx.push(self.ask_folder_name(RENAME_FOLDER, t!("library-rename-folder"), folder.clone()));
                self.pending_folder = Some(folder);
            }
            AppAction::DeleteFolder { folder } => {
                self.prefs.library_folders.retain(|f| f.name != folder);
                self.save_prefs(&mut fx);
            }
            AppAction::MoveToFolder { machine, folder } => {
                for f in &mut self.prefs.library_folders {
                    f.machines.retain(|m| *m != machine);
                }
                if let Some(folder) = folder {
                    if let Some(f) = self.prefs.library_folders.iter_mut().find(|f| f.name == folder) {
                        f.machines.push(machine);
                    }
                }
                self.save_prefs(&mut fx);
            }
            AppAction::ToggleFolder { folder } => {
                if let Some(f) = self.prefs.library_folders.iter_mut().find(|f| f.name == folder) {
                    f.collapsed = !f.collapsed;
                }
                self.save_prefs(&mut fx);
            }
            AppAction::ShowInFileManager { machine } => match self.prefs.machine_dir(&machine) {
                Ok(dir) => fx.push(Effect::RevealPath(dir)),
                Err(e) => fx.push(Effect::Toast(e)),
            },
            AppAction::OpenConfigFile { machine } => {
                if self.selected() == Some(machine.as_str()) && self.is_dirty() {
                    self.flush(&mut fx);
                }
                match self.prefs.machine_path(&machine) {
                    Ok(path) => fx.push(Effect::OpenPath(path)),
                    Err(e) => fx.push(Effect::Toast(e)),
                }
            }
            AppAction::SetLanguage(lang) => {
                crate::model::i18n::set_language(&lang);
                self.prefs.language = lang;
                self.save_prefs(&mut fx);
            }
            AppAction::SetUiScale(s) => {
                self.prefs.ui_scale = s.clamp(crate::model::settings::UI_SCALE_MIN, crate::model::settings::UI_SCALE_MAX);
                self.save_prefs(&mut fx);
            }
            AppAction::SetDisplayScale(s) => {
                self.prefs.vm_scale = s;
                self.save_prefs(&mut fx);
            }
            AppAction::ConfigEdited => fx.extend(self.mark_dirty()),
            AppAction::Flush => {
                if self.is_dirty() {
                    self.flush(&mut fx);
                }
            }
            AppAction::RequestQuit => self.request_quit(&mut fx),
            AppAction::Session(a) => self.session_apply(a, &mut fx),
            AppAction::Serial(a) => self.serial.apply(a, &self.cfg, self.notify.clone()),
            AppAction::Settings(a) => {
                use super::settings::{SettingsAction, SettingsController};
                let live = match &a {
                    SettingsAction::Set { field, .. } => SettingsController::live_change(field),
                    SettingsAction::Forward(_) => true,
                    _ => false,
                };
                let dir = self.machine_dir();
                let (changed, more) = self.settings.apply(a, &mut self.cfg, dir);
                fx.extend(more);
                if changed {
                    fx.extend(self.mark_dirty());
                    if live && self.session.is_running() {
                        use crate::model::handle::Cmd;
                        if let Some(subnet) = self.cfg.nat_subnet.clone().filter(|s| iris::config::parse_nat_subnet(s).is_ok()) {
                            self.session.send(Cmd::SetNatSubnet(subnet));
                        }
                        self.session.send(Cmd::SetPortForwards(self.cfg.port_forward.clone()));
                        self.session.send(Cmd::SetPcapInterface(self.cfg.network.pcap_interface.clone()));
                    }
                }
            }
            AppAction::Display(a) => {
                let ps2 = self.session.ps2();
                let guest = ps2.as_deref().map(|p| p as &dyn super::display::GuestInput);
                let per_line = self.cfg.mouse_scroll_pixels_per_line;
                fx.extend(self.display.apply(a, guest, &mut self.prefs, per_line));
            }
            AppAction::Answer(a) => {
                if a.purpose().starts_with("settings.") {
                    fx.extend(self.apply(AppAction::Settings(super::settings::SettingsAction::Answer(a))));
                } else if a.purpose().starts_with("session.") {
                    self.session_apply(super::session::SessionAction::Answer(a), &mut fx);
                } else {
                    self.answer(a, &mut fx);
                }
            }
        }
        fx
    }

    fn tick(&mut self) -> Vec<Effect> {
        let mut fx = std::mem::take(&mut self.startup);
        self.session.drain(&mut fx);
        let events = self.route_session_events(&mut fx);
        use super::session::SessionEvent;
        if events.iter().any(|e| matches!(e, SessionEvent::Stopped | SessionEvent::Halted | SessionEvent::PoweredOff)) {
            if self.display.captured() && events.contains(&SessionEvent::Halted) {
                // Give the cursor back once the guest is safe to stop.
                fx.push(Effect::Toast(t!("toast-guest-halted")));
            }
            self.display.force_release();
        }
        let ps2 = self.session.ps2();
        self.display.tick(ps2.as_deref().map(|p| p as &dyn super::display::GuestInput));
        if events.contains(&SessionEvent::Started) {
            self.serial.reconnect(&self.cfg, self.notify.clone());
        }
        self.session_events.extend(events);
        if self.dirty_since.is_some_and(|t| t.elapsed() >= AUTOSAVE_AFTER) {
            self.flush(&mut fx);
        }
        fx
    }
}

impl AppController {
    /// Session events (started, stopped, halted, powered off, sync done) since
    /// the last call, for the parts of the shell that react to them.
    pub fn take_session_events(&mut self) -> Vec<super::session::SessionEvent> {
        std::mem::take(&mut self.session_events)
    }

    pub(crate) fn apply_config_edited(&mut self) -> Vec<Effect> {
        self.mark_dirty()
    }

    pub(crate) fn apply_flush(&mut self) -> Vec<Effect> {
        let mut fx = Vec::new();
        self.flush(&mut fx);
        fx
    }

    /// The time until the autosave is due, so a shell that sleeps between
    /// wakes can schedule one.
    pub fn autosave_due_in(&self) -> Option<Duration> {
        self.dirty_since.map(|t| AUTOSAVE_AFTER.saturating_sub(t.elapsed()))
    }

    /// A disk sync ended: if it was the quit's first step, finish quitting.
    #[doc(hidden)]
    pub fn sync_finished(&mut self) -> Vec<Effect> {
        let mut fx = Vec::new();
        if self.quit_after_sync {
            if self.is_dirty() {
                self.flush(&mut fx);
            }
            fx.push(Effect::Quit);
        }
        fx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::job::NoWake;
    use crate::model::machines::testing::TempRoot;

    fn app_in(root: &TempRoot, names: &[&str]) -> AppController {
        let mut prefs = GuiSettings::load_in(root.0.clone(), &root.0);
        for n in names {
            let mut cfg = MachineConfig::default();
            cfg.scsi.clear();
            prefs.save_machine(n, &mut cfg).unwrap();
        }
        prefs.active_machine = names.first().map(|s| s.to_string());
        prefs.save().unwrap();
        AppController::new(GuiSettings::load_in(root.0.clone(), &root.0), Arc::new(NoWake))
    }

    // The working directory is process-wide: run the cwd-changing tests in
    // one thread, one after another.
    static CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn library_groups_by_family_and_folders_come_first() {
        let _g = CWD.lock().unwrap_or_else(|e| e.into_inner());
        let root = TempRoot::new("lib");
        let mut app = app_in(&root, &["alpha", "beta"]);
        let groups = app.library();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key, GroupKey::Family(Family::Indy));
        assert_eq!(groups[0].rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["alpha", "beta"]);
        assert!(groups[0].rows[0].selected);
        assert_eq!(groups[0].rows[0].dot, None, "a stopped machine with no problem has no dot");

        app.apply(AppAction::NewFolder);
        app.apply(AppAction::Answer(Answer::Text { purpose: NEW_FOLDER, text: Some("Work".into()) }));
        app.apply(AppAction::MoveToFolder { machine: "beta".into(), folder: Some("Work".into()) });
        let groups = app.library();
        assert_eq!(groups[0].key, GroupKey::Folder("Work".into()));
        assert_eq!(groups[0].rows[0].name, "beta");
        assert_eq!(groups[1].rows.len(), 1, "beta is no longer under its family");

        // Folders persist in gui.json.
        let reloaded = GuiSettings::load_in(root.0.clone(), &root.0);
        assert_eq!(reloaded.library_folders[0].machines, ["beta"]);
        app.shutdown();
    }

    #[test]
    fn rename_and_delete_go_through_prompts() {
        let _g = CWD.lock().unwrap_or_else(|e| e.into_inner());
        let root = TempRoot::new("rename");
        let mut app = app_in(&root, &["alpha", "beta"]);
        let fx = app.apply(AppAction::Rename { machine: "alpha".into() });
        let Some(Effect::AskText(spec)) = fx.first() else { panic!("{fx:?}") };
        assert!(spec.validate("alpha").is_err(), "unchanged");
        assert!(spec.validate("gamma").is_ok());
        app.apply(AppAction::Answer(Answer::Text { purpose: RENAME_MACHINE, text: Some("gamma".into()) }));
        assert_eq!(app.selected(), Some("gamma"));
        assert_eq!(std::env::current_dir().unwrap(), app.prefs.machine_dir("gamma").unwrap());

        let fx = app.apply(AppAction::Delete { machine: "gamma".into() });
        assert!(matches!(fx.first(), Some(Effect::Confirm(c)) if c.destructive));
        // A "no" leaves it alone.
        app.apply(AppAction::Answer(Answer::Confirmed { purpose: DELETE_MACHINE, yes: false }));
        assert!(app.prefs.machines.contains_key("gamma"));
        app.apply(AppAction::Delete { machine: "gamma".into() });
        app.apply(AppAction::Answer(Answer::Confirmed { purpose: DELETE_MACHINE, yes: true }));
        assert!(!app.prefs.machines.contains_key("gamma"));
        assert_eq!(app.selected(), Some("beta"));
        app.shutdown();
    }

    #[test]
    fn edits_autosave_after_a_pause() {
        let _g = CWD.lock().unwrap_or_else(|e| e.into_inner());
        let root = TempRoot::new("autosave");
        let mut app = app_in(&root, &["alpha"]);
        app.config_mut().serial_log = Some("serial.log".into());
        app.apply(AppAction::ConfigEdited);
        assert!(app.is_dirty());
        assert!(app.tick().is_empty() && app.is_dirty(), "not yet");
        app.dirty_since = Some(Instant::now() - AUTOSAVE_AFTER);
        app.tick();
        assert!(!app.is_dirty());
        let saved = machines::read(&app.prefs.machine_path("alpha").unwrap()).unwrap();
        assert_eq!(saved.serial_log.as_deref(), Some("serial.log"));
        app.shutdown();
    }

    #[test]
    fn quitting_with_nothing_pending_saves_and_quits() {
        let _g = CWD.lock().unwrap_or_else(|e| e.into_inner());
        let root = TempRoot::new("quit");
        let mut app = app_in(&root, &["alpha"]);
        let fx = app.apply(AppAction::RequestQuit);
        assert_eq!(fx.last(), Some(&Effect::Quit));
        app.shutdown();
    }

    /// Power a disk-less Indy on through the session, see it run, and power
    /// it off: at the PROM with no disks the stop needs no question.
    #[test]
    fn power_on_and_off_through_the_session() {
        let _g = CWD.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("IRIS_NO_EXIT_ON_POWEROFF", "1");
        let root = TempRoot::new("power");
        let mut prefs = GuiSettings::load_in(root.0.clone(), &root.0);
        let mut cfg = MachineConfig::default();
        cfg.banks = iris::config::banks_from4([8, 8, 0, 0]);
        cfg.scsi.clear();
        cfg.headless = true;
        cfg.monitor_port = Some(0);
        cfg.serial_port_a = Some(0);
        cfg.serial_port_b = Some(0);
        prefs.save_machine("indy", &mut cfg).unwrap();
        prefs.active_machine = Some("indy".into());
        prefs.save().unwrap();
        let mut app = AppController::new(GuiSettings::load_in(root.0.clone(), &root.0), Arc::new(NoWake));
        let wait = |app: &mut AppController, what: &str, done: &dyn Fn(&AppController) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(20);
            while !done(app) && Instant::now() < deadline {
                app.tick();
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(done(app), "{what}");
        };
        let fx = app.apply(AppAction::Session(super::super::session::SessionAction::PowerOn));
        assert!(fx.iter().all(|e| matches!(e, Effect::Toast(_))), "{fx:?}");
        wait(&mut app, "started", &|a| a.session().is_running());
        assert!(app.take_session_events().contains(&super::super::session::SessionEvent::Started));
        assert!(std::path::Path::new("nvram.bin").exists(), "the NVRAM was seeded");
        let bar = app.status_bar();
        assert_eq!(bar.executor, super::super::session::SessionController::executor());
        let fx = app.apply(AppAction::Session(super::super::session::SessionAction::PowerOff));
        assert!(!fx.iter().any(|e| matches!(e, Effect::Choose(_))), "no disks, no question: {fx:?}");
        wait(&mut app, "stopped", &|a| !a.session().is_running() && a.session().is_idle());
        app.shutdown();
    }

    #[test]
    fn duplicate_selects_nothing_and_joins_the_folder() {
        let _g = CWD.lock().unwrap_or_else(|e| e.into_inner());
        let root = TempRoot::new("dupapp");
        let mut app = app_in(&root, &["alpha"]);
        app.apply(AppAction::NewFolder);
        app.apply(AppAction::Answer(Answer::Text { purpose: NEW_FOLDER, text: Some("Lab".into()) }));
        app.apply(AppAction::MoveToFolder { machine: "alpha".into(), folder: Some("Lab".into()) });
        app.apply(AppAction::Duplicate { machine: "alpha".into() });
        assert!(app.prefs.machines.contains_key("alpha-2"));
        assert_eq!(app.selected(), Some("alpha"));
        assert_eq!(app.prefs.library_folders[0].machines, ["alpha", "alpha-2"]);
        app.shutdown();
    }
}
