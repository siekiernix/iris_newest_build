//! Pure facts about machine profiles and their configs that every shell needs:
//! which family a profile belongs to, how SCSI targets are named, how a config
//! is refitted when the profile changes. No UI.

use iris::config::*;

/// The Indy and Indigo2 family: the machines built around the MC memory
/// controller, with Newport resolution presets, a DS1386 NVRAM and (on the
/// Indigo2) a motherboard EEPROM.
pub fn mc_machine(profile: MachineProfile) -> bool {
    matches!(profile, MachineProfile::IndyIp24 | MachineProfile::Indigo2Ip22 | MachineProfile::Indigo2Ip28)
}

/// Whether `profile` has the Indigo2's motherboard EEPROM (`nveeprom`). The
/// Indy keeps its PROM environment in the NVRAM, the O2 in its flash.
pub fn has_nveeprom(profile: MachineProfile) -> bool {
    matches!(profile, MachineProfile::Indigo2Ip22 | MachineProfile::Indigo2Ip28)
}

/// Whether `cfg.prom` runs the PROM built into iris: `(embedded)`, an empty
/// value the GUI writes as `(embedded)` before a start, or the default name
/// with neither it nor the machine's release file name in the machine
/// folder (`prom_image::resolve`).
pub fn builtin_prom(cfg: &MachineConfig) -> bool {
    let prom = cfg.prom.as_str();
    let exists = |p: &str| std::path::Path::new(p).exists();
    prom.is_empty()
        || iris::dev::prom_image::is_embedded_keyword(prom)
        || (prom == DEFAULT_PROM
            && !exists(prom)
            && !cfg.machine.profile.release_prom_file().is_some_and(exists))
}

/// The `[scsi.KEY]` key of the target at (`controller`, `id`), whichever
/// spelling the config used: the bare id with `controller = 1`, or `16 + id`.
pub fn scsi_key(cfg: &MachineConfig, controller: u8, id: u8) -> Option<u8> {
    cfg.scsi.iter().find(|(k, d)| d.placement(**k) == (controller, id)).map(|(k, _)| *k)
}

/// How a `[scsi.KEY]` target is named to the user: its id on controller 0,
/// `controller:id` on another (so key 19 reads "1:3", not "19").
pub fn scsi_name(cfg: &MachineConfig, key: u8) -> String {
    let (controller, id) = cfg.scsi.get(&key).map_or((0, key), |d| d.placement(key));
    if controller == 0 { id.to_string() } else { format!("{controller}:{id}") }
}

/// Choices reflect the boards actually fitted by machine constructors. The O2
/// has a fixed GBE display; its shared board field is only a placeholder.
pub fn board_supported(board: GraphicsBoard, profile: MachineProfile) -> bool {
    board.supports(profile)
        && (profile != MachineProfile::O2Ip32 || board == GraphicsBoard::Newport)
        && (profile != MachineProfile::Onyx2Ip27 || board != GraphicsBoard::Ir3)
}

