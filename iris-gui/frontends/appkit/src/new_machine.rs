//! The New Machine window: the spec's form (`NewMachineSpec::fields`) drawn
//! with the settings renderer, then Create or Cancel. The spec lives here
//! until Create hands `NewMachineSpec::create()` to the app controller.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSBackingStoreType, NSView, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use iris_gui_core::controller::app::AppAction;
use iris_gui_core::controller::new_machine::{field, NewMachineSpec};
use iris_gui_core::controller::settings::{control_id, part, Value};
use iris_gui_core::controller::{Effect, Enabled};

use crate::settings::{field_view, push, stack, view};
use crate::ui::{heading, ns, set_id};

/// The id of the Cancel button.
const CANCEL: &str = "new-machine.cancel";

pub struct NewMachineWindow {
    pub window: Retained<NSWindow>,
    pub spec: NewMachineSpec,
    holder: Retained<NSView>,
}

/// What a control in the window asks the shell to do.
pub enum Next {
    Redraw,
    Pick(Effect),
    Create(AppAction),
    Close,
    Nothing,
}

impl NewMachineWindow {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
        // SAFETY: a standard window, kept by the shell while it is open.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                NSRect::new(NSPoint::new(240.0, 240.0), NSSize::new(640.0, 520.0)),
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: the shell keeps the window.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&ns(&t!("dialog-new-machine-title")));
        let holder = NSView::new(mtm);
        window.setContentView(Some(&holder));
        set_id(&holder, "new-machine.window");
        let mut w = Self { window, spec: NewMachineSpec::default(), holder };
        w.redraw(mtm);
        w.window.center();
        w
    }

    /// Rebuild the form from the spec.
    pub fn redraw(&mut self, mtm: MainThreadMarker) {
        self.window.makeFirstResponder(None);
        for v in self.holder.subviews().iter() {
            v.removeFromSuperview();
        }
        let mut rows: Vec<Retained<NSView>> = vec![view(&heading(mtm, &t!("dialog-new-machine-title"), 20.0))];
        let mut toml = None;
        for f in self.spec.fields() {
            rows.push(field_view(mtm, &f, &mut toml));
        }
        let create = push(mtm, field::CREATE, &t!("dialog-new-machine-create"));
        create.setKeyEquivalent(&ns("\r"));
        if let Enabled::No(why) = self.spec.can_create() {
            create.setEnabled(false);
            create.setToolTip(Some(&ns(&why)));
        }
        let cancel = push(mtm, CANCEL, &t!("common-cancel"));
        cancel.setKeyEquivalent(&ns("\u{1b}"));
        let spacer = NSView::new(mtm);
        spacer.widthAnchor().constraintGreaterThanOrEqualToConstant(240.0).setActive(true);
        rows.push(view(&stack(mtm, &[spacer, view(&cancel), view(&create)], false, 8.0)));
        let page = stack(mtm, &rows, true, 14.0);
        page.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 20.0, left: 24.0, bottom: 20.0, right: 24.0 });
        page.setTranslatesAutoresizingMaskIntoConstraints(false);
        self.holder.addSubview(&page);
        page.topAnchor().constraintEqualToAnchor(&self.holder.topAnchor()).setActive(true);
        page.leadingAnchor().constraintEqualToAnchor(&self.holder.leadingAnchor()).setActive(true);
        page.trailingAnchor().constraintEqualToAnchor(&self.holder.trailingAnchor()).setActive(true);
        page.bottomAnchor().constraintEqualToAnchor(&self.holder.bottomAnchor()).setActive(true);
    }

    /// A control in the window reported `value`.
    pub fn control(&mut self, id: &str, value: Value) -> Next {
        if id == CANCEL {
            return Next::Close;
        }
        if id == field::CREATE {
            return match self.spec.can_create() {
                Enabled::Yes => Next::Create(self.spec.create()),
                Enabled::No(_) => Next::Nothing,
            };
        }
        for f in [field::DISK, field::CDROM] {
            if id == control_id(f, None, Some(part::BROWSE)) {
                return self.spec.pick(f).map_or(Next::Nothing, Next::Pick);
            }
        }
        if self.spec.apply(id, value) {
            Next::Redraw
        } else {
            Next::Nothing
        }
    }
}
