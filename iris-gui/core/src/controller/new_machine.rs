//! The new-machine dialog: what a new machine starts with and the rules that
//! keep the choices consistent (a profile change resets the CPU, graphics,
//! RAM and the name that followed the old model).

use iris::config::{FibreChannelConfig, GraphicsBoard, MachineConfig, MachineProfile, ScsiDeviceConfig, MAX_BANKS};
use iris::cpu::parts::Part;
use iris::dev::ng1::vc2_timings::NewportResolution;

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

#[cfg(test)]
mod tests {
    use super::*;

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
