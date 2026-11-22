//! The new-machine dialog: what a new machine starts with and the rules that
//! keep the choices consistent (a profile change resets the CPU, graphics,
//! RAM and the name that followed the old model).

use iris::config::{FibreChannelConfig, GraphicsBoard, MachineConfig, MachineProfile, ScsiDeviceConfig, MAX_BANKS};
use iris::cpu::parts::Part;
use iris::dev::ng1::vc2_timings::NewportResolution;

use super::settings::{Field, Value};
use super::{Answer, Effect, Enabled, PickSpec, Severity};
use crate::model::hardware::{default_name, distribute_ram};
use crate::model::{machines, ram};

/// The choices in the dialog. Plain data: a shell edits it, then asks
/// [`NewMachineSpec::build`] for the config.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMachineSpec {
    pub name: String,
    pub profile: MachineProfile,
    pub cpu: &'static Part,
    pub graphics: GraphicsBoard,
    pub resolution: NewportResolution,
    pub use_embedded_prom: bool,
    pub prom_path: String,
    pub nvram_path: String,
    pub nveeprom_path: String,
    pub ram_total_mb: u32,
    /// Set each bank instead of a total.
    pub ram_advanced: bool,
    pub ram_banks: [u32; MAX_BANKS],
    /// The first disk's image; empty for none.
    pub disk_path: String,
    pub cdrom_path: String,
    pub attach_cdrom: bool,
}

impl Default for NewMachineSpec {
    fn default() -> Self {
        let profile = MachineProfile::default();
        let mut s = Self {
            name: String::new(),
            profile,
            cpu: profile.cpus()[0],
            graphics: GraphicsBoard::Newport,
            resolution: NewportResolution::default(),
            use_embedded_prom: true,
            prom_path: "prom.bin".into(),
            nvram_path: crate::model::settings::GuiSettings::default_nvram_path(),
            nveeprom_path: crate::model::settings::GuiSettings::default_nveeprom_path(),
            ram_total_mb: 256,
            ram_advanced: false,
            ram_banks: iris::config::banks_from4([128, 128, 0, 0]),
            disk_path: crate::model::settings::GuiSettings::default_disk_path(1),
            cdrom_path: String::new(),
            attach_cdrom: false,
        };
        s.name = default_name(profile).into();
        s
    }
}

impl NewMachineSpec {
    /// Change the profile and refit what depends on it. A name the user has
    /// not changed follows the model.
    pub fn set_profile(&mut self, p: MachineProfile) {
        if p == self.profile {
            return;
        }
        let name = self.name.trim();
        if name.is_empty() || name == default_name(self.profile) {
            self.name = default_name(p).into();
        }
        self.profile = p;
        self.cpu = p.cpus()[0];
        self.graphics = if p.ip28() { GraphicsBoard::SolidImpact } else { GraphicsBoard::default_for(p) };
        self.use_embedded_prom = true;
        self.ram_total_mb = ram::ram_presets(p)[0];
        self.ram_banks = distribute_ram(self.ram_total_mb, p);
        self.resolution = NewportResolution::Guest;
    }

    /// The name the machine will get (the model's when left empty).
    pub fn effective_name(&self) -> &str {
        let n = self.name.trim();
        if n.is_empty() {
            default_name(self.profile)
        } else {
            n
        }
    }

    /// Why the name cannot be used, if it cannot.
    pub fn name_error(&self) -> Option<String> {
        machines::validate_name(self.effective_name()).err()
    }

