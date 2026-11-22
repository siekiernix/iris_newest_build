//! The one per-profile table the GUI keeps (D8). The core says what hardware a
//! profile has (`MachineProfile::cpus()`, `memory_banks()`, `GraphicsBoard`);
//! what it does not say, and every shell needs, lives here, so adding a machine
//! is the core profile, one row here and one picture.

use iris::config::MachineProfile;

/// The groups the library shows machines in by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Indy,
    Indigo2,
    O2,
    Octane,
    FuelTezro,
    OriginOnyx,
}

impl Family {
    pub const ALL: &'static [Family] =
        &[Family::Indy, Family::Indigo2, Family::O2, Family::Octane, Family::FuelTezro, Family::OriginOnyx];

    pub fn title(self) -> String {
        match self {
            Family::Indy => t!("library-family-indy"),
            Family::Indigo2 => t!("library-family-indigo2"),
            Family::O2 => t!("library-family-o2"),
            Family::Octane => t!("library-family-octane"),
            Family::FuelTezro => t!("library-family-fuel-tezro"),
            Family::OriginOnyx => t!("library-family-origin-onyx"),
        }
    }
}

/// A picture of the machine for the library and the summary. Each shell maps
/// these to its own images; `svg_name` names the shared fallback set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MachineIcon {
    Indy,
    Indigo2,
    Indigo2Impact,
    O2,
    Octane,
    Origin200,
    Origin2000,
    Onyx2,
    Fuel,
    Tezro,
    Origin3200,
    Onyx3000,
}

impl MachineIcon {
    pub fn svg_name(self) -> &'static str {
        match self {
            MachineIcon::Indy => "machine-indy",
            MachineIcon::Indigo2 => "machine-indigo2",
            MachineIcon::Indigo2Impact => "machine-indigo2-impact",
            MachineIcon::O2 => "machine-o2",
            MachineIcon::Octane => "machine-octane",
            MachineIcon::Origin200 => "machine-origin200",
            MachineIcon::Origin2000 => "machine-origin2000",
            MachineIcon::Onyx2 => "machine-onyx2",
            MachineIcon::Fuel => "machine-fuel",
            MachineIcon::Tezro => "machine-tezro",
            MachineIcon::Origin3200 => "machine-origin3200",
            MachineIcon::Onyx3000 => "machine-onyx3000",
        }
    }
}

/// What the GUI needs to know about a profile beyond the core's tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileTraits {
    pub profile: MachineProfile,
    pub family: Family,
    pub icon: MachineIcon,
    /// The product as people say it: "Indy", "Indigo2 IMPACT", "Origin 3200".
    pub model: &'static str,
    /// The board number: "IP24".
    pub board: &'static str,
}

const fn row(profile: MachineProfile, family: Family, icon: MachineIcon, model: &'static str, board: &'static str) -> ProfileTraits {
    ProfileTraits { profile, family, icon, model, board }
}

/// One row per profile, in `MachineProfile::ALL` order.
pub const TRAITS: &[ProfileTraits] = &[
    row(MachineProfile::IndyIp24, Family::Indy, MachineIcon::Indy, "Indy", "IP24"),
    row(MachineProfile::Indigo2Ip22, Family::Indigo2, MachineIcon::Indigo2, "Indigo2", "IP22"),
    row(MachineProfile::Indigo2Ip28, Family::Indigo2, MachineIcon::Indigo2Impact, "Indigo2 IMPACT", "IP28"),
    row(MachineProfile::O2Ip32, Family::O2, MachineIcon::O2, "O2", "IP32"),
    row(MachineProfile::OctaneIp30, Family::Octane, MachineIcon::Octane, "Octane", "IP30"),
    row(MachineProfile::Origin200Ip29, Family::OriginOnyx, MachineIcon::Origin200, "Origin 200", "IP29"),
    row(MachineProfile::Origin2000Ip27, Family::OriginOnyx, MachineIcon::Origin2000, "Origin 2000", "IP27"),
    row(MachineProfile::Onyx2Ip27, Family::OriginOnyx, MachineIcon::Onyx2, "Onyx2", "IP27"),
    row(MachineProfile::FuelIp34, Family::FuelTezro, MachineIcon::Fuel, "Fuel", "IP34"),
    row(MachineProfile::TezroIp53, Family::FuelTezro, MachineIcon::Tezro, "Tezro", "IP53"),
    row(MachineProfile::Origin3200Ip35, Family::OriginOnyx, MachineIcon::Origin3200, "Origin 3200", "IP35"),
    row(MachineProfile::Onyx3000Ip35, Family::OriginOnyx, MachineIcon::Onyx3000, "Onyx 3000", "IP35"),
];

/// The traits row for `profile`.
pub fn traits(profile: MachineProfile) -> &'static ProfileTraits {
    TRAITS.iter().find(|t| t.profile == profile).expect("every profile has a ProfileTraits row")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_exactly_one_row() {
        for p in MachineProfile::ALL {
            assert_eq!(TRAITS.iter().filter(|t| t.profile == p).count(), 1, "{p:?}");
        }
        assert_eq!(TRAITS.len(), MachineProfile::ALL.len());
    }

    #[test]
    fn the_board_matches_the_core_label() {
        for t in TRAITS {
            assert!(t.profile.label().contains(t.board), "{:?}", t.profile);
        }
    }
}
