//! The main window (D3): toolbar, the library on the left, the selected
//! machine on the right (its summary when stopped), the status bar under it.

use objc2::rc::Retained;
use objc2::Message;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2::{define_class, MainThreadOnly};
use objc2_app_kit::NSWindowDelegate;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSColor, NSFont, NSScrollView, NSSplitView, NSStackView, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize};
use iris_gui_core::controller::app::AppController;
use iris_gui_core::controller::ids;
use iris_gui_core::controller::overview::Overview;
use iris_gui_core::controller::session::{DeviceLight, LightState};

use crate::library::Library;
use crate::toolbar;
use crate::ui::{label, ns, secondary, set_id};

pub struct MainWindow {
    pub window: Retained<NSWindow>,
    split: Retained<NSSplitView>,
    library: Library,
    content: Retained<NSScrollView>,
    /// Holds the summary or the display, whichever the run state calls for.
    pane: Retained<NSView>,
    pub display: Retained<crate::display::DisplayView>,
    detached: Option<Retained<NSWindow>>,
    _window_delegate: Retained<WindowDelegate>,
    status: Retained<NSTextField>,
    lights: Retained<NSStackView>,
    light_buttons: Vec<Retained<NSButton>>,
    last_lights: Vec<DeviceLight>,
    toast: Retained<NSTextField>,
    _toolbar_delegate: Retained<toolbar::Delegate>,
    toolbar: Retained<objc2_app_kit::NSToolbar>,
    toolbar_layout: Vec<&'static str>,
    /// The machine page's tab: 0 Details, 1 Saved States.
    pub summary_tab: usize,
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
        let display = crate::display::DisplayView::new(mtm);
        let pane = NSView::new(mtm);
        for v in [Retained::into_super(content.clone()), Retained::into_super(display.clone())] {
            v.setTranslatesAutoresizingMaskIntoConstraints(false);
            pane.addSubview(&v);
            pin(&v, &pane);
        }
        display.setHidden(true);
        split.addSubview(&library.scroll);
        split.addSubview(&pane);
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
        window.setAcceptsMouseMovedEvents(true);
        let window_delegate = WindowDelegate::new(mtm);
        window.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*window_delegate)));
        Self {
            window,
            split,
            library,
            content,
            pane,
            display,
            detached: None,
            _window_delegate: window_delegate,
            status,
            lights,
            light_buttons: Vec::new(),
            last_lights: Vec::new(),
            toast,
            _toolbar_delegate: delegate,
            toolbar: tb.clone(),
            toolbar_layout: Vec::new(),
            summary_tab: 0,
            last: None,
            library_hidden: false,
        }
    }

    /// The display follows the controller: shown while a machine runs (in
    /// its own window when detached), capture, full screen and scaling.
    pub fn sync_display(&mut self, mtm: MainThreadMarker, app: &AppController) {
        let running = app.session().is_running();
        let d = app.display();
        let display_view: &NSView = &self.display;
        // Detached: the display moves to a window of its own.
        if running && d.detached() && self.detached.is_none() {
            let w = new_display_window(mtm);
            display_view.removeFromSuperview();
            w.setContentView(Some(display_view));
            w.makeKeyAndOrderFront(None);
            self.detached = Some(w);
        } else if (!running || !d.detached()) && self.detached.is_some() {
            if let Some(w) = self.detached.take() {
                display_view.removeFromSuperview();
                w.orderOut(None);
            }
            display_view.setTranslatesAutoresizingMaskIntoConstraints(false);
            self.pane.addSubview(display_view);
            pin(display_view, &self.pane);
        }
        let show_display = running && self.detached.is_none();
        if self.display.isHidden() == show_display {
            self.display.setHidden(!show_display);
            self.content.setHidden(show_display);
            if show_display {
                self.window.makeFirstResponder(Some(&self.display));
            }
        }
        self.display.set_captured(running && d.captured());
        self.display.set_follow(running && iris_gui_core::controller::display::follows_pointer(app.prefs()));
        let (max, integer) = iris_gui_core::controller::display::scaling(app.prefs(), 1.0);
        let filter = match app.prefs().display_filter {
            iris_gui_core::controller::display::DisplayFilter::Auto => None,
            iris_gui_core::controller::display::DisplayFilter::Nearest => Some(true),
            iris_gui_core::controller::display::DisplayFilter::Linear => Some(false),
        };
        self.display.set_scaling(max, integer, filter);
        self.display.set_overlay(mtm, app.display_overlay().as_deref());
        // Full screen: the window that shows the display.
        let w = self.detached.clone().unwrap_or_else(|| self.window.clone());
        let is_full = w.styleMask().contains(NSWindowStyleMask::FullScreen);
        if is_full != d.fullscreen() {
            w.toggleFullScreen(None);
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

    /// Show the machine page's other tab on the next refresh.
    pub fn set_summary_tab(&mut self, tab: usize) {
        if self.summary_tab != tab {
            self.summary_tab = tab;
            self.last = None;
        }
    }

    /// Bring every view in line with the controller. Views are set only when
    /// their value changed.
    pub fn refresh(&mut self, mtm: MainThreadMarker, app: &AppController) {
        self.sync_display(mtm, app);
        self.library.refresh(&app.library());
        let title = app.selected().map(str::to_string).unwrap_or_else(|| t!("app-name"));
        if self.window.title().to_string() != title {
            self.window.setTitle(&ns(&title));
        }
        // The model under the name, as macOS 11+ windows can show it.
        if crate::ui::has_symbols() {
            let sub = app.overview().map(|o| o.spec).unwrap_or_default();
            if self.window.subtitle().to_string() != sub {
                self.window.setSubtitle(&ns(&sub));
            }
        }
        // The toolbar follows the machine's state.
        let layout = app.toolbar_layout();
        if layout != self.toolbar_layout {
            toolbar::set_layout(&self.toolbar, &layout);
            self.toolbar_layout = layout;
        }
        let o = app.overview();
        if self.last.as_ref() != Some(&o) {
            let page = crate::summary::build(mtm, o.as_ref(), self.summary_tab);
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
        // The lights: one borderless button each, made when the set of
        // lights changes and restyled in place otherwise, so a click is not
        // lost to a refresh. A click runs the command the light stands for
        // (`AppController::light_command`), routed by its id.
        let same = self.light_buttons.len() == bar.lights.len()
            && self.last_lights.iter().zip(&bar.lights).all(|(a, b)| a.id == b.id);
        if !same {
            for v in self.lights.arrangedSubviews().iter() {
                self.lights.removeArrangedSubview(&v);
                v.removeFromSuperview();
            }
            self.light_buttons.clear();
            self.last_lights.clear();
            for l in &bar.lights {
                let b = crate::ui::button(mtm, "", &l.id);
                b.setBordered(false);
                self.lights.addArrangedSubview(&b);
                self.light_buttons.push(b);
            }
        }
        for (i, l) in bar.lights.iter().enumerate() {
            let enabled = app
                .light_command(&l.id)
                .is_some_and(|c| app.command_enabled(&c) == iris_gui_core::controller::Enabled::Yes);
            if self.last_lights.get(i) == Some(l) && self.light_buttons[i].isEnabled() == enabled {
                continue;
            }
            style_light(&self.light_buttons[i], l, enabled);
        }
        self.last_lights = bar.lights.clone();
    }
}

fn style_light(b: &NSButton, l: &DeviceLight, enabled: bool) {
    match crate::glyphs::icon(l.icon, &l.tooltip) {
        Some(img) => b.setImage(Some(&img)),
        None => b.setTitle(&ns("\u{2022}")),
    }
    if let Some(badge) = &l.badge {
        b.setTitle(&ns(badge));
        b.setImagePosition(objc2_app_kit::NSCellImagePosition::ImageLeading);
    } else if b.image().is_some() {
        b.setTitle(&ns(""));
        b.setImagePosition(objc2_app_kit::NSCellImagePosition::ImageOnly);
    }
    let color = match l.state {
        LightState::Off => NSColor::tertiaryLabelColor(),
        LightState::Idle => NSColor::secondaryLabelColor(),
        LightState::Reading | LightState::Active => NSColor::systemGreenColor(),
        LightState::Writing => NSColor::systemOrangeColor(),
        LightState::Fault => NSColor::systemRedColor(),
    };
    b.setContentTintColor(Some(&color));
    b.setEnabled(enabled);
    b.setToolTip(Some(&ns(&l.tooltip)));
}

fn pin(v: &NSView, to: &NSView) {
    v.leadingAnchor().constraintEqualToAnchor(&to.leadingAnchor()).setActive(true);
    v.trailingAnchor().constraintEqualToAnchor(&to.trailingAnchor()).setActive(true);
    v.topAnchor().constraintEqualToAnchor(&to.topAnchor()).setActive(true);
    v.bottomAnchor().constraintEqualToAnchor(&to.bottomAnchor()).setActive(true);
}

fn new_display_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Miniaturizable | NSWindowStyleMask::Resizable;
    // SAFETY: a standard window, kept by the main window while detached.
    let w = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            NSRect::new(NSPoint::new(160.0, 160.0), NSSize::new(1282.0, 1024.0)),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the main window keeps it.
    unsafe { w.setReleasedWhenClosed(false) };
    w.setTitle(&ns(&t!("tb-detach")));
    w.setAcceptsMouseMovedEvents(true);
    w
}

define_class!(
    /// The main window's delegate: focus for the capture rules, full screen
    /// the user left by other means.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IRISWindowDelegate"]
    pub struct WindowDelegate;

    unsafe impl NSObjectProtocol for WindowDelegate {}

    unsafe impl NSWindowDelegate for WindowDelegate {
        #[unsafe(method(windowDidBecomeKey:))]
        fn became_key(&self, _n: &objc2_foundation::NSNotification) {
            crate::app::display(iris_gui_core::controller::display::DisplayAction::Focus(true));
        }

        #[unsafe(method(windowDidResignKey:))]
        fn resigned_key(&self, _n: &objc2_foundation::NSNotification) {
            crate::app::display(iris_gui_core::controller::display::DisplayAction::Focus(false));
        }

        #[unsafe(method(windowDidEnterFullScreen:))]
        fn entered_full(&self, _n: &objc2_foundation::NSNotification) {
            crate::app::display(iris_gui_core::controller::display::DisplayAction::FullScreenChanged(true));
        }

        #[unsafe(method(windowDidExitFullScreen:))]
        fn exited_full(&self, _n: &objc2_foundation::NSNotification) {
            crate::app::display(iris_gui_core::controller::display::DisplayAction::FullScreenChanged(false));
        }
    }
);

impl WindowDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<WindowDelegate>().set_ivars(());
        // SAFETY: NSObject's plain init.
        unsafe { objc2::msg_send![super(this), init] }
    }
}

fn as_view(t: &Retained<NSTextField>) -> Retained<NSView> {
    let v: &NSView = t.as_ref();
    v.retain()
}