    /// The RAM totals this profile offers.
    pub fn ram_presets(&self) -> &'static [u32] {
        ram::ram_presets(self.profile)
    }

    /// The config to create.
    pub fn build(&self) -> MachineConfig {
        let p = self.profile;
        let mut cfg = MachineConfig::default();
        cfg.machine.profile = p;
        cfg.set_cpu(self.cpu);
        cfg.banks_set = true;
        cfg.graphics.board = Some(self.graphics);
        cfg.graphics.resolution =
            if self.graphics == GraphicsBoard::Newport { self.resolution } else { NewportResolution::Guest };
        // "(embedded)" names the PROM built into iris.
        cfg.prom = if self.use_embedded_prom || self.prom_path.is_empty() {
            "(embedded)".into()
        } else {
            self.prom_path.clone()
        };
        cfg.nvram = self.nvram_path.clone();
        cfg.nveeprom = self.nveeprom_path.clone();
        cfg.banks = if self.ram_advanced { self.ram_banks } else { distribute_ram(self.ram_total_mb, p) };
        // Keep the Octane PROM's flash records between runs.
        if p == MachineProfile::OctaneIp30 {
            cfg.octane.flash = Some("flash-ip30.bin".into());
        }
        if p.is_ip35_bricks() {
            if !self.disk_path.is_empty() {
                let mut fc = FibreChannelConfig::default();
                fc.disks.insert(1, ScsiDeviceConfig { path: self.disk_path.clone(), ..Default::default() });
                cfg.fibre_channel = Some(fc);
            }
        } else {
            // Only what the user asked for: no default targets.
            cfg.scsi.clear();
            if !self.disk_path.is_empty() {
                cfg.scsi.insert(1, ScsiDeviceConfig { path: self.disk_path.clone(), ..Default::default() });
            }
            if self.attach_cdrom && !self.cdrom_path.is_empty() {
                cfg.scsi.insert(4, ScsiDeviceConfig { path: self.cdrom_path.clone(), cdrom: true, ..Default::default() });
            }
        }
        cfg
    }
}

/// Field ids of the new-machine form ([`NewMachineSpec::fields`]).
pub mod field {
    pub const NAME: &str = "new-machine.name";
    pub const MODEL: &str = "new-machine.model";
    pub const CPU: &str = "new-machine.cpu";
    pub const GRAPHICS: &str = "new-machine.graphics";
    pub const RAM: &str = "new-machine.ram";
    pub const DISK: &str = "new-machine.disk";
    pub const ATTACH_CDROM: &str = "new-machine.attach-cdrom";
    pub const CDROM: &str = "new-machine.cdrom";
    pub const NOTE: &str = "new-machine.note";
    /// The Create button.
    pub const CREATE: &str = "new-machine.create";
}

/// Purposes of the image pickers' answers.
pub const PICK_DISK: &str = "app.new-machine.disk";
pub const PICK_CDROM: &str = "app.new-machine.cdrom";

