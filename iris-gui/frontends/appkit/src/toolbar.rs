//! The main window's toolbar (plan, "The window"): icon buttons with
//! tooltips, enabled from the controller through `validateToolbarItem:`.

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSToolbar, NSToolbarDelegate, NSToolbarDisplayMode, NSToolbarFlexibleSpaceItemIdentifier, NSToolbarItem};
use objc2_foundation::{NSArray, NSCopying, NSString};
use iris_gui_core::controller::commands;
use iris_gui_core::controller::keys::KeyMap;
use iris_gui_core::controller::ids::toolbar as tb;

use crate::glyphs;
use crate::ui::{ns, target};

const FLEX: &str = "flex";

/// Left to right; `FLEX` is the flexible space before the right-hand group.
const ITEMS: &[&str] = &[
    tb::SHOW_LIBRARY,
    tb::POWER_ON,
    tb::SHUT_DOWN_GUEST,
    tb::POWER_OFF,
    tb::RESTART,
    tb::SAVE_STATE,
    tb::SAVED_STATES,
    tb::SCREENSHOT,
    tb::INSERT_DISC,
    tb::BROWSE_FILES,
    FLEX,
    tb::DETACH_DISPLAY,
    tb::FULL_SCREEN,
    tb::SERIAL_CONSOLE,
    tb::MACHINE_SETTINGS,
];

fn make_item(mtm: MainThreadMarker, id: &NSString) -> Option<Retained<NSToolbarItem>> {
    let key = id.to_string();
    let tip = commands::tip(&key, &KeyMap::default())?;
    let (icon, tooltip) = (tip.icon, tip.hover_text());
    let item = NSToolbarItem::initWithItemIdentifier(mtm.alloc(), id);
    item.setLabel(&ns(&tooltip));
    item.setPaletteLabel(&ns(&tooltip));
    item.setToolTip(Some(&ns(&tooltip)));
    match glyphs::icon(icon, &tooltip) {
        Some(image) => item.setImage(Some(&image)),
        None => item.setLabel(&ns(&tooltip)),
    }
    let t = target(mtm);
    // SAFETY: the target is never dropped.
    unsafe {
        item.setTarget(Some(&t));
        item.setAction(Some(sel!(act:)));
    }
    Some(item)
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IRISToolbarDelegate"]
    pub struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSToolbarDelegate for Delegate {
        #[unsafe(method_id(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:))]
        fn item(&self, _toolbar: &NSToolbar, id: &NSString, _insert: bool) -> Option<Retained<NSToolbarItem>> {
            make_item(self.mtm(), id)
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn defaults(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            all_ids()
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn allowed(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            all_ids()
        }
    }
);

fn all_ids() -> Retained<NSArray<NSString>> {
    let v: Vec<_> = ITEMS
        .iter()
        .map(|id| {
            if *id == FLEX {
                // SAFETY: a constant AppKit defines.
                unsafe { NSToolbarFlexibleSpaceItemIdentifier }.copy()
            } else {
                ns(id)
            }
        })
        .collect();
    NSArray::from_retained_slice(&v)
}

/// The toolbar and its delegate, which the caller keeps (the toolbar holds it weakly).
pub fn build(mtm: MainThreadMarker) -> (Retained<NSToolbar>, Retained<Delegate>) {
    let delegate: Retained<Delegate> = {
        let this = mtm.alloc::<Delegate>().set_ivars(());
        // SAFETY: NSObject's plain init.
        unsafe { msg_send![super(this), init] }
    };
    let toolbar = NSToolbar::initWithIdentifier(mtm.alloc(), &ns("iris.main"));
    toolbar.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    toolbar.setDisplayMode(NSToolbarDisplayMode::IconOnly);
    toolbar.setAllowsUserCustomization(false);
    (toolbar, delegate)
}
