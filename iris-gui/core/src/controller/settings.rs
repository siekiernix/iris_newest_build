//! Machine settings as data (D8): sections of fields, built from the core's
//! hardware tables and the [`profile`](super::profile) traits row, so a shell
//! implements each field kind once and a new machine needs no shell code.
//!
//! The schema covers the settings people change most (machine, memory,
//! display, disks, network). Every other key is reachable through the
//! complete-config editor (the Advanced section), which takes the whole TOML,
//! validates it and replaces the config.

use std::path::PathBuf;

use iris::config::{
    ForwardBind, ForwardProto, GraphicsBoard, MachineConfig, MachineProfile, NetMode, PortForwardConfig,
    ScsiDeviceConfig, ScsiKind, VALID_BANK_SIZES,
};
use iris::dev::ng1::vc2_timings::NewportResolution;

use super::icons::Icon;
use super::profile::traits;
use super::{Answer, Effect, Enabled, PickSpec, Severity};
use crate::model::hardware;

/// Field ids (stable; also accessibility identifiers).
pub mod field {
    pub const PROFILE: &str = "settings.machine.profile";
    pub const CPU: &str = "settings.machine.cpu";
    pub const CPUS: &str = "settings.machine.cpus";
    pub const PROM: &str = "settings.machine.prom";
    pub const BUILTIN_PROM: &str = "settings.machine.builtin-prom";
    pub const NVRAM: &str = "settings.machine.nvram";
    pub const NVEEPROM: &str = "settings.machine.nveeprom";
    pub const BANKS: &str = "settings.memory.banks";
    pub const BOARD: &str = "settings.display.board";
    pub const HEADS: &str = "settings.display.heads";
    pub const RESOLUTION: &str = "settings.display.resolution";
    pub const DISKS: &str = "settings.disks.table";
    pub const NET_MODE: &str = "settings.network.mode";
    pub const NAT_SUBNET: &str = "settings.network.nat-subnet";
    pub const PCAP_INTERFACE: &str = "settings.network.pcap-interface";
    pub const FORWARDS: &str = "settings.network.forwards";
    pub const TOML: &str = "settings.advanced.toml";
    pub const NOTE: &str = "settings.note";
}

/// What a file field picks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickKind {
    File { filters: Vec<(String, Vec<String>)> },
    Folder,
}

/// One SCSI target row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskRow {
    pub key: u8,
    /// "3" or "1:3".
    pub name: String,
    pub kind: ScsiKind,
    pub path: String,
    pub overlay: bool,
    pub scratch: bool,
    /// Missing or unreadable image, said in words.
    pub problem: Option<String>,
}

/// One port forward row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardRow {
    pub tcp: bool,
    pub host_port: u16,
    pub guest_port: u16,
    pub any_interface: bool,
}

/// How a field is edited. A shell draws each kind once.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    Choice { options: Vec<String>, selected: usize },
    Toggle(bool),
    Text { value: String, placeholder: String, error: Option<String> },
    Number { value: f64, min: f64, max: f64, step: f64, unit: String },
    Path { value: String, pick: PickKind },
    /// Memory banks: each bank's size in MB, and the sizes a bank takes.
    BankGrid { banks: Vec<u32>, sizes: Vec<u32>, summary: String },
    DiskTable(Vec<DiskRow>),
    PortForwardTable(Vec<ForwardRow>),
    /// The whole config as TOML.
    Toml { text: String },
    Note(Severity, String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub id: &'static str,
    pub label: String,
    pub help: Option<String>,
    pub kind: FieldKind,
    pub enabled: Enabled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub id: &'static str,
    pub title: String,
    /// One line for the device list on the left of the sheet.
    pub summary: String,
    pub icon: Icon,
    pub fields: Vec<Field>,
}

/// A value for [`SettingsAction::Set`].
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Index(usize),
    Bool(bool),
    Text(String),
    Number(f64),
}