/// Refit hardware when the user explicitly changes profiles. Do not run this
/// each frame: it would erase intentional edits and imported configurations.
pub fn switch_profile(cfg: &mut MachineConfig) {
    let p = cfg.machine.profile;
    cfg.machine.cpu = None;
    cfg.machine.cpus = None;
    cfg.clock.fixed_mhz = None;
    cfg.banks = distribute_ram(crate::model::ram::ram_presets(p)[0], p);
    cfg.banks_set = true;
    cfg.graphics.board = Some(if p.ip28() {
        GraphicsBoard::SolidImpact
    } else {
        GraphicsBoard::default_for(p)
    });
    cfg.graphics.heads = 1;
    // `compile = "sync"` needs one CPU; the new machine may start with more.
    if cfg.machine.cpus() > 1 {
        cfg.jitv2.compile = JitCompile::Async;
    }
    cfg.graphics.resolution = Default::default();
    cfg.octane.graphics = None;
    // SN0 rejects another product's private table, even when it is inactive.
    cfg.origin200 = Default::default();
    cfg.origin2000 = Default::default();
    cfg.onyx2 = Default::default();
    if p.is_ip35_bricks() {
        // These machines use the I-brick's Fibre Channel adapter. The core
        // expects its historical default SCSI map to remain untouched.
        cfg.scsi = MachineConfig::default().scsi;
    } else {
        // Targets the new machine's buses cannot hold, which the Disks tab
        // would no longer show: a second controller, wide ids, DaynaPort.
        let (controllers, max_id) = (p.scsi_controllers(), p.max_scsi_id());
        let daynaport = matches!(p, MachineProfile::IndyIp24 | MachineProfile::Indigo2Ip22
            | MachineProfile::Indigo2Ip28 | MachineProfile::OctaneIp30);
        cfg.scsi.retain(|k, d| {
            let (c, id) = d.placement(*k);
            c < controllers && id <= max_id && (daynaport || !d.is_daynaport())
        });
    }
    // What `MachineConfig::validate` refuses on the new machine, and the
    // Advanced tab no longer shows for it.
    if !matches!(p, MachineProfile::FuelIp34 | MachineProfile::TezroIp53) {
        cfg.machine.emu_id = 0;
    }
    let octane = p == MachineProfile::OctaneIp30;
    if !octane {
        cfg.xio_board.clear();
    }
    if !(octane || p == MachineProfile::O2Ip32) {
        cfg.nvme = Default::default();
    } else if !octane {
        // The O2 has one slot: no option board to name.
        cfg.nvme.board = None;
        cfg.nvme.slot = None;
    }
    if !(octane || p.is_ip35_bricks()) {
        cfg.fibre_channel = None;
    } else if let (false, Some(fc)) = (octane, &mut cfg.fibre_channel) {
        fc.board = None;
        fc.slot = None;
        fc.bridge = false;
    }
    // On the Octane both cards sit on an `[[xio_board]]`; with none left
    // (an O2's NVMe, an IP35 loop) there is no slot for them.
    if octane && cfg.xio_board.is_empty() {
        cfg.nvme = Default::default();
        cfg.fibre_channel = None;
    }
    if !(mc_machine(p) || octane || p == MachineProfile::TezroIp53) {
        cfg.virtio_serial.enabled = false;
        cfg.virtio_9p.enabled = false;
    }
    if mc_machine(p) && cfg.network.mode == NetMode::Off {
        cfg.network.mode = NetMode::Nat;
    }
    // The console's own channel is logged by `serial_log`.
    if matches!(p, MachineProfile::OctaneIp30 | MachineProfile::O2Ip32) {
        cfg.serial_log_a = None;
    }
    if mc_machine(p) {
        cfg.serial_log_b = None;
    }
}

/// The name a new machine of `profile` gets until the user types one:
/// the profile's name up to its board number (`o2`, `octane`, `indigo2`).
pub fn default_name(profile: MachineProfile) -> &'static str {
    let name = profile.name();
    name.split_once('_').map_or(name, |(model, _)| model)
}

/// Fill the profile's banks largest first, without gaps.
pub fn distribute_ram(total: u32, profile: MachineProfile) -> [u32; iris::config::MAX_BANKS] {
    let mut remaining = total;
    let mut banks = [0u32; iris::config::MAX_BANKS];
    for slot in banks.iter_mut().take(profile.memory_banks()) {
        // Pick the largest size in VALID_BANK_SIZES that is <= remaining.
        let pick = VALID_BANK_SIZES.iter()
            .filter(|&&s| s > 0 && s <= remaining && profile.bank_sizes().contains(&s))
            .max().copied().unwrap_or(0);
        *slot = pick;
        remaining -= pick;
        if remaining == 0 { break; }
    }
    banks
}

