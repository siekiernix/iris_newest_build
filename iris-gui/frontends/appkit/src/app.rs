//! The application: `NSApplication` and its delegate, the one app
//! controller, and the loop of action -> effects -> refresh.
//!
//! The controller lives in a thread-local cell. It is borrowed only to apply
//! an action or tick and to refresh views; effects (modal panels) run with no
//! borrow held, so a panel's run loop may tick the app underneath.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSApplicationTerminateReply};
use objc2_foundation::NSNotification;
use iris_gui_core::controller::app::{AppAction, AppController};
use iris_gui_core::controller::commands::ShellCommand;
use iris_gui_core::controller::{Controller, Effect, Enabled};

use crate::wake::MacWake;
use crate::window::MainWindow;

struct Shell {
    app: AppController,
    win: MainWindow,
    wake: Arc<MacWake>,
    /// The controller agreed to quit: let the next terminate through.
    may_quit: bool,
}

thread_local! {
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<Delegate>>> = const { RefCell::new(None) };
    static LATER: RefCell<VecDeque<Box<dyn FnOnce()>>> = RefCell::default();
    static SCRIPT: RefCell<VecDeque<String>> = RefCell::default();
}

fn mtm() -> MainThreadMarker {
    MainThreadMarker::new().expect("the shell runs on the main thread")
}

/// Run `f` with the shell borrowed; `None` when it is busy (a modal panel
/// ticked from inside a handler) or not built yet.
fn with<R>(f: impl FnOnce(&mut Shell) -> R) -> Option<R> {
    SHELL.with(|s| s.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IRISAppDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _note: &NSNotification) {
            start(self.mtm());
        }

        /// Cmd+Q and the app menu's Quit come here, not through a window
        /// close: the controller decides (it may fold CHD diffs first and
        /// answer with Effect::Quit later).
        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self, _app: &NSApplication) -> NSApplicationTerminateReply {
            if with(|s| s.may_quit).unwrap_or(true) {
                return NSApplicationTerminateReply::TerminateNow;
            }
            later(Box::new(|| dispatch(AppAction::RequestQuit)));
            NSApplicationTerminateReply::TerminateCancel
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _note: &NSNotification) {
            with(|s| s.app.shutdown());
            AppController::release_instance();
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn last_window_closed(&self, _app: &NSApplication) -> bool {
            true
        }
    }
);

/// Run the application until it quits.
pub fn run() {
    let mtm = mtm();
    let ns_app = NSApplication::sharedApplication(mtm);
    ns_app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate: Retained<Delegate> = {
        let this = mtm.alloc::<Delegate>().set_ivars(());
        // SAFETY: NSObject's plain init.
        unsafe { msg_send![super(this), init] }
    };
    ns_app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    DELEGATE.with(|d| *d.borrow_mut() = Some(delegate));
    crate::menu::build(mtm);
    ns_app.run();
}

fn start(mtm: MainThreadMarker) {
    let wake = Arc::new(MacWake::default());
    let app = AppController::launch(wake.clone());
    let win = MainWindow::new(mtm);
    // Frames wake the shell like any event: the sink calls the same wake.
    let (head0, head1) = app.session().frame_sinks();
    head0.set_wake(wake.clone());
    head1.set_wake(wake.clone());
    win.window.makeKeyAndOrderFront(None);
    let empty = app.is_empty();
    SHELL.with(|s| *s.borrow_mut() = Some(Shell { app, win, wake, may_quit: false }));
    NSApplication::sharedApplication(mtm).activate();
    refresh();
    tick();
    // A test script: command ids, separated by `;`, one every few seconds
    // (`IRIS_APPKIT_SCRIPT_DELAY`, default 3). "select:NAME" selects a machine.
    if let Ok(script) = std::env::var("IRIS_APPKIT_SCRIPT") {
        SCRIPT.with(|q| q.borrow_mut().extend(script.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty())));
        schedule_script_step();
    }
    if empty && std::env::var_os("IRIS_APPKIT_SCRIPT").is_none() {
        // The first run: offer to make a machine.
        later(Box::new(|| shell_command(ShellCommand::NewMachine)));
    }
}

/// Run `f` from a main-queue block of its own, once the handler on the stack returned.
pub fn later(f: Box<dyn FnOnce()>) {
    LATER.with(|l| l.borrow_mut().push_back(f));
    dispatch2::DispatchQueue::main().exec_async(|| {
        let next = LATER.with(|l| l.borrow_mut().pop_front());
        if let Some(f) = next {
            f();
        }
    });
}

fn schedule_script_step() {
    let delay: f64 = std::env::var("IRIS_APPKIT_SCRIPT_DELAY").ok().and_then(|d| d.parse().ok()).unwrap_or(3.0);
    let t = crate::ui::target(mtm());
    // SAFETY: the target is never dropped; the selector takes one object argument.
    unsafe {
        let _: () = objc2::msg_send![&*t, performSelector: objc2::sel!(scriptStep:), withObject: std::ptr::null::<objc2::runtime::AnyObject>(), afterDelay: delay];
    }
}