/// Disk table edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskEdit {
    /// Add a target of `kind` at the first free id on controller 0.
    Add(ScsiKind),
    Remove(u8),
    SetPath { key: u8, path: String },
    /// Ask for an image file.
    Pick(u8),
    SetKind { key: u8, kind: ScsiKind },
    SetOverlay { key: u8, on: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardEdit {
    Add,
    Remove(usize),
    Set(usize, ForwardRow),
}

/// Settings intents. Each changes the selected machine's in-memory config;
/// the app marks it dirty and autosaves it.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsAction {
    Set { field: &'static str, value: Value },
    /// Ask for a file or folder for a path field.
    Pick(&'static str),
    /// Set memory bank `index` to `size` MB.
    Bank { index: usize, size: u32 },
    Disk(DiskEdit),
    Forward(ForwardEdit),
    /// Replace the whole config with this TOML, if it is valid.
    ApplyToml(String),
    Answer(Answer),
}

pub(crate) const PICK: &str = "settings.pick";

/// Remembers which field a picker is for.
#[derive(Debug, Default)]
pub struct SettingsController {
    picking: Option<(&'static str, Option<u8>)>,
    toml_error: Option<String>,
}

fn disk_filters() -> Vec<(String, Vec<String>)> {
    vec![(t!("settings-filter-disk-images"), ["img", "raw", "chd", "hda", "iso", "bin", "toast", "cdr"].map(String::from).to_vec())]
}

fn any_file() -> PickKind {
    PickKind::File { filters: Vec::new() }
}

impl SettingsController {
    /// The sections for `cfg`. `running` disables what only applies at power on.
    pub fn sections(&self, cfg: &MachineConfig, running: bool) -> Vec<Section> {
        let p = cfg.machine.profile;
        let t = traits(p);
        let at_power_on = || Enabled::when(!running, || t!("settings-next-power-on"));
        let mut out = Vec::new();

        // Machine.
        let mut f = vec![Field {
            id: field::PROFILE,
            label: t!("settings-machine-type"),
            help: None,
            kind: FieldKind::Choice {
                options: MachineProfile::ALL.iter().map(|p| p.label().to_string()).collect(),
                selected: MachineProfile::ALL.iter().position(|q| *q == p).unwrap_or(0),
            },
            enabled: at_power_on(),
        }];
        let cpus = p.cpus();
        f.push(Field {
            id: field::CPU,
            label: t!("config-cpu"),
            help: None,
            kind: FieldKind::Choice {
                options: cpus.iter().map(|c| c.label().to_string()).collect(),
                selected: cpus.iter().position(|c| *c == cfg.cpu()).unwrap_or(0),
            },
            enabled: if cpus.len() < 2 { Enabled::No(t!("settings-one-cpu")) } else { at_power_on() },
        });
        if p.max_cpus() > 1 {
            f.push(Field {
                id: field::CPUS,
                label: t!("settings-cpu-count"),
                help: None,
                kind: FieldKind::Number {
                    value: cfg.machine.cpus() as f64,
                    min: 1.0,
                    max: p.max_cpus() as f64,
                    step: 1.0,
                    unit: String::new(),
                },
                enabled: at_power_on(),
            });
        }
        let builtin = hardware::builtin_prom(cfg);
        f.push(Field {
            id: field::BUILTIN_PROM,
            label: t!("config-prom-builtin"),
            help: None,
            kind: FieldKind::Toggle(builtin),
            enabled: at_power_on(),
        });
        if !builtin {
            f.push(Field {
                id: field::PROM,
                label: t!("config-prom"),
                help: None,
                kind: FieldKind::Path { value: cfg.prom.clone(), pick: any_file() },
                enabled: at_power_on(),
            });
        }
        f.push(Field {
            id: field::NVRAM,
            label: t!("config-nvram"),
            help: None,
            kind: FieldKind::Path { value: cfg.nvram.clone(), pick: any_file() },
            enabled: at_power_on(),
        });
        if hardware::has_nveeprom(p) {
            f.push(Field {
                id: field::NVEEPROM,
                label: t!("config-nveeprom"),
                help: Some(t!("config-nveeprom-tip")),
                kind: FieldKind::Path { value: cfg.nveeprom.clone(), pick: any_file() },
                enabled: at_power_on(),
            });
        }
        out.push(Section {
            id: super::ids::settings::GENERAL,
            title: t!("settings-section-machine"),
            summary: format!("{}, {}", t.model, cfg.cpu().label()),
            icon: Icon::MachineSettings,
            fields: f,
        });

        // Memory.
        let banks: Vec<u32> = cfg.banks.iter().take(p.memory_banks()).copied().collect();
        let total: u32 = banks.iter().sum();
        out.push(Section {
            id: "settings.memory",
            title: t!("settings-section-memory"),
            summary: t!("settings-memory-summary", mb = total),
            icon: Icon::Info,
            fields: vec![Field {
                id: field::BANKS,
                label: t!("settings-memory-banks"),
                help: Some(crate::model::ram::bank_size_tip(p)),
                kind: FieldKind::BankGrid {
                    banks,
                    sizes: VALID_BANK_SIZES.iter().copied().filter(|s| *s == 0 || p.bank_sizes().contains(s)).collect(),
                    summary: crate::model::ram::ram_summary(&cfg.banks),
                },
                enabled: at_power_on(),
            }],
        });

        // Display.
        let mut f = Vec::new();
        let boards: Vec<GraphicsBoard> =
            GraphicsBoard::ALL.iter().copied().filter(|b| hardware::board_supported(*b, p)).collect();
        let board = cfg.graphics_board();
        if p == MachineProfile::O2Ip32 {
            f.push(note(Severity::Info, t!("settings-o2-display")));
        } else {
            f.push(Field {
                id: field::BOARD,
                label: t!("config-graphics-board"),
                help: None,
                kind: FieldKind::Choice {
                    options: boards.iter().map(|b| b.label().to_string()).collect(),
                    selected: boards.iter().position(|b| *b == board).unwrap_or(0),
                },
                enabled: if boards.len() < 2 { Enabled::No(t!("settings-one-board")) } else { at_power_on() },
            });
        }
        let newport = hardware::mc_machine(p) && board == GraphicsBoard::Newport && !cfg.headless;
        if newport {
            f.push(Field {
                id: field::RESOLUTION,
                label: t!("config-resolution"),
                help: None,
                kind: FieldKind::Choice {
                    options: NewportResolution::ALL.iter().map(|r| r.label().to_string()).collect(),
                    selected: NewportResolution::ALL.iter().position(|r| *r == cfg.graphics.resolution).unwrap_or(0),
                },
                enabled: at_power_on(),
            });
            if matches!(p, MachineProfile::Indigo2Ip22) {
                f.push(Field {
                    id: field::HEADS,
                    label: t!("settings-heads"),
                    help: None,
                    kind: FieldKind::Choice { options: vec!["1".into(), "2".into()], selected: (cfg.graphics.heads.clamp(1, 2) - 1) as usize },
                    enabled: at_power_on(),
                });
            }
        }
        out.push(Section {
            id: super::ids::settings::DISPLAY,
            title: t!("settings-section-display"),
            summary: if p == MachineProfile::O2Ip32 { "GBE".into() } else { board.label().to_string() },
            icon: Icon::DisplayScale,
            fields: f,
        });

        // Disks.
        if !p.is_ip35_bricks() {
            let rows: Vec<DiskRow> = cfg
                .scsi
                .iter()
                .map(|(&key, d)| DiskRow {
                    key,
                    name: hardware::scsi_name(cfg, key),
                    kind: d.kind(),
                    path: d.path.clone(),
                    overlay: d.overlay,
                    scratch: d.scratch,
                    problem: (d.kind() == ScsiKind::Disk && !d.scratch && !crate::model::machines::readable(&d.path))
                        .then(|| t!("settings-disk-missing")),
                })
                .collect();
            let n = rows.iter().filter(|r| r.kind == ScsiKind::Disk).count();
            out.push(Section {
                id: super::ids::settings::DISKS,
                title: t!("settings-section-disks"),
                summary: t!("settings-disks-summary", count = n),
                icon: Icon::Disk,
                fields: vec![Field {
                    id: field::DISKS,
                    label: t!("config-scsi"),
                    help: None,
                    kind: FieldKind::DiskTable(rows),
                    enabled: Enabled::Yes,
                }],
            });
        }

        // Network.
        let modes = net_modes();
        let mode = cfg.network.mode;
        let mut f = vec![Field {
            id: field::NET_MODE,
            label: t!("config-net-mode"),
            help: None,
            kind: FieldKind::Choice {
                options: modes.iter().map(|m| net_mode_label(*m)).collect(),
                selected: modes.iter().position(|m| *m == mode).unwrap_or(0),
            },
            enabled: at_power_on(),
        }];
        match mode {
            NetMode::Nat => {
                let value = cfg.nat_subnet.clone().unwrap_or_default();
                let error = (!value.is_empty())
                    .then(|| iris::config::parse_nat_subnet(&value).err().map(|e| e.to_string()))
                    .flatten();
                f.push(Field {
                    id: field::NAT_SUBNET,
                    label: t!("config-net-network"),
                    help: None,
                    kind: FieldKind::Text { value, placeholder: "192.168.0.0/24".into(), error },
                    enabled: Enabled::Yes,
                });
                f.push(Field {
                    id: field::FORWARDS,
                    label: t!("config-forwards"),
                    help: None,
                    kind: FieldKind::PortForwardTable(
                        cfg.port_forward
                            .iter()
                            .map(|r| ForwardRow {
                                tcp: r.proto == ForwardProto::Tcp,
                                host_port: r.host_port,
                                guest_port: r.guest_port,
                                any_interface: r.bind == ForwardBind::Any,
                            })
                            .collect(),
                    ),
                    enabled: Enabled::Yes,
                });
            }
            NetMode::Pcap => {
                f.push(Field {
                    id: field::PCAP_INTERFACE,
                    label: t!("settings-pcap-interface"),
                    help: Some(t!("settings-pcap-interface-help")),
                    kind: FieldKind::Text {
                        value: cfg.network.pcap_interface.clone().unwrap_or_default(),
                        placeholder: t!("common-auto"),
                        error: None,
                    },
                    enabled: Enabled::Yes,
                });
                if !iris::build_features::PCAP {
                    f.push(note(Severity::Warning, t!("settings-pcap-not-built")));
                }
            }
            NetMode::Off | NetMode::Stream => {}
        }
        out.push(Section {
            id: super::ids::settings::NETWORK,
            title: t!("settings-section-network"),
            summary: net_mode_label(mode),
            icon: Icon::Network,
            fields: f,
        });

        // Advanced: every key, as TOML.
        out.push(Section {
            id: "settings.advanced",
            title: t!("settings-section-advanced"),
            summary: t!("settings-advanced-summary"),
            icon: Icon::Edit,
            fields: vec![
                Field {
                    id: field::TOML,
                    label: t!("settings-toml"),
                    help: Some(t!("settings-toml-help")),
                    kind: FieldKind::Toml { text: crate::model::machines::to_toml(cfg).unwrap_or_default() },
                    enabled: at_power_on(),
                },
            ]
            .into_iter()
            .chain(self.toml_error.clone().map(|e| note(Severity::Error, e)))
            .collect(),
        });
        out
    }

    /// Apply `action` to `cfg`. Returns whether the config changed, and the
    /// effects to carry out.
    pub fn apply(&mut self, action: SettingsAction, cfg: &mut MachineConfig, dir: Option<PathBuf>) -> (bool, Vec<Effect>) {
        let mut fx = Vec::new();
        let changed = match action {
            SettingsAction::Set { field, value } => set(cfg, field, value),
            SettingsAction::Pick(field) => {
                let title = t!("common-browse");
                let spec = match field {
                    field::PROM | field::NVRAM | field::NVEEPROM => PickSpec::open(PICK, title).in_dir(dir),
                    _ => return (false, fx),
                };
                self.picking = Some((field, None));
                fx.push(Effect::PickFile(spec));
                false
            }
            SettingsAction::Bank { index, size } => {
                if index < cfg.machine.profile.memory_banks() && cfg.banks[index] != size {
                    cfg.banks[index] = size;
                    cfg.banks_set = true;
                    true
                } else {
                    false
                }
            }
            SettingsAction::Disk(edit) => match edit {
                DiskEdit::Add(kind) => {
                    let p = cfg.machine.profile;
                    let free = (0..=p.max_scsi_id()).find(|id| hardware::scsi_key(cfg, 0, *id).is_none() && *id != 7);
                    match free {
                        Some(id) => {
                            let mut dev = ScsiDeviceConfig { kind_field: kind, cdrom: kind == ScsiKind::Cdrom, ..Default::default() };
                            if kind == ScsiKind::Disk {
                                dev.path = crate::model::settings::GuiSettings::default_disk_path(id);
                            }
                            cfg.scsi.insert(id, dev);
                            true
                        }
                        None => {
                            fx.push(Effect::Toast(t!("settings-no-free-id")));
                            false
                        }
                    }
                }
                DiskEdit::Remove(key) => cfg.scsi.remove(&key).is_some(),
                DiskEdit::SetPath { key, path } => match cfg.scsi.get_mut(&key) {
                    Some(d) if d.path != path => {
                        d.path = path;
                        true
                    }
                    _ => false,
                },
                DiskEdit::Pick(key) => {
                    self.picking = Some((field::DISKS, Some(key)));
                    fx.push(Effect::PickFile(
                        PickSpec { filters: disk_filters(), ..PickSpec::open(PICK, t!("common-browse")) }
                            .in_dir(dir.map(|d| d.join("disks"))),
                    ));
                    false
                }
                DiskEdit::SetKind { key, kind } => match cfg.scsi.get_mut(&key) {
                    Some(d) if d.kind() != kind => {
                        d.kind_field = kind;
                        d.cdrom = kind == ScsiKind::Cdrom;
                        true
                    }
                    _ => false,
                },
                DiskEdit::SetOverlay { key, on } => match cfg.scsi.get_mut(&key) {
                    Some(d) if d.overlay != on => {
                        d.overlay = on;
                        true
                    }
                    _ => false,
                },
            },
            SettingsAction::Forward(edit) => {
                match edit {
                    ForwardEdit::Add => {
                        let host = (2222u16..).find(|p| !cfg.port_forward.iter().any(|r| r.host_port == *p)).unwrap_or(2222);
                        cfg.port_forward.push(PortForwardConfig {
                            proto: ForwardProto::Tcp,
                            host_port: host,
                            guest_port: 22,
                            bind: ForwardBind::default(),
                        });
                    }
                    ForwardEdit::Remove(i) => {
                        if i < cfg.port_forward.len() {
                            cfg.port_forward.remove(i);
                        }
                    }
                    ForwardEdit::Set(i, row) => {
                        if let Some(r) = cfg.port_forward.get_mut(i) {
                            r.proto = if row.tcp { ForwardProto::Tcp } else { ForwardProto::Udp };
                            r.host_port = row.host_port;
                            r.guest_port = row.guest_port;
                            r.bind = if row.any_interface { ForwardBind::Any } else { ForwardBind::Localhost };
                        }
                    }
                }
                true
            }
            SettingsAction::ApplyToml(text) => match hardware::parse_complete(&text) {
                Ok(new) => {
                    self.toml_error = None;
                    *cfg = new;
                    true
                }
                Err(e) => {
                    self.toml_error = Some(e);
                    false
                }
            },
            SettingsAction::Answer(Answer::Picked { purpose: PICK, paths }) => {
                let (Some((f, key)), Some(p)) = (self.picking.take(), paths.into_iter().next()) else {
                    return (false, fx);
                };
                let path = p.to_string_lossy().into_owned();
                match (f, key) {
                    (field::DISKS, Some(key)) => {
                        return self.apply(SettingsAction::Disk(DiskEdit::SetPath { key, path }), cfg, None)
                    }
                    (f, _) => set(cfg, f, Value::Text(path)),
                }
            }
            SettingsAction::Answer(_) => false,
        };
        (changed, fx)
    }

    /// The live command a change needs on a running machine, if any: the NAT
    /// subnet and port forwards change without a reboot.
    pub fn live_change(field: &'static str) -> bool {
        matches!(field, field::NAT_SUBNET | field::FORWARDS | field::PCAP_INTERFACE)
    }
}

fn note(severity: Severity, text: String) -> Field {
    Field { id: field::NOTE, label: String::new(), help: None, kind: FieldKind::Note(severity, text), enabled: Enabled::Yes }
}

fn net_modes() -> Vec<NetMode> {
    vec![NetMode::Nat, NetMode::Pcap, NetMode::Off]
}

fn net_mode_label(m: NetMode) -> String {
    match m {
        NetMode::Nat => t!("config-net-mode-nat"),
        NetMode::Pcap => t!("config-net-mode-pcap"),
        NetMode::Off => t!("settings-net-off"),
        NetMode::Stream => t!("status-net-stream"),
    }
}

fn set(cfg: &mut MachineConfig, field: &'static str, value: Value) -> bool {
    let p = cfg.machine.profile;
    match (field, value) {
        (field::PROFILE, Value::Index(i)) => {
            let Some(&new) = MachineProfile::ALL.get(i) else { return false };
            if new == p {
                return false;
            }
            cfg.machine.profile = new;
            // Refit only on an explicit change, never per frame.
            hardware::switch_profile(cfg);
            true
        }
        (field::CPU, Value::Index(i)) => match p.cpus().get(i) {
            Some(&c) if c != cfg.cpu() => {
                cfg.set_cpu(c);
                true
            }
            _ => false,
        },
        (field::CPUS, Value::Number(n)) => {
            let n = (n.round() as u8).clamp(1, p.max_cpus());
            if p.accepts_cpus(n).is_err() || cfg.machine.cpus() == n {
                return false;
            }
            cfg.machine.cpus = Some(n);
            true
        }
        (field::BUILTIN_PROM, Value::Bool(on)) => {
            if on {
                cfg.prom = "(embedded)".into();
            } else if hardware::builtin_prom(cfg) {
                cfg.prom = iris::config::DEFAULT_PROM.into();
            }
            true
        }
        (field::PROM, Value::Text(s)) => replace(&mut cfg.prom, s),
        (field::NVRAM, Value::Text(s)) => replace(&mut cfg.nvram, s),
        (field::NVEEPROM, Value::Text(s)) => replace(&mut cfg.nveeprom, s),
        (field::BOARD, Value::Index(i)) => {
            let boards: Vec<GraphicsBoard> =
                GraphicsBoard::ALL.iter().copied().filter(|b| hardware::board_supported(*b, p)).collect();
            match boards.get(i) {
                Some(&b) if b != cfg.graphics_board() => {
                    cfg.graphics.board = Some(b);
                    true
                }
                _ => false,
            }
        }
        (field::RESOLUTION, Value::Index(i)) => match NewportResolution::ALL.get(i) {
            Some(&r) if r != cfg.graphics.resolution => {
                cfg.graphics.resolution = r;
                true
            }
            _ => false,
        },
        (field::HEADS, Value::Index(i)) => {
            let h = (i as u8 + 1).clamp(1, 2);
            if h == cfg.graphics.heads {
                return false;
            }
            cfg.graphics.heads = h;
            true
        }
        (field::NET_MODE, Value::Index(i)) => match net_modes().get(i) {
            Some(&m) if m != cfg.network.mode => {
                cfg.network.mode = m;
                true
            }
            _ => false,
        },
        (field::NAT_SUBNET, Value::Text(s)) => {
            let v = (!s.trim().is_empty()).then(|| s.trim().to_string());
            if v == cfg.nat_subnet {
                return false;
            }
            cfg.nat_subnet = v;
            true
        }
        (field::PCAP_INTERFACE, Value::Text(s)) => {
            let v = (!s.trim().is_empty()).then(|| s.trim().to_string());
            if v == cfg.network.pcap_interface {
                return false;
            }
            cfg.network.pcap_interface = v;
            true
        }
        // A value of the wrong kind for the field, or a field with no setter.
        _ => false,
    }
}

fn replace(slot: &mut String, s: String) -> bool {
    if *slot == s {
        false
    } else {
        *slot = s;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section<'a>(s: &'a [Section], id: &str) -> &'a Section {
        s.iter().find(|x| x.id == id).unwrap()
    }

    #[test]
    fn every_profile_builds_a_schema_whose_choices_round_trip() {
        let c = SettingsController::default();
        for p in MachineProfile::ALL {
            let mut cfg = MachineConfig::default();
            cfg.machine.profile = p;
            hardware::switch_profile(&mut cfg);
            let sections = c.sections(&cfg, false);
            for s in &sections {
                for f in &s.fields {
                    if let FieldKind::Choice { selected, options } = &f.kind {
                        assert!(*selected < options.len(), "{p:?} {}", f.id);
                        // Choosing what is selected changes nothing.
                        let mut again = cfg.clone();
                        assert!(!set(&mut again, f.id, Value::Index(*selected)), "{p:?} {}", f.id);
                    }
                }
            }
            assert_eq!(p.is_ip35_bricks(), !sections.iter().any(|s| s.id == super::super::ids::settings::DISKS));
        }
    }

    #[test]
    fn switching_profile_refits_and_validates() {
        let mut c = SettingsController::default();
        let mut cfg = MachineConfig::default();
        let octane = MachineProfile::ALL.iter().position(|p| *p == MachineProfile::OctaneIp30).unwrap();
        let (changed, _) = c.apply(SettingsAction::Set { field: field::PROFILE, value: Value::Index(octane) }, &mut cfg, None);
        assert!(changed);
        assert_eq!(cfg.machine.profile, MachineProfile::OctaneIp30);
        cfg.validate().unwrap();
    }

    #[test]
    fn running_disables_power_on_fields_but_not_live_ones() {
        let c = SettingsController::default();
        let cfg = MachineConfig::default();
        let s = c.sections(&cfg, true);
        let machine = section(&s, super::super::ids::settings::GENERAL);
        assert!(!machine.fields[0].enabled.is_yes());
        let net = section(&s, super::super::ids::settings::NETWORK);
        let subnet = net.fields.iter().find(|f| f.id == field::NAT_SUBNET).unwrap();
        assert!(subnet.enabled.is_yes());
    }

    #[test]
    fn disks_add_remove_and_bad_toml_is_refused() {
        let mut c = SettingsController::default();
        let mut cfg = MachineConfig::default();
        cfg.scsi.clear();
        c.apply(SettingsAction::Disk(DiskEdit::Add(ScsiKind::Cdrom)), &mut cfg, None);
        assert!(cfg.scsi.values().any(|d| d.is_cdrom()));
        let key = *cfg.scsi.keys().next().unwrap();
        c.apply(SettingsAction::Disk(DiskEdit::Remove(key)), &mut cfg, None);
        assert!(cfg.scsi.is_empty());
        let (changed, _) = c.apply(SettingsAction::ApplyToml("made_up = true".into()), &mut cfg, None);
        assert!(!changed);
        let s = c.sections(&cfg, false);
        let adv = section(&s, "settings.advanced");
        assert!(adv.fields.iter().any(|f| matches!(f.kind, FieldKind::Note(Severity::Error, _))));
    }

    #[test]
    fn banks_only_take_the_profiles_sizes() {
        let c = SettingsController::default();
        let cfg = MachineConfig::default();
        let s = c.sections(&cfg, false);
        let FieldKind::BankGrid { sizes, banks, .. } = &section(&s, "settings.memory").fields[0].kind else { panic!() };
        assert_eq!(banks.len(), MachineProfile::IndyIp24.memory_banks());
        assert!(sizes.iter().all(|s| *s == 0 || MachineProfile::IndyIp24.bank_sizes().contains(s)));
    }
}