/// Parse and validate the text of a complete machine config.
pub fn parse_complete(text: &str) -> Result<MachineConfig, String> {
    let cfg = MachineConfig::from_toml_str(text).map_err(|e| e.to_string())?;
    cfg.validate()?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_can_be_created_saved_and_switched_to() {
        assert_eq!(MachineProfile::ALL, MachineProfile::EVERY);
        for p in MachineProfile::ALL {
            let mut cfg = MachineConfig::default();
            cfg.machine.profile = p;
            switch_profile(&mut cfg);
            cfg.validate().unwrap_or_else(|e| panic!("{p:?}: {e}"));
            let loaded = parse_complete(&toml::to_string(&cfg).unwrap()).unwrap();
            assert_eq!(loaded.machine.profile, p);
            assert_eq!(loaded.banks, cfg.banks);
            // Changing away from an SN0 table must not leave a foreign table.
            cfg.origin200.flash = Some("other-flash.bin".into());
            switch_profile(&mut cfg);
            cfg.validate()
                .unwrap_or_else(|e| panic!("switch to {p:?}: {e}"));
        }
    }

    /// Switching away from a machine with expansion, extra disks and an
    /// identity leaves nothing the new machine refuses.
    #[test]
    fn switching_from_a_populated_machine_leaves_a_valid_config() {
        for from in [MachineProfile::OctaneIp30, MachineProfile::TezroIp53, MachineProfile::O2Ip32] {
            for to in MachineProfile::ALL {
                let mut cfg = MachineConfig::default();
                cfg.machine.profile = from;
                switch_profile(&mut cfg);
                match from {
                    MachineProfile::OctaneIp30 => {
                        cfg.xio_board.push(XioBoardConfig { kind: iris::dev::xio_pci::Kind::Shoehorn, widget: None });
                        cfg.nvme.path = "nvme.raw".into();
                        cfg.serial_log_b = Some("b.log".into());
                        cfg.scsi.insert(iris::config::scsi_slot(1, 12), ScsiDeviceConfig { path: "w.raw".into(), ..Default::default() });
                    }
                    MachineProfile::TezroIp53 => {
                        cfg.machine.emu_id = 7;
                        cfg.machine.cpus = Some(1);
                        cfg.jitv2.compile = JitCompile::Sync;
                        cfg.network.mode = NetMode::Off;
                    }
                    _ => {
                        cfg.nvme.path = "nvme.raw".into();
                        cfg.serial_log_b = Some("b.log".into());
                    }
                }
                cfg.validate().unwrap_or_else(|e| panic!("populated {from:?}: {e}"));
                cfg.machine.profile = to;
                switch_profile(&mut cfg);
                cfg.validate().unwrap_or_else(|e| panic!("{from:?} -> {to:?}: {e}"));
            }
        }
    }

    /// An older `[scsi.3] controller = 1` table is found at (1, 3) only, and
    /// is named 1:3; key 19 says the same.
    #[test]
    fn scsi_targets_are_found_by_placement() {
        let mut cfg = MachineConfig::default();
        cfg.machine.profile = MachineProfile::Indigo2Ip22;
        cfg.scsi.clear();
        cfg.scsi.insert(3, ScsiDeviceConfig { controller: 1, ..Default::default() });
        cfg.scsi.insert(iris::config::scsi_slot(1, 5), ScsiDeviceConfig::default());
        assert_eq!(scsi_key(&cfg, 0, 3), None);
        assert_eq!(scsi_key(&cfg, 1, 3), Some(3));
        assert_eq!(scsi_key(&cfg, 1, 5), Some(21));
        assert_eq!((scsi_name(&cfg, 3), scsi_name(&cfg, 21)), ("1:3".to_string(), "1:5".to_string()));
    }

    #[test]
    fn all_offered_ram_presets_validate() {
        for p in MachineProfile::ALL {
            for &mb in crate::model::ram::ram_presets(p) {
                let mut cfg = MachineConfig::default();
                cfg.machine.profile = p;
                switch_profile(&mut cfg);
                cfg.banks = distribute_ram(mb, p);
                assert_eq!(cfg.banks.iter().sum::<u32>(), mb, "{p:?}: {mb}");
                cfg.validate()
                    .unwrap_or_else(|e| panic!("{p:?} {mb} MB: {e}"));
            }
        }
    }

    #[test]
    fn complete_editor_rejects_invalid_and_unknown_settings() {
        assert!(parse_complete("scale = 0").is_err());
        assert!(parse_complete("made_up = true").is_err());
        assert!(parse_complete("[graphics]\ntile = 19").is_err());
    }
}
