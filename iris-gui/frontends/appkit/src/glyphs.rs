//! The glyph for each icon meaning (D11): SF Symbols on macOS 11 and later.
//! Below 11 there are no symbols; controls fall back to their titles until the
//! shared SVG set in `iris-gui/assets/icons/` exists.

use objc2::rc::Retained;
use objc2_app_kit::NSImage;
use iris_gui_core::controller::icons::Icon;
use iris_gui_core::controller::profile::MachineIcon;

use crate::ui::{has_symbols, ns};

pub fn symbol_name(icon: Icon) -> &'static str {
    match icon {
        Icon::ShowLibrary => "sidebar.left",
        Icon::PowerOn => "play.fill",
        Icon::ShutDownGuest => "power",
        Icon::PowerOff => "stop.fill",
        Icon::Restart => "arrow.clockwise",
        Icon::SaveState => "square.and.arrow.down",
        Icon::SavedStates => "square.stack.3d.up",
        Icon::Screenshot => "camera",
        Icon::InsertDisc => "opticaldisc",
        Icon::BrowseFiles => "folder",
        Icon::DetachDisplay => "macwindow.on.rectangle",
        Icon::FullScreen => "arrow.up.left.and.arrow.down.right",
        Icon::SerialConsole => "terminal",
        Icon::MachineSettings => "slider.horizontal.3",
        Icon::NewMachine => "plus",
        Icon::NewFolder => "folder.badge.plus",
        Icon::Folder => "folder",
        Icon::Disk => "internaldrive",
        Icon::CdRom => "opticaldisc",
        Icon::Network => "network",
        Icon::Audio => "speaker.wave.2",
        Icon::Camera => "video",
        Icon::InputCapture => "keyboard",
        Icon::DisplayScale => "display",
        Icon::Warning => "exclamationmark.triangle",
        Icon::Error => "xmark.octagon",
        Icon::Info => "info.circle",
        Icon::Delete => "trash",
        Icon::Rename => "pencil",
        Icon::Restore => "clock.arrow.circlepath",
        Icon::Export => "square.and.arrow.up",
        Icon::Import => "square.and.arrow.down",
        Icon::Edit => "doc.text",
        Icon::Permissions => "lock",
        Icon::Check => "checkmark.seal",
        Icon::Up => "arrow.up",
        Icon::Refresh => "arrow.clockwise",
        Icon::Cancel => "xmark",
        Icon::Duplicate => "plus.square.on.square",
    }
}

/// A picture for a machine. The plan's answer is a small picture of each
/// machine; until those exist, a desktop or a tower by kind.
pub fn machine_symbol(icon: MachineIcon) -> &'static str {
    match icon {
        MachineIcon::Indy | MachineIcon::Indigo2 | MachineIcon::Indigo2Impact | MachineIcon::O2 => "desktopcomputer",
        MachineIcon::Octane | MachineIcon::Fuel | MachineIcon::Tezro => "pc",
        MachineIcon::Origin200
        | MachineIcon::Origin2000
        | MachineIcon::Onyx2
        | MachineIcon::Origin3200
        | MachineIcon::Onyx3000 => "server.rack",
    }
}

/// The image for `name`, `None` below macOS 11.
pub fn symbol(name: &str, description: &str) -> Option<Retained<NSImage>> {
    if !has_symbols() {
        return None;
    }
    NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns(name), Some(&ns(description)))
}

pub fn icon(icon: Icon, description: &str) -> Option<Retained<NSImage>> {
    symbol(symbol_name(icon), description)
}
