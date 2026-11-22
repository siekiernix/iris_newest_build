//! The running machine: power, saved states, the status-bar lights, CD-ROMs,
//! copy-on-write disks and the safe-stop rules.
//!
//! One session runs at a time (D2), but the session hangs off its own
//! controller, not the app, so several remain possible later.

use crate::model::handle::EmulatorHandle;

/// Progress of a disk synchronisation (folding CHD diffs back into their
/// bases, or applying one copy-on-write overlay).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncProgress {
    /// Disk being synced (0-based) and the total.
    pub disk: usize,
    pub total: usize,
    /// Fraction through the current disk.
    pub fraction: f32,
}

/// The emulator worker and what the GUI knows about the machine it runs.
pub struct SessionController {
    pub(crate) emu: EmulatorHandle,
    pub(crate) syncing: Option<SyncProgress>,
}

impl SessionController {
    pub(crate) fn spawn() -> Self {
        Self { emu: EmulatorHandle::spawn(), syncing: None }
    }

    /// Whether a machine is up (from the worker's lifecycle events).
    pub fn is_running(&self) -> bool {
        self.emu.is_running()
    }

    /// A disk synchronisation in progress, for its progress window.
    pub fn syncing(&self) -> Option<SyncProgress> {
        self.syncing
    }

    /// Whether the worker is idle: no machine, no queued command, no machine
    /// left behind by a stop that timed out. Changing the working directory
    /// (selecting, renaming or deleting a machine) waits for this.
    pub fn is_idle(&self) -> bool {
        self.emu.can_change_directory() && self.syncing.is_none()
    }

    /// The worker handle, for the parts of the egui shell not yet on a
    /// controller. Goes away once every slice has moved.
    #[doc(hidden)]
    pub fn handle(&self) -> &EmulatorHandle {
        &self.emu
    }

    #[doc(hidden)]
    pub fn handle_mut(&mut self) -> &mut EmulatorHandle {
        &mut self.emu
    }

    #[doc(hidden)]
    pub fn set_syncing(&mut self, s: Option<SyncProgress>) {
        self.syncing = s;
    }
}
