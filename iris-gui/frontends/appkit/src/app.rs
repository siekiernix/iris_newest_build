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
    settings: Option<crate::settings::SettingsWindow>,
    new_machine: Option<crate::new_machine::NewMachineWindow>,
    browser: Option<crate::browser::BrowserWindow>,
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
    SHELL.with(|s| *s.borrow_mut() = Some(Shell { app, win, wake, may_quit: false, settings: None, new_machine: None, browser: None }));
    NSApplication::sharedApplication(mtm).activate();
    refresh();
    tick();
    // A test script: command ids, separated by `;`, one every few seconds
    // (`IRIS_APPKIT_SCRIPT_DELAY`, default 3). "select:NAME" selects a machine,
    // "page:ID" shows a settings section, "set:ID=VALUE" reports a settings control,
    // "open:N" and "pick:N,M" open and select rows of the disk browser.
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
    if let Some(name) = step.strip_prefix("select:") {
        select_machine(name.to_string());
    } else if let Some((id, v)) = step.strip_prefix("set:").and_then(|s| s.split_once('=')) {
        // set:ID=i:N | b:0/1 | t:TEXT | n:NUMBER, as the control with ID would report.
        use iris_gui_core::controller::settings::Value;
        let value = match v.split_once(':') {
            Some(("i", n)) => n.parse().ok().map(Value::Index),
            Some(("b", b)) => Some(Value::Bool(b == "1")),
            Some(("t", t)) => Some(Value::Text(t.to_string())),
            Some(("n", n)) => n.parse().ok().map(Value::Number),
            _ => None,
        };
        if let Some(value) = value {
            setting(id.to_string(), value);
        }
    } else if let Some(row) = step.strip_prefix("open:").and_then(|r| r.parse().ok()) {
        browser_activate(row);
    } else if let Some(rows) = step.strip_prefix("pick:") {
        browser_select(rows.split(',').filter_map(|r| r.parse().ok()).collect());
    } else if let Some(page) = step.strip_prefix("page:") {
        setting(format!("settings-page:{page}"), iris_gui_core::controller::settings::Value::Bool(true));
    } else {
        command(&step);
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
    let due = with(|s| {
        s.win.refresh(m, &s.app);
        if let Some(w) = &mut s.settings {
            w.refresh(m, &s.app);
        }
        // The browser window is shown exactly while the controller has a browser.
        match (s.app.browser(), &mut s.browser) {
            (Some(b), Some(w)) => w.refresh(m, b),
            (Some(b), None) => {
                let mut w = crate::browser::BrowserWindow::new(m);
                w.refresh(m, b);
                w.window.makeKeyAndOrderFront(None);
                s.browser = Some(w);
            }
            (None, Some(_)) => {
                if let Some(mut w) = s.browser.take() {
                    w.close();
                }
            }
            (None, None) => {}
        }
        s.app.autosave_due_in()
    })
    .flatten();
    // Nothing wakes a stopped machine's shell, so the autosave pause needs a
    // tick of its own. Each edit asks again; a tick before it is due is harmless.
    if let Some(d) = due {
        let t = crate::ui::target(m);
        // SAFETY: the target is never dropped; the selector takes one object argument.
        unsafe {
            let _: () = objc2::msg_send![&*t, performSelector: objc2::sel!(tickNow:), withObject: std::ptr::null::<objc2::runtime::AnyObject>(), afterDelay: d.as_secs_f64() + 0.05];
        }
    }
}

/// A settings control reported a new value.
pub fn setting(id: String, value: iris_gui_core::controller::settings::Value) {
    if id.starts_with("new-machine.") {
        return new_machine_control(id, value);
    }
    if id.starts_with("browser.") {
        let on = matches!(value, iris_gui_core::controller::settings::Value::Bool(true));
        return browser_control(&id, on);
    }
    enum Then {
        Page,
        Act(iris_gui_core::controller::settings::SettingsAction),
    }
    let then = with(|s| {
        let w = s.settings.as_mut()?;
        if let Some(page) = w.page_for(&id) {
            w.show_page(page);
            return Some(Then::Page);
        }
        w.action(&id, value).map(Then::Act)
    })
    .flatten();
    match then {
        Some(Then::Page) => refresh(),
        Some(Then::Act(a)) => dispatch(AppAction::Settings(a)),
        None => {}
    }
}

/// Open a fresh New Machine window.
fn open_new_machine() {
    let m = mtm();
    let w = with(|s| {
        if let Some(old) = s.new_machine.take() {
            old.window.orderOut(None);
        }
        let w = crate::new_machine::NewMachineWindow::new(m);
        let win = w.window.clone();
        s.new_machine = Some(w);
        win
    });
    if let Some(w) = w {
        w.makeKeyAndOrderFront(None);
    }
}

