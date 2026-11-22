//! The main window (D3): toolbar, the library on the left, the selected
//! machine on the right (its summary when stopped), the status bar under it.

use objc2::rc::Retained;
use objc2::Message;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFont, NSScrollView, NSSplitView, NSStackView, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize};
use iris_gui_core::controller::app::AppController;
use iris_gui_core::controller::ids;
use iris_gui_core::controller::overview::Overview;
use iris_gui_core::controller::session::LightState;

use crate::library::Library;
use crate::toolbar;
use crate::ui::{label, ns, secondary, set_id};

pub struct MainWindow {
    pub window: Retained<NSWindow>,
    split: Retained<NSSplitView>,
    library: Library,
    content: Retained<NSScrollView>,
    status: Retained<NSTextField>,
    lights: Retained<NSStackView>,
    toast: Retained<NSTextField>,
    _toolbar_delegate: Retained<toolbar::Delegate>,
    last: Option<Option<Overview>>,
    library_hidden: bool,
}

impl MainWindow {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let frame = NSRect::new(NSPoint::new(120.0, 120.0), NSSize::new(1180.0, 760.0));
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        // SAFETY: a standard window; it is kept until the app quits.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(mtm.alloc(), frame, style, NSBackingStoreType::Buffered, false)
        };
        // SAFETY: we keep the window alive ourselves.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&ns(&t!("app-name")));
        window.setMinSize(NSSize::new(760.0, 480.0));
        let (tb, delegate) = toolbar::build(mtm);
        window.setToolbar(Some(&tb));

        let library = Library::new(mtm);
        let content = NSScrollView::initWithFrame(mtm.alloc(), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(900.0, 700.0)));
        content.setHasVerticalScroller(true);
        content.setDrawsBackground(false);
        let split = NSSplitView::initWithFrame(mtm.alloc(), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1180.0, 700.0)));
        split.setVertical(true);
        split.setDividerStyle(objc2_app_kit::NSSplitViewDividerStyle::Thin);
        split.addSubview(&library.scroll);
        split.addSubview(&content);
        split.setHoldingPriority_forSubviewAtIndex(260.0, 0);

        let status = label(mtm, "");
        status.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        set_id(&status, ids::status::RUN_STATE);
        let toast = secondary(mtm, "");
        toast.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        set_id(&toast, ids::status::TOAST);
        let lights = NSStackView::new(mtm);
        lights.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        lights.setSpacing(6.0);
        let spacer = NSView::new(mtm);
        spacer.setContentHuggingPriority_forOrientation(1.0, objc2_app_kit::NSLayoutConstraintOrientation::Horizontal);
        let bar = NSStackView::stackViewWithViews(
            &NSArray::from_retained_slice(&[
                as_view(&status),
                as_view(&toast),
                spacer,
                Retained::into_super(lights.clone()),
            ]),
            mtm,
        );
        bar.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        bar.setSpacing(12.0);
        bar.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 4.0, left: 12.0, bottom: 4.0, right: 12.0 });
        bar.heightAnchor().constraintEqualToConstant(26.0).setActive(true);

        // The split fills the window above the status bar, edge to edge.
        let root = NSView::new(mtm);
        for v in [Retained::into_super(split.clone()), Retained::into_super(bar.clone())] {
            v.setTranslatesAutoresizingMaskIntoConstraints(false);
            root.addSubview(&v);
            v.leadingAnchor().constraintEqualToAnchor(&root.leadingAnchor()).setActive(true);
            v.trailingAnchor().constraintEqualToAnchor(&root.trailingAnchor()).setActive(true);
        }
        split.topAnchor().constraintEqualToAnchor(&root.topAnchor()).setActive(true);
        split.bottomAnchor().constraintEqualToAnchor(&bar.topAnchor()).setActive(true);
        bar.bottomAnchor().constraintEqualToAnchor(&root.bottomAnchor()).setActive(true);
        window.setContentView(Some(&root));
        split.setPosition_ofDividerAtIndex(250.0, 0);
        window.center();
        window.setFrameAutosaveName(&ns("IRISMainWindow"));
        Self {
            window,
            split,
            library,
            content,
            status,
            lights,
            toast,
            _toolbar_delegate: delegate,
            last: None,
            library_hidden: false,
        }
    }

    pub fn toggle_library(&mut self) {
        self.library_hidden = !self.library_hidden;
        self.library.scroll.setHidden(self.library_hidden);
        self.split.adjustSubviews();
    }

    pub fn toast(&self, text: &str) {
        self.toast.setStringValue(&ns(text));
    }

    /// Bring every view in line with the controller. Views are set only when
    /// their value changed.
    pub fn refresh(&mut self, mtm: MainThreadMarker, app: &AppController) {
        self.library.refresh(&app.library());
        let title = app.selected().map(str::to_string).unwrap_or_else(|| t!("app-name"));
        if self.window.title().to_string() != title {
            self.window.setTitle(&ns(&title));
        }
        let o = app.overview();
        if self.last.as_ref() != Some(&o) {
            let page = crate::summary::build(mtm, o.as_ref());
            // A flipped document view holds the page at its top; the page
            // takes the pane's width and its own height.
            let doc = crate::ui::flipped(mtm);
            doc.setTranslatesAutoresizingMaskIntoConstraints(false);
            page.setTranslatesAutoresizingMaskIntoConstraints(false);
            doc.addSubview(&page);
            self.content.setDocumentView(Some(&doc));
            let clip = self.content.contentView();
            doc.topAnchor().constraintEqualToAnchor(&clip.topAnchor()).setActive(true);
            doc.leadingAnchor().constraintEqualToAnchor(&clip.leadingAnchor()).setActive(true);
            doc.widthAnchor().constraintEqualToAnchor(&clip.widthAnchor()).setActive(true);
            page.topAnchor().constraintEqualToAnchor(&doc.topAnchor()).setActive(true);
            page.leadingAnchor().constraintEqualToAnchor(&doc.leadingAnchor()).setActive(true);
            page.trailingAnchor().constraintLessThanOrEqualToAnchor(&doc.trailingAnchor()).setActive(true);
            doc.heightAnchor().constraintEqualToAnchor(&page.heightAnchor()).setActive(true);
            self.last = Some(o);
        }
        let bar = app.status_bar();
        let mut s = format!("{}   {}", bar.run_label, bar.executor);
        if let Some(p) = &bar.cpu_pending {
            s.push_str(&format!("   {p}"));
        }
        if bar.unsaved {
            s.push_str("   \u{2022}");
        }
        if let Some(m) = &bar.mips {
            s.push_str(&format!("   {m}"));
        }
        if let Some(h) = &bar.hz {
            s.push_str(&format!("   {h}"));
        }
        if self.status.stringValue().to_string() != s {
            self.status.setStringValue(&ns(&s));
        }
        // The lights: rebuilt each refresh (a handful of small views).
        for v in self.lights.arrangedSubviews().iter() {
            self.lights.removeArrangedSubview(&v);
            v.removeFromSuperview();
        }
        for l in &bar.lights {
            let img = crate::glyphs::icon(l.icon, &l.tooltip);
            let v: Retained<NSView> = match img {
                Some(img) => {
                    let iv = objc2_app_kit::NSImageView::imageViewWithImage(&img, mtm);
                    let color = match l.state {
                        LightState::Off => NSColor::tertiaryLabelColor(),
                        LightState::Idle => NSColor::secondaryLabelColor(),
                        LightState::Reading | LightState::Active => NSColor::systemGreenColor(),
                        LightState::Writing => NSColor::systemOrangeColor(),
                        LightState::Fault => NSColor::systemRedColor(),
                    };
                    iv.setContentTintColor(Some(&color));
                    Retained::into_super(Retained::into_super(iv))
                }
                None => as_view(&label(mtm, "\u{2022}")),
            };
            v.setToolTip(Some(&ns(&l.tooltip)));
            set_id(&v, &l.id);
            self.lights.addArrangedSubview(&v);
        }
    }
}

fn as_view(t: &Retained<NSTextField>) -> Retained<NSView> {
    let v: &NSView = t.as_ref();
    v.retain()
}
