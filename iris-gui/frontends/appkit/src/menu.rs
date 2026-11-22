//! The menu bar (D7, plan "Menu bar (macOS)"). Every item of ours carries a
//! command id, goes to the action target and greys out with the controller's
//! `command_enabled`; shortcuts come from the controller's key map. The
//! standard items (Edit, Window) go to the first responder.

use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2::{sel, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem, NSUserInterfaceItemIdentification};
use iris_gui_core::controller::commands::{id_of, menu as m};
use iris_gui_core::controller::ids::toolbar as tb;
use iris_gui_core::controller::keys::{Command, Key, KeyMap};

use crate::ui::{ns, target};

fn system(mtm: MainThreadMarker, title: &str, action: Sel, key: &str) -> Retained<NSMenuItem> {
    // SAFETY: a plain menu item; its action goes up the responder chain.
    unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &ns(title), Some(action), &ns(key)) }
}

/// An item of ours: reports `id`, with the key map's shortcut for it.
fn ours(mtm: MainThreadMarker, keys: &KeyMap, title: &str, id: &str) -> Retained<NSMenuItem> {
    let it = system(mtm, title, sel!(act:), "");
    let t = target(mtm);
    // SAFETY: the target is never dropped.
    unsafe { it.setTarget(Some(&t)) };
    it.setIdentifier(Some(&ns(id)));
    if let Some(cmd) = Command::ALL.iter().copied().find(|c| id_of(*c) == id) {
        if let Some(chord) = keys.chord(cmd) {
            let key = match chord.key {
                Key::Char(c) => c.to_ascii_lowercase().to_string(),
                // F-keys are handled by the key map in the window, not as equivalents.
                _ => String::new(),
            };
            if !key.is_empty() {
                let mut mods = NSEventModifierFlags::empty();
                if chord.primary {
                    mods |= NSEventModifierFlags::Command;
                }
                if chord.mac_ctrl {
                    mods |= NSEventModifierFlags::Control;
                }
                if chord.alt {
                    mods |= NSEventModifierFlags::Option;
                }
                if chord.shift {
                    mods |= NSEventModifierFlags::Shift;
                }
                it.setKeyEquivalent(&ns(&key));
                it.setKeyEquivalentModifierMask(mods);
            }
        }
    }
    it
}

fn submenu(mtm: MainThreadMarker, bar: &NSMenu, title: &str, items: Vec<Option<Retained<NSMenuItem>>>) -> Retained<NSMenu> {
    let menu = NSMenu::initWithTitle(mtm.alloc(), &ns(title));
    for it in items {
        match it {
            Some(it) => menu.addItem(&it),
            None => menu.addItem(&NSMenuItem::separatorItem(mtm)),
        }
    }
    let host = NSMenuItem::new(mtm);
    host.setSubmenu(Some(&menu));
    bar.addItem(&host);
    menu
}

