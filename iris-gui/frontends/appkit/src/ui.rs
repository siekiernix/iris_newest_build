//! Small AppKit helpers and the one action target every control reports to.

use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSButton, NSColor, NSFont, NSMenuItem, NSTextField, NSToolbarItem, NSUserInterfaceItemIdentification, NSView,
};
use objc2_foundation::NSString;

pub fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(&s.replace('\0', ""))
}

define_class!(
    /// The target of every control's, toolbar item's and menu item's action.
    /// The sender's identifier is the controller's widget id.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IRISTarget"]
    pub struct Target;

    unsafe impl NSObjectProtocol for Target {}

    impl Target {
        #[unsafe(method(act:))]
        fn act(&self, sender: Option<&AnyObject>) {
            if let Some(id) = sender.and_then(sender_id) {
                crate::app::command(&id);
            }
        }

        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            item.identifier().map_or(true, |id| crate::app::enabled(&id.to_string()))
        }

        #[unsafe(method(validateToolbarItem:))]
        fn validate_toolbar_item(&self, item: &NSToolbarItem) -> bool {
            crate::app::enabled(&item.itemIdentifier().to_string())
        }
    }
);

thread_local! {
    static TARGET: OnceCell<Retained<Target>> = const { OnceCell::new() };
}

pub fn target(mtm: MainThreadMarker) -> Retained<Target> {
    TARGET.with(|t| {
        t.get_or_init(|| {
            let this = mtm.alloc::<Target>().set_ivars(());
            // SAFETY: NSObject's plain init.
            unsafe { msg_send![super(this), init] }
        })
        .clone()
    })
}

fn sender_id(sender: &AnyObject) -> Option<String> {
    if let Some(item) = sender.downcast_ref::<NSToolbarItem>() {
        return Some(item.itemIdentifier().to_string());
    }
    if let Some(item) = sender.downcast_ref::<NSMenuItem>() {
        return item.identifier().map(|i| i.to_string());
    }
    let view = sender.downcast_ref::<NSView>()?;
    view.identifier().map(|s| s.to_string())
}

/// Give `view` its controller id, as identifier and accessibility identifier.
pub fn set_id(view: &NSView, id: &str) {
    view.setIdentifier(Some(&ns(id)));
    view.setAccessibilityIdentifier(Some(&ns(id)));
}

/// A push button that reports `id` to the target.
pub fn button(mtm: MainThreadMarker, title: &str, id: &str) -> Retained<NSButton> {
    let t = target(mtm);
    // SAFETY: the target outlives every control (it is never dropped).
    let b = unsafe { NSButton::buttonWithTitle_target_action(&ns(title), Some(&t), Some(sel!(act:)), mtm) };
    set_id(&b, id);
    b
}

/// A non-editable label.
pub fn label(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let l = NSTextField::labelWithString(&ns(text), mtm);
    l.setSelectable(false);
    l
}

/// A label in a bold font of `size` points.
pub fn heading(mtm: MainThreadMarker, text: &str, size: f64) -> Retained<NSTextField> {
    let l = label(mtm, text);
    l.setFont(Some(&NSFont::boldSystemFontOfSize(size)));
    l
}

/// A secondary label (smaller, the secondary text colour).
pub fn secondary(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let l = label(mtm, text);
    l.setTextColor(Some(&NSColor::secondaryLabelColor()));
    l
}

/// Whether this Mac has SF Symbols (macOS 11 and later).
pub fn has_symbols() -> bool {
    objc2_foundation::NSProcessInfo::processInfo()
        .isOperatingSystemAtLeastVersion(objc2_foundation::NSOperatingSystemVersion {
            majorVersion: 11,
            minorVersion: 0,
            patchVersion: 0,
        })
}
