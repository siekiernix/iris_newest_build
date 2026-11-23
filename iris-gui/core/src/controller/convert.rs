//! Converting a machine's compressed CHD disk so that stopping it is quick
//! (`model::chd_convert`). The machine page suggests it for disks compressed
//! with lzma or zstd, whose rebuild after every stop takes minutes; the
//! Devices menu offers it for any compressed CHD. zlib comes first: it stays
//! small. Uncompressed never rebuilds, but its file can be very large.

use std::path::PathBuf;

use crate::model::chd_convert::{self, ChdFormat, ChdHeader};
use crate::model::job::Job;
use crate::model::machines;

use super::app::{AppAction, AppController};
use super::{Answer, ChoiceSpec, Effect, Enabled, JobSnapshot};

/// Which format a compressed disk converts to.
pub const CONVERT_FORMAT: &str = "app.convert-format";
/// Cancels a running conversion (the status bar's button).
pub const CANCEL_ID: &str = "app.convert-cancel";

/// A conversion in progress.
pub(crate) struct Conversion {
    path: String,
    label: String,
    format: ChdFormat,
    before: u64,
    job: Job<u64>,
}

/// A format question waiting for its answer.
pub(crate) struct Asked {
    key: u8,
    formats: Vec<ChdFormat>,
}

impl AppController {
    /// The selected machine's `[scsi.KEY]` disk as a CHD: its label, path
    /// and header, once the disk probe has read it.
    fn chd_disk(&self, key: u8) -> Option<(String, String, ChdHeader)> {
        let cfg = self.config();
        let d = cfg.scsi.get(&key)?;
        if d.is_cdrom() || d.path.trim().is_empty() {
            return None;
        }
        let dir = self.machine_dir()?;
        let path = machines::resolve(&dir, &d.path).to_string_lossy().into_owned();
        self.probe.readable(&path);
        let header = self.probe.chd_header(&path)?;
        let label = t!("convert-disk-label", id = crate::model::hardware::scsi_name(cfg, key));
        Some((label, path, header))
    }

    /// Whether `[scsi.KEY]` is a compressed CHD, and so can be converted.
    pub fn convertible(&self, key: u8) -> bool {
        self.chd_disk(key).is_some_and(|(_, _, h)| h.compressed())
    }

    /// Whether the machine page should suggest converting `[scsi.KEY]`:
    /// compressed with a codec that is slow to rebuild.
    pub fn suggest_convert(&self, key: u8) -> bool {
        self.chd_disk(key).is_some_and(|(_, _, h)| h.slow())
    }

    pub(crate) fn convert_enabled(&self) -> Enabled {
        if self.conversion.is_some() {
            return Enabled::No(t!("convert-busy"));
        }
        if self.session().is_running() {
            return Enabled::No(t!("convert-needs-stopped"));
        }
        self.can_change_machine()
    }

    /// The running conversion, for the status bar.
    pub fn conversion(&self) -> Option<JobSnapshot> {
        self.conversion.as_ref().map(|c| c.job.snapshot())
    }

    /// Ask which format `[scsi.KEY]` converts to.
    pub(crate) fn ask_convert(&mut self, key: u8, fx: &mut Vec<Effect>) {
        if let Enabled::No(why) = self.convert_enabled() {
            fx.push(Effect::Toast(why));
            return;
        }
        let Some((label, _, h)) = self.chd_disk(key).filter(|(_, _, h)| h.compressed()) else {
            fx.push(Effect::Toast(t!("convert-not-compressed")));
            return;
        };
        let human = super::disk_browser::human;
        let now = h.codecs.join(", ");
        let mut message = t!("convert-intro", disk = label.as_str(), codecs = now.as_str(), size = human(h.file_bytes));
        let mut formats = Vec::new();
        let mut choices = Vec::new();
        if !h.zlib_only() {
            formats.push(ChdFormat::Zlib);
            choices.push(t!("convert-to-zlib"));
            message.push_str("\n\n");
            message.push_str(&t!("convert-zlib-body", size = human(chd_convert::room_needed(&h, ChdFormat::Zlib))));
        }
        formats.push(ChdFormat::Uncompressed);
        choices.push(t!("convert-to-uncompressed"));
        message.push_str("\n\n");
        message.push_str(&t!("convert-uncompressed-body", size = human(h.logical_bytes)));
        message.push_str("\n\n");
        message.push_str(&t!("convert-outro"));
        choices.push(t!("common-cancel"));
        fx.push(Effect::Choose(ChoiceSpec {
            purpose: CONVERT_FORMAT,
            title: t!("convert-title", disk = label.as_str()),
            message,
            dismiss: formats.len(),
            default: Some(0),
            destructive: Vec::new(),
            choices,
        }));
        self.pending_convert = Some(Asked { key, formats });
    }