impl NewMachineSpec {
    /// The form as fields, the same kinds the settings use, so a shell draws
    /// it with its settings renderer. Path fields browse with
    /// [`NewMachineSpec::pick`].
    pub fn fields(&self) -> Vec<Field> {
        use super::settings::{FieldKind as K, PickKind};
        let p = self.profile;
        let one = |n: usize, why: String| Enabled::when(n > 1, || why);
        let cpus = p.cpus();
        let boards: Vec<GraphicsBoard> =
            GraphicsBoard::ALL.iter().copied().filter(|b| crate::model::hardware::board_supported(*b, p)).collect();
        let presets = self.ram_presets();
        let mut f = vec![
            Field {
                id: field::NAME,
                label: t!("dialog-new-machine-name"),
                help: None,
                kind: K::Text { value: self.name.clone(), placeholder: default_name(p).into(), error: self.name_error() },
                enabled: Enabled::Yes,
            },
            Field {
                id: field::MODEL,
                label: t!("dialog-new-machine-model"),
                help: None,
                kind: K::Choice {
                    options: MachineProfile::ALL.iter().map(|m| m.label().to_string()).collect(),
                    selected: MachineProfile::ALL.iter().position(|m| *m == p).unwrap_or(0),
                },
                enabled: Enabled::Yes,
            },
            Field {
                id: field::CPU,
                label: t!("dialog-new-machine-cpu"),
                help: None,
                kind: K::Choice {
                    options: cpus.iter().map(|c| c.label().to_string()).collect(),
                    selected: cpus.iter().position(|c| std::ptr::eq(*c, self.cpu)).unwrap_or(0),
                },
                enabled: one(cpus.len(), t!("settings-one-cpu")),
            },
        ];
        // The O2's display is built in: no board to choose.
        if p != MachineProfile::O2Ip32 {
            f.push(Field {
                id: field::GRAPHICS,
                label: t!("config-graphics-board"),
                help: None,
                kind: K::Choice {
                    options: boards.iter().map(|b| b.label().to_string()).collect(),
                    selected: boards.iter().position(|b| *b == self.graphics).unwrap_or(0),
                },
                enabled: one(boards.len(), t!("settings-one-board")),
            });
        }
        f.extend([
            Field {
                id: field::RAM,
                label: t!("dialog-new-machine-ram-total"),
                help: None,
                kind: K::Choice {
                    options: presets.iter().map(|mb| t!("dialog-new-machine-mb", size = *mb)).collect(),
                    selected: presets.iter().position(|mb| *mb == self.ram_total_mb).unwrap_or(0),
                },
                enabled: Enabled::Yes,
            },
            Field {
                id: field::DISK,
                label: t!("dialog-new-machine-hdd"),
                help: None,
                kind: K::Path { value: self.disk_path.clone(), pick: PickKind::File { filters: Vec::new() } },
                enabled: Enabled::Yes,
            },
        ]);
        if !p.is_ip35_bricks() {
            f.push(Field {
                id: field::ATTACH_CDROM,
                label: t!("dialog-new-machine-cdrom-attach"),
                help: None,
                kind: K::Toggle(self.attach_cdrom),
                enabled: Enabled::Yes,
            });
            if self.attach_cdrom {
                f.push(Field {
                    id: field::CDROM,
                    label: t!("dialog-new-machine-cdrom"),
                    help: None,
                    kind: K::Path { value: self.cdrom_path.clone(), pick: PickKind::File { filters: Vec::new() } },
                    enabled: Enabled::Yes,
                });
            }
        }
        let note = match p {
            MachineProfile::OctaneIp30 => Some(t!("dialog-new-machine-octane-note")),
            MachineProfile::O2Ip32 => Some(t!("dialog-new-machine-o2-note")),
            _ if p.ip28() => Some(t!("dialog-new-machine-ip28-note")),
            _ => None,
        };
        if let Some(n) = note {
            f.push(Field { id: field::NOTE, label: String::new(), help: None, kind: K::Note(Severity::Info, n), enabled: Enabled::Yes });
        }
        f
    }

    /// Apply a form control's new value. `false` when it changed nothing.
    pub fn apply(&mut self, id: &str, value: Value) -> bool {
        let before = self.clone();
        match (id, value) {
            (field::NAME, Value::Text(t)) => self.name = t,
            (field::MODEL, Value::Index(i)) => {
                if let Some(p) = MachineProfile::ALL.get(i) {
                    self.set_profile(*p);
                }
            }
            (field::CPU, Value::Index(i)) => {
                if let Some(c) = self.profile.cpus().get(i) {
                    self.cpu = c;
                }
            }
            (field::GRAPHICS, Value::Index(i)) => {
                let p = self.profile;
                if let Some(b) = GraphicsBoard::ALL
                    .iter()
                    .copied()
                    .filter(|b| crate::model::hardware::board_supported(*b, p))
                    .nth(i)
                {
                    self.graphics = b;
                }
            }
            (field::RAM, Value::Index(i)) => {
                if let Some(mb) = self.ram_presets().get(i) {
                    self.ram_total_mb = *mb;
                    self.ram_banks = distribute_ram(*mb, self.profile);
                }
            }
            (field::DISK, Value::Text(t)) => self.disk_path = t.trim().to_string(),
            (field::ATTACH_CDROM, Value::Bool(b)) => self.attach_cdrom = b,
            (field::CDROM, Value::Text(t)) => self.cdrom_path = t.trim().to_string(),
            _ => {}
        }
        *self != before
    }