/// A control in the New Machine window reported a value.
fn new_machine_control(id: String, value: iris_gui_core::controller::settings::Value) {
    use crate::new_machine::Next;
    let m = mtm();
    let Some(next) = with(|s| s.new_machine.as_mut().map(|w| w.control(&id, value))).flatten() else { return };
    match next {
        Next::Nothing => {}
        Next::Redraw => {
            with(|s| s.new_machine.as_mut().map(|w| w.redraw(m)));
        }
        Next::Pick(e) => {
            // The picker runs here, with no borrow held; its answer is the window's.
            let out = crate::effects::run(m, vec![e]);
            with(|s| {
                if let Some(w) = s.new_machine.as_mut() {
                    if out.answers.iter().any(|a| w.spec.picked(a)) {
                        w.redraw(m);
                    }
                }
            });
        }
        Next::Close => {
            if let Some(w) = with(|s| s.new_machine.take()).flatten() {
                w.window.orderOut(None);
            }
        }
        Next::Create(action) => {
            if let Some(w) = with(|s| s.new_machine.take()).flatten() {
                w.window.orderOut(None);
            }
            dispatch(action);
        }
    }
}

/// Open the settings window at `page` (a section id), or bring it forward.
fn open_settings(page: Option<&'static str>) {
    let m = mtm();
    let w = with(|s| {
        let w = s.settings.get_or_insert_with(|| {
            crate::settings::SettingsWindow::new(m, page.unwrap_or(iris_gui_core::controller::ids::settings::GENERAL))
        });
        if let Some(p) = page {
            w.show_page(p);
        }
        w.window.clone()
    });
    refresh();
    if let Some(w) = w {
        w.makeKeyAndOrderFront(None);
    }
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
    if let Some(page) = out.open_settings {
        open_settings(Some(page));
    }
    for a in out.answers {
        dispatch(AppAction::Answer(a));
    }
    if out.quit {
        with(|s| s.may_quit = true);
        NSApplication::sharedApplication(mtm()).terminate(None);
    }
}

/// A browser control: its action, with the editor's text first for Save and Close.
fn browser_control(id: &str, value: bool) {
    use iris_gui_core::controller::disk_browser::{action_for, BrowserAction};
    let Some(action) = action_for(id, value) else { return };
    if matches!(action, BrowserAction::EditorSave | BrowserAction::EditorClose) {
        if let Some(text) = with(|s| s.browser.as_ref().and_then(|w| w.editor_text())).flatten() {
            let changed = with(|s| s.app.browser().and_then(|b| b.editor()).is_some_and(|e| e.text != text)).unwrap_or(false);
            if changed {
                dispatch(AppAction::Browser(BrowserAction::EditorChanged(text)));
            }
        }
    }
    dispatch(AppAction::Browser(action));
}

/// Rows selected in the browser's table.
pub fn browser_select(rows: Vec<usize>) {
    dispatch(AppAction::Browser(iris_gui_core::controller::disk_browser::BrowserAction::Select(rows)));
}

/// A row of the browser's table was double-clicked.
pub fn browser_activate(row: usize) {
    dispatch(AppAction::Browser(iris_gui_core::controller::disk_browser::BrowserAction::Activate(row)));
}

/// The browser window's close button.
pub fn close_browser() {
    dispatch(AppAction::CloseBrowser);
}

/// A toolbar item, menu item or button with this command id was used.
pub fn command(id: &str) {
    if id.starts_with("browser.") {
        return browser_control(id, false);
    }
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
            // The front window: the settings window as well as the main one.
            match NSApplication::sharedApplication(mtm()).keyWindow() {
                Some(w) => w.performClose(None),
                None => {
                    with(|s| s.win.window.performClose(None));
                }
            }
        }
        ShellCommand::About => {
            let text = iris_gui_core::controller::debug::diagnostics_text();
            run_effects(vec![Effect::Alert {
                severity: iris_gui_core::controller::Severity::Info,
                title: t!("app-name"),
                message: text,
            }]);
        }
        ShellCommand::Settings => open_settings(None),
        ShellCommand::NewMachine => open_new_machine(),
        ShellCommand::SavedStates => {
            let states = with(|s| s.app.saved_states()).unwrap_or_default();
            crate::menu::saved_states(mtm(), &states);
        }
        ShellCommand::AppSettings
        | ShellCommand::Benchmark
        | ShellCommand::Diagnostics => {
            with(|s| s.win.toast(&t!("appkit-not-yet")));
        }
    }
}