    /// The format question's answer.
    pub(crate) fn convert_answer(&mut self, a: &Answer, fx: &mut Vec<Effect>) -> bool {
        let Answer::Chose { purpose: CONVERT_FORMAT, index } = a else { return false };
        let Some(asked) = self.pending_convert.take() else { return true };
        let Some(&format) = asked.formats.get(*index) else { return true };
        self.start_convert(asked.key, format, fx);
        true
    }

    fn start_convert(&mut self, key: u8, format: ChdFormat, fx: &mut Vec<Effect>) {
        if let Enabled::No(why) = self.convert_enabled() {
            fx.push(Effect::Toast(why));
            return;
        }
        let Some((label, path, h)) = self.chd_disk(key) else { return };
        // The browser holds the image open; it must not see the file change.
        if !self.close_browser(fx) {
            return;
        }
        let need = chd_convert::room_needed(&h, format);
        if let Some(free) = chd_convert::free_bytes(std::path::Path::new(&path)) {
            if free < need {
                let human = super::disk_browser::human;
                fx.push(Effect::Toast(t!("convert-no-room", need = human(need), free = human(free))));
                return;
            }
        }
        let job_path = PathBuf::from(&path);
        let job = Job::spawn(t!("convert-running", disk = label.as_str()), true, self.notify(), move |ctx| {
            let mut progress = |f: f32| {
                ctx.progress(super::Progress::Determinate { done: (f * 1000.0) as u64, total: 1000, unit: super::Unit::Items })
            };
            chd_convert::convert(&job_path, format, &mut progress, &|| ctx.cancelled()).map_err(|e| e.to_string())
        });
        self.conversion = Some(Conversion { path, label, format, before: h.file_bytes, job });
    }

    pub(crate) fn cancel_convert(&mut self) {
        if let Some(c) = &self.conversion {
            c.job.cancel();
        }
    }

    /// A finished conversion's message; called every tick.
    pub(crate) fn convert_tick(&mut self, fx: &mut Vec<Effect>) {
        let Some(c) = &mut self.conversion else { return };
        let snap = c.job.snapshot();
        if snap.is_running() {
            return;
        }
        let cancelled = snap.cancel_requested;
        let result = c.job.outcome();
        let Some(c) = self.conversion.take() else { return };
        self.probe.forget(&c.path);
        let human = super::disk_browser::human;
        fx.push(Effect::Toast(match result {
            Some(Ok(size)) => {
                let format = match c.format {
                    ChdFormat::Zlib => t!("convert-format-zlib"),
                    ChdFormat::Uncompressed => t!("convert-format-uncompressed"),
                };
                t!("convert-done", disk = c.label.as_str(), format = format, size = human(size), before = human(c.before))
            }
            _ if cancelled => t!("convert-cancelled", disk = c.label.as_str()),
            Some(Err(e)) => t!("convert-failed", disk = c.label.as_str(), error = e),
            None => t!("convert-failed", disk = c.label.as_str(), error = "?"),
        }));
    }
}

/// The convert actions' ids map here from `commands.rs`.
pub(crate) fn action_for(id: &str) -> Option<AppAction> {
    if id == CANCEL_ID {
        return Some(AppAction::CancelConversion);
    }
    let key = id.strip_prefix(super::ids::item::CONVERT_DISK)?.parse().ok()?;
    Some(AppAction::ConvertDisk { key })
}