    /// The picker a path field's Browse button opens; its answer comes back
    /// as `Answer::Picked` and goes to [`NewMachineSpec::picked`].
    pub fn pick(&self, id: &str) -> Option<Effect> {
        let (purpose, title, filters) = match id {
            field::DISK => (PICK_DISK, t!("dialog-new-machine-hdd-picker"), vec![(t!("dialog-new-machine-hdd-filter"), vec!["img".into(), "raw".into(), "chd".into(), "hda".into()])]),
            field::CDROM => (PICK_CDROM, t!("dialog-new-machine-cdrom-picker"), vec![(t!("dialog-new-machine-iso-filter"), vec!["iso".into(), "img".into(), "bin".into(), "toast".into(), "cdr".into(), "chd".into()])]),
            _ => return None,
        };
        Some(Effect::PickFile(PickSpec {
            purpose,
            title,
            filters,
            save: false,
            file_name: String::new(),
            start_dir: None,
            multiple: false,
        }))
    }

    /// A picker's answer: set the field it was for.
    pub fn picked(&mut self, answer: &Answer) -> bool {
        let Answer::Picked { purpose, paths } = answer else { return false };
        let id = match *purpose {
            PICK_DISK => field::DISK,
            PICK_CDROM => field::CDROM,
            _ => return false,
        };
        let Some(path) = paths.first() else { return false };
        self.apply(id, Value::Text(path.to_string_lossy().into_owned()))
    }

    /// Why Create is not available now, if it is not.
    pub fn can_create(&self) -> Enabled {
        match self.name_error() {
            Some(e) => Enabled::No(e),
            None => Enabled::Yes,
        }
    }

    /// The action that creates the machine.
    pub fn create(&self) -> super::app::AppAction {
        super::app::AppAction::CreateMachine { name: self.effective_name().to_string(), config: Box::new(self.build()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_edits_the_spec() {
        let mut s = NewMachineSpec::default();
        let fields = s.fields();
        assert!(fields.iter().any(|f| f.id == field::NAME));
        let o2 = MachineProfile::ALL.iter().position(|m| *m == MachineProfile::O2Ip32).unwrap();
        assert!(s.apply(field::MODEL, Value::Index(o2)));
        assert_eq!(s.profile, MachineProfile::O2Ip32);
        assert!(s.fields().iter().any(|f| f.id == field::NOTE));
        assert!(!s.fields().iter().any(|f| f.id == field::GRAPHICS), "the O2's display is built in");
        assert!(!s.apply(field::MODEL, Value::Index(o2)), "no change");
        assert!(s.apply(field::ATTACH_CDROM, Value::Bool(true)));
        assert!(s.fields().iter().any(|f| f.id == field::CDROM));
        let picked = Answer::Picked { purpose: PICK_CDROM, paths: vec!["/x/irix.iso".into()] };
        assert!(s.picked(&picked));
        assert_eq!(s.cdrom_path, "/x/irix.iso");
        assert!(matches!(s.pick(field::DISK), Some(Effect::PickFile(_))));
        s.apply(field::NAME, Value::Text("../bad".into()));
        assert!(matches!(s.can_create(), Enabled::No(_)));
        s.apply(field::NAME, Value::Text("lab".into()));
        assert_eq!(s.can_create(), Enabled::Yes);
        s.build().validate().unwrap();
    }

    #[test]
    fn every_profile_builds_a_valid_machine() {
        for p in MachineProfile::ALL {
            let mut s = NewMachineSpec::default();
            s.set_profile(p);
            assert_eq!(s.effective_name(), default_name(p));
            s.build().validate().unwrap_or_else(|e| panic!("{p:?}: {e}"));
        }
    }

    #[test]
    fn a_typed_name_survives_a_profile_change() {
        let mut s = NewMachineSpec { name: "lab".into(), ..Default::default() };
        s.set_profile(MachineProfile::O2Ip32);
        assert_eq!(s.name, "lab");
        assert!(s.name_error().is_none());
        s.name = "../x".into();
        assert!(s.name_error().is_some());
    }
}