pub fn build(mtm: MainThreadMarker) {
    let keys = KeyMap::default();
    let o = |title: String, id: &str| Some(ours(mtm, &keys, &title, id));
    let bar = NSMenu::new(mtm);

    let app_name = t!("app-name");
    submenu(
        mtm,
        &bar,
        &app_name,
        vec![
            o(t!("mb-about", name = app_name.as_str()), m::ABOUT),
            None,
            o(t!("mb-app-settings"), m::APP_SETTINGS),
            None,
            Some(system(mtm, &t!("mb-hide", name = app_name.as_str()), sel!(hide:), "h")),
            None,
            o(t!("mb-quit", name = app_name.as_str()), m::QUIT),
        ],
    );
    submenu(
        mtm,
        &bar,
        &t!("mb-file"),
        vec![
            o(t!("mb-new-machine"), m::NEW_MACHINE),
            o(t!("mb-new-folder"), m::NEW_FOLDER),
            None,
            o(t!("mb-duplicate"), m::DUPLICATE),
            o(t!("mb-rename"), m::RENAME),
            o(t!("mb-delete"), m::DELETE),
            None,
            o(t!("mb-show-in-finder"), m::SHOW_IN_FINDER),
            o(t!("mb-open-config"), m::OPEN_CONFIG),
            None,
            o(t!("mb-close"), m::CLOSE),
        ],
    );
    submenu(
        mtm,
        &bar,
        &t!("mb-edit"),
        vec![
            Some(system(mtm, &t!("mb-undo"), sel!(undo:), "z")),
            None,
            Some(system(mtm, &t!("mb-cut"), sel!(cut:), "x")),
            Some(system(mtm, &t!("mb-copy"), sel!(copy:), "c")),
            Some(system(mtm, &t!("mb-paste"), sel!(paste:), "v")),
            Some(system(mtm, &t!("mb-select-all"), sel!(selectAll:), "a")),
        ],
    );
    submenu(
        mtm,
        &bar,
        &t!("mb-view"),
        vec![
            o(t!("tb-show-library"), tb::SHOW_LIBRARY),
            None,
            o(t!("tb-full-screen"), tb::FULL_SCREEN),
            o(t!("tb-detach"), tb::DETACH_DISPLAY),
            None,
            o(t!("tb-serial-console"), tb::SERIAL_CONSOLE),
        ],
    );
    let send = NSMenu::initWithTitle(mtm.alloc(), &ns(&t!("mb-send-key")));
    for (title, id) in [("F11", m::SEND_F11), ("Ctrl+Alt+Del", m::SEND_CAD), ("Esc", m::SEND_ESC)] {
        send.addItem(&ours(mtm, &keys, title, id));
    }
    let send_host = NSMenuItem::new(mtm);
    send_host.setTitle(&ns(&t!("mb-send-key")));
    send_host.setSubmenu(Some(&send));
    submenu(
        mtm,
        &bar,
        &t!("mb-machine"),
        vec![
            o(t!("tb-power-on"), tb::POWER_ON),
            o(t!("tb-shut-down"), tb::SHUT_DOWN_GUEST),
            o(t!("tb-power-off"), tb::POWER_OFF),
            o(t!("tb-restart"), tb::RESTART),
            None,
            o(t!("tb-save-state"), tb::SAVE_STATE),
            o(t!("tb-saved-states"), tb::SAVED_STATES),
            o(t!("tb-screenshot"), tb::SCREENSHOT),
            None,
            o(t!("mb-capture"), m::CAPTURE),
            Some(send_host),
            None,
            o(t!("tb-settings"), tb::MACHINE_SETTINGS),
        ],
    );
    submenu(
        mtm,
        &bar,
        &t!("mb-devices"),
        vec![
            o(t!("tb-insert-disc"), tb::INSERT_DISC),
            o(t!("tb-browse-files"), tb::BROWSE_FILES),
            None,
            o(t!("mb-sync-disks"), m::SYNC_DISKS),
            o(t!("mb-check-network"), m::CHECK_NETWORK),
        ],
    );
    submenu(
        mtm,
        &bar,
        &t!("mb-tools"),
        vec![
            o(t!("mb-benchmark"), m::BENCHMARK),
            o(t!("mb-camera-test"), m::CAMERA_TEST),
            o(t!("mb-diagnostics"), m::DIAGNOSTICS),
            None,
            o(t!("mb-reset-nvram"), m::RESET_NVRAM),
        ],
    );
    let window = submenu(
        mtm,
        &bar,
        &t!("mb-window"),
        vec![
            Some(system(mtm, &t!("mb-minimize"), sel!(performMiniaturize:), "m")),
            Some(system(mtm, &t!("mb-zoom"), sel!(performZoom:), "")),
        ],
    );
    submenu(mtm, &bar, &t!("mb-help"), vec![o(t!("mb-guide"), m::GUIDE), o(t!("mb-report-issue"), m::REPORT_ISSUE)]);

    let app = NSApplication::sharedApplication(mtm);
    app.setMainMenu(Some(&bar));
    app.setWindowsMenu(Some(&window));
}