/// Run the next scripted command id and schedule the one after.
pub fn script_step() {
    let Some(step) = SCRIPT.with(|q| q.borrow_mut().pop_front()) else { return };
    eprintln!("iris-appkit script: {step}");
    match step.strip_prefix("select:") {
        Some(name) => select_machine(name.to_string()),
        None => command(&step),
    }
    schedule_script_step();
}

/// A wake: drain the controller, carry out what it asks, present a new
/// frame, redraw.
pub fn tick() {
    let fx = with(|s| {
        s.wake.ticked();
        let fx = s.app.tick();
        for e in s.app.take_session_events() {
            if e == iris_gui_core::controller::session::SessionEvent::Started {
                s.win.display.clear();
            }
        }
        if s.app.session().is_running() {
            let (sink, _) = s.app.session().frame_sinks();
            if s.win.display.take(sink) {
                s.win.display.draw();
            }
        }
        fx
    });
    match fx {
        Some(fx) => {
            run_effects(fx);
            refresh();
        }
        // Busy: try again from a block of its own.
        None => later(Box::new(tick)),
    }
}

fn refresh() {
    let m = mtm();
    with(|s| s.win.refresh(m, &s.app));
}

/// Apply an app action, then carry out its effects with no borrow held.
pub fn dispatch(action: AppAction) {
    let Some(fx) = with(|s| s.app.apply(action)) else {
        return;
    };
    run_effects(fx);
    refresh();
}

fn run_effects(fx: Vec<Effect>) {
    if fx.is_empty() {
        return;
    }
    let out = crate::effects::run(mtm(), fx);
    if let Some(t) = out.toasts.last() {
        let t = t.clone();
        with(|s| s.win.toast(&t));
    }
    if out.open_settings.is_some() {
        shell_command(ShellCommand::Settings);
    }
    for a in out.answers {
        dispatch(AppAction::Answer(a));
    }
    if out.quit {
        with(|s| s.may_quit = true);
        NSApplication::sharedApplication(mtm()).terminate(None);
    }
}

/// A toolbar item, menu item or button with this command id was used.
pub fn command(id: &str) {
    let Some(routed) = with(|s| s.app.command(id)) else { return };
    run_effects(routed.effects);
    if let Some(c) = routed.shell {
        shell_command(c);
    }
    refresh();
}

/// Input and capture for the display: applied at once, without a full
/// redraw (motion arrives many times a second); the view follows capture.
pub fn display(action: iris_gui_core::controller::display::DisplayAction) {
    let m = mtm();
    let fx = with(|s| {
        let before = (s.app.display().captured(), s.app.display().fullscreen(), s.app.display().detached());
        let fx = s.app.apply(AppAction::Display(action));
        let after = (s.app.display().captured(), s.app.display().fullscreen(), s.app.display().detached());
        if before != after {
            s.win.sync_display(m, &s.app);
        }
        fx
    });
    if let Some(fx) = fx {
        run_effects(fx);
    }
}

/// A key pressed while the display is not captured: the key map decides.
pub fn shortcut(key: iris_gui_core::controller::keys::Key, mods: iris_gui_core::controller::keys::Mods) {
    let keys = iris_gui_core::controller::keys::KeyMap::default();
    if let Some(cmd) = keys.lookup(key, mods) {
        command(iris_gui_core::controller::commands::id_of(cmd));
    }
}

/// Whether a menu or toolbar item with this id is enabled.
pub fn enabled(id: &str) -> bool {
    with(|s| s.app.command_enabled(id)).map_or(true, |e| e == Enabled::Yes)
}

pub fn select_machine(name: String) {
    dispatch(AppAction::Select(name));
}

/// The commands that only change the shell's own views. Later slices fill
/// these in (settings sheet, browser, benchmark); for now they say so.
fn shell_command(c: ShellCommand) {
    match c {
        ShellCommand::ToggleLibrary => {
            with(|s| s.win.toggle_library());
        }
        ShellCommand::CloseWindow => {
            with(|s| s.win.window.performClose(None));
        }
        ShellCommand::About => {
            let text = iris_gui_core::controller::debug::diagnostics_text();
            run_effects(vec![Effect::Alert {
                severity: iris_gui_core::controller::Severity::Info,
                title: t!("app-name"),
                message: text,
            }]);
        }
        ShellCommand::Settings
        | ShellCommand::SavedStates
        | ShellCommand::BrowseFiles
        | ShellCommand::BrowseDisk(_)
        | ShellCommand::NewMachine
        | ShellCommand::AppSettings
        | ShellCommand::Benchmark
        | ShellCommand::Diagnostics => {
            with(|s| s.win.toast(&t!("appkit-not-yet")));
        }
    }
}
