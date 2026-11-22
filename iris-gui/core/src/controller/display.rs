//! The guest display: input capture and its chords, full screen, the detached
//! window, and how the picture is scaled.
//!
//! The shell turns its native key events into physical [`GuestKey`]s (never
//! layout-translated: clanker/gui/keyboard-layout-send-physical-keys-not-logical.md)
//! and its pointer into deltas, and sends them here only while
//! [`DisplayController::captured`]. The rules for what a key does while
//! captured (the release chord, Ctrl+Alt+Esc, F11) live here, once.

use std::time::{Duration, Instant};

pub use winit::keyboard::KeyCode as GuestKey;
/// The guest's frames as the shell's presenter takes them.
pub use crate::model::framebuffer::{Frame, FrameSink};

use super::keys::{CaptureChords, ModKey};
use super::Effect;
use crate::model::settings::{DisplayFilter, DisplayMode, GuiSettings};

/// How long the window must stay unfocused before a capture is released. Long
/// enough to ride out macOS's focus flicker around cursor grabs, short enough
/// that a real app switch frees the cursor promptly.
pub const FOCUS_LOSS_GRACE: Duration = Duration::from_millis(300);

/// Display intents. Key and pointer input arrive only while captured.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayAction {
    /// Route the keyboard and mouse to the guest (a click on the display).
    Capture,
    Release,
    /// The window gained or lost focus.
    Focus(bool),
    /// A physical key went down or up.
    Key { key: GuestKey, down: bool, repeat: bool },
    /// Relative motion in points, the buttons held (bit 0 left, 1 right,
    /// 2 middle, 3 and 4 the side buttons), and the wheel in lines and points.
    Pointer { dx: f32, dy: f32, buttons: u8, wheel_lines: f32, wheel_points: f32 },
    ToggleFullScreen,
    /// The window left full screen by itself (the green button, Esc in AppKit).
    FullScreenChanged(bool),
    ToggleDetached,
    SetMode(DisplayMode),
    /// The magnification in Scale mode, guest pixels to points.
    SetScale(f32),
    SetIntegerOnly(bool),
    SetFilter(DisplayFilter),
    /// Send a key chord the host would otherwise keep (View > Send Key).
    Send(GuestChord),
}

/// Chords the Send Key menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestChord {
    F11,
    CtrlAltDel,
    Escape,
}

/// Capture, full screen and the detached window.
#[derive(Debug)]
pub struct DisplayController {
    captured: bool,
    fullscreen: bool,
    detached: bool,
    chords: CaptureChords,
    held_mods: Vec<GuestKey>,
    chord_consumed: bool,
    last_buttons: u8,
    unfocused_since: Option<Instant>,
    /// Sub-unit motion and wheel carried to the next event.
    carry: (f32, f32, f32),
}

impl Default for DisplayController {
    fn default() -> Self {
        Self {
            captured: false,
            fullscreen: false,
            detached: false,
            chords: CaptureChords::default(),
            held_mods: Vec::new(),
            chord_consumed: false,
            last_buttons: 0,
            unfocused_since: None,
            carry: (0.0, 0.0, 0.0),
        }
    }
}

fn mod_key(k: ModKey) -> GuestKey {
    match k {
        ModKey::ControlLeft => GuestKey::ControlLeft,
        ModKey::ControlRight => GuestKey::ControlRight,
        ModKey::AltLeft => GuestKey::AltLeft,
        ModKey::AltRight => GuestKey::AltRight,
        ModKey::ShiftLeft => GuestKey::ShiftLeft,
        ModKey::ShiftRight => GuestKey::ShiftRight,
        ModKey::SuperLeft => GuestKey::SuperLeft,
        ModKey::SuperRight => GuestKey::SuperRight,
    }
}

fn is_modifier(k: GuestKey) -> bool {
    matches!(
        k,
        GuestKey::ShiftLeft
            | GuestKey::ShiftRight
            | GuestKey::ControlLeft
            | GuestKey::ControlRight
            | GuestKey::AltLeft
            | GuestKey::AltRight
            | GuestKey::SuperLeft
            | GuestKey::SuperRight
    )
}

/// Where the guest's input goes: the live PS/2 controller, or nowhere.
pub trait GuestInput {
    fn key(&self, key: GuestKey, down: bool);
    fn mouse(&self, buttons: u8, dx: i32, dy: i32, dz: i32);
}

impl GuestInput for iris::dev::ps2::Ps2Controller {
    fn key(&self, key: GuestKey, down: bool) {
        self.push_kb(key, down);
    }
    fn mouse(&self, buttons: u8, dx: i32, dy: i32, dz: i32) {
        self.push_mouse_input(buttons, dx, dy, dz);
    }
}

impl DisplayController {
    pub fn with_chords(chords: CaptureChords) -> Self {
        Self { chords, ..Self::default() }
    }

    /// Input goes to the guest: the shell hides and locks the host cursor.
    pub fn captured(&self) -> bool {
        self.captured
    }

    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub fn detached(&self) -> bool {
        self.detached
    }

    /// The hint shown while captured: how to get the mouse back.
    pub fn release_hint(&self) -> String {
        t!("main-capture-release", keys = CaptureChords::release_text(cfg!(target_os = "macos")))
    }

    fn release_keys(&self) -> [GuestKey; 2] {
        [mod_key(self.chords.release[0]), mod_key(self.chords.release[1])]
    }

    fn chord_held(&self) -> bool {
        self.release_keys().iter().all(|k| self.held_mods.contains(k))
    }

    fn ctrl_alt_held(&self) -> bool {
        let ctrl = self.held_mods.iter().any(|k| matches!(k, GuestKey::ControlLeft | GuestKey::ControlRight));
        let alt = self.held_mods.iter().any(|k| matches!(k, GuestKey::AltLeft | GuestKey::AltRight));
        ctrl && alt
    }

    fn engage(&mut self) {
        if self.captured {
            return;
        }
        self.captured = true;
        self.held_mods.clear();
        self.chord_consumed = false;
        self.last_buttons = 0;
        self.unfocused_since = None;
        self.carry = (0.0, 0.0, 0.0);
    }

    /// Release, lifting every modifier the guest still thinks is held.
    fn release(&mut self, guest: Option<&dyn GuestInput>) {
        if !self.captured {
            return;
        }
        for k in self.held_mods.drain(..) {
            if let Some(g) = guest {
                g.key(k, false);
            }
        }
        self.captured = false;
        self.chord_consumed = false;
        self.last_buttons = 0;
        self.unfocused_since = None;
    }

    /// The machine stopped or halted: let go of the cursor without touching
    /// the guest (it may be gone).
    pub(crate) fn force_release(&mut self) {
        self.held_mods.clear();
        self.release(None);
    }

    /// Handle one action. `guest` is the running machine's input, when there
    /// is one; `prefs` holds the app-wide display settings (D22);
    /// `wheel_px_per_line` is the machine's `mouse_scroll_pixels_per_line`.
    pub fn apply(
        &mut self,
        action: DisplayAction,
        guest: Option<&dyn GuestInput>,
        prefs: &mut GuiSettings,
        wheel_px_per_line: f64,
    ) -> Vec<Effect> {
        let mut fx = Vec::new();
        match action {
            DisplayAction::Capture => {
                if guest.is_some() {
                    self.engage();
                }
            }
            DisplayAction::Release => self.release(guest),
            DisplayAction::Focus(true) => self.unfocused_since = None,
            DisplayAction::Focus(false) => {
                if self.captured {
                    self.unfocused_since.get_or_insert_with(Instant::now);
                }
            }
            DisplayAction::Key { key, down, repeat } => {
                if !self.captured {
                    return fx;
                }
                let Some(g) = guest else { return fx };
                self.key(g, key, down, repeat);
            }
            DisplayAction::Pointer { dx, dy, buttons, wheel_lines, wheel_points } => {
                if !self.captured {
                    return fx;
                }
                let Some(g) = guest else { return fx };
                let per_line = if wheel_px_per_line > 0.0 { wheel_px_per_line as f32 } else { 1.0 };
                self.carry.0 += dx;
                self.carry.1 += dy;
                self.carry.2 += wheel_lines + wheel_points / per_line;
                let (mx, my, mz) = (self.carry.0.trunc(), self.carry.1.trunc(), self.carry.2.trunc());
                self.carry = (self.carry.0 - mx, self.carry.1 - my, self.carry.2 - mz);
                if buttons != self.last_buttons || mx != 0.0 || my != 0.0 || mz != 0.0 {
                    g.mouse(buttons, mx as i32, my as i32, mz as i32);
                    self.last_buttons = buttons;
                }
            }
            DisplayAction::ToggleFullScreen => self.fullscreen = !self.fullscreen,
            DisplayAction::FullScreenChanged(on) => self.fullscreen = on,
            DisplayAction::ToggleDetached => self.detached = !self.detached,
            DisplayAction::SetMode(m) => {
                prefs.display_mode = m;
                save(prefs, &mut fx);
            }
            DisplayAction::SetScale(s) => {
                prefs.vm_scale = s.clamp(0.25, 4.0);
                prefs.display_mode = DisplayMode::Scale;
                save(prefs, &mut fx);
            }
            DisplayAction::SetIntegerOnly(on) => {
                prefs.integer_only = on;
                save(prefs, &mut fx);
            }
            DisplayAction::SetFilter(f) => {
                prefs.display_filter = f;
                save(prefs, &mut fx);
            }
            DisplayAction::Send(chord) => {
                let Some(g) = guest else { return fx };
                // Lift what the guest holds so the chord arrives clean, then
                // put it back.
                let held = self.held_mods.clone();
                for k in &held {
                    g.key(*k, false);
                }
                let keys: &[GuestKey] = match chord {
                    GuestChord::F11 => &[GuestKey::F11],
                    GuestChord::Escape => &[GuestKey::Escape],
                    GuestChord::CtrlAltDel => &[GuestKey::ControlLeft, GuestKey::AltLeft, GuestKey::Delete],
                };
                for k in keys {
                    g.key(*k, true);
                }
                for k in keys.iter().rev() {
                    g.key(*k, false);
                }
                for k in &held {
                    g.key(*k, true);
                }
            }
        }
        fx
    }

    fn key(&mut self, g: &dyn GuestInput, key: GuestKey, down: bool, repeat: bool) {
        // Ctrl+Alt+Esc releases at once, the explicit fallback for the chord.
        if key == GuestKey::Escape && down && self.ctrl_alt_held() {
            self.release(Some(g));
            return;
        }
        if key == GuestKey::F11 {
            // Plain F11 toggles full screen (the shell's key map); Ctrl+Alt+F11
            // is the only way to send F11 to the guest, and it arrives bare.
            if down && !repeat && self.ctrl_alt_held() {
                self.chord_consumed = true;
                let held = self.held_mods.clone();
                for k in &held {
                    g.key(*k, false);
                }
                g.key(GuestKey::F11, true);
                g.key(GuestKey::F11, false);
                for k in &held {
                    g.key(*k, true);
                }
            } else if down && !repeat {
                self.fullscreen = !self.fullscreen;
            }
            return;
        }
        let was_chord = self.chord_held();
        if is_modifier(key) {
            if down {
                if !self.held_mods.contains(&key) {
                    self.held_mods.push(key);
                }
            } else {
                self.held_mods.retain(|k| *k != key);
            }
        } else if down && was_chord {
            self.chord_consumed = true;
        }
        let now_chord = self.chord_held();
        if now_chord && !was_chord {
            self.chord_consumed = false;
        }
        g.key(key, down);
        // The release chord fires on key-up, and only when nothing else was
        // pressed while it was held (so Ctrl+Alt+F11 still reaches the guest).
        if was_chord && !now_chord && !self.chord_consumed {
            self.release(Some(g));
        }
    }

    /// Release after a real focus loss; the grace rides out macOS's flicker.
    pub(crate) fn tick(&mut self, guest: Option<&dyn GuestInput>) {
        if self.captured && self.unfocused_since.is_some_and(|t| t.elapsed() >= FOCUS_LOSS_GRACE) {
            self.release(guest);
        }
    }
}

fn save(prefs: &mut GuiSettings, fx: &mut Vec<Effect>) {
    if let Err(e) = prefs.save() {
        fx.push(Effect::Toast(t!("toast-settings-save-failed", error = e)));
    }
}

/// How large to draw a guest picture of `px` pixels in `avail` points on a
/// screen of `ppp` device pixels per point, and whether to sample it nearest
/// (crisp) rather than linear.
///
/// `max_scale` (points per guest pixel) caps the picture: a window with room
/// for more centres the display at that scale instead of a fractional blow-up;
/// 0 means fit the window. A size within `INTEGER_SNAP` above a whole number of
/// device pixels per guest pixel snaps down to it, never up (overhanging the
/// view would clip rows). `integer_only` always snaps down to a whole multiple
/// when one fits.
pub fn draw_size(avail: (f32, f32), px: (f32, f32), ppp: f32, max_scale: f32, integer_only: bool) -> ((f32, f32), bool) {
    let aspect = px.0 / px.1;
    let mut fit = if avail.0 / avail.1 > aspect { (avail.1 * aspect, avail.1) } else { (avail.0, avail.0 / aspect) };
    if px.0 < 1.0 || px.1 < 1.0 || ppp <= 0.0 {
        return (fit, true);
    }
    if max_scale > 0.0 && fit.1 > px.1 * max_scale {
        fit = (px.0 * max_scale, px.1 * max_scale);
    }
    let dev = fit.1 * ppp / px.1;
    let n = (dev + 1e-3).floor();
    if n >= 1.0 && (integer_only || dev - n <= INTEGER_SNAP) {
        ((px.0 * n / ppp, px.1 * n / ppp), true)
    } else {
        (fit, false)
    }
}

/// How far above a whole device-pixel scale a fit may be and still be drawn at
/// it: makes an integer scale sticky when the window is a little too big.
pub const INTEGER_SNAP: f32 = 0.02;

/// The scale cap and sampling for the current preferences, as `draw_size`
/// takes them: `(max_scale, integer_only)` and whether a non-integer size is
/// drawn nearest anyway (a forced filter).
pub fn scaling(prefs: &GuiSettings, zoom: f32) -> (f32, bool) {
    let max = match prefs.display_mode {
        DisplayMode::Fit => 0.0,
        DisplayMode::Scale => prefs.vm_scale / zoom.max(0.1),
    };
    (max, prefs.integer_only)
}

/// Whether to sample nearest, given the filter preference and whether the
/// size is a whole number of device pixels per guest pixel.
pub fn nearest(prefs: &GuiSettings, integer: bool) -> bool {
    match prefs.display_filter {
        DisplayFilter::Auto => integer,
        DisplayFilter::Nearest => true,
        DisplayFilter::Linear => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Rec(RefCell<Vec<(GuestKey, bool)>>);
    impl GuestInput for Rec {
        fn key(&self, key: GuestKey, down: bool) {
            self.0.borrow_mut().push((key, down));
        }
        fn mouse(&self, _: u8, _: i32, _: i32, _: i32) {}
    }

    fn pc() -> DisplayController {
        DisplayController::with_chords(CaptureChords::for_platform(false))
    }

    fn key(d: &mut DisplayController, g: &Rec, k: GuestKey, down: bool) {
        let mut prefs = GuiSettings::default();
        d.apply(DisplayAction::Key { key: k, down, repeat: false }, Some(g), &mut prefs, 1.0);
    }

    #[test]
    fn the_release_chord_fires_on_key_up_when_nothing_else_was_pressed() {
        let g = Rec::default();
        let mut d = pc();
        let mut prefs = GuiSettings::default();
        d.apply(DisplayAction::Capture, Some(&g), &mut prefs, 1.0);
        key(&mut d, &g, GuestKey::ControlLeft, true);
        key(&mut d, &g, GuestKey::AltLeft, true);
        assert!(d.captured());
        key(&mut d, &g, GuestKey::AltLeft, false);
        assert!(!d.captured(), "chord released");
        // Ctrl was still held in the guest: the release lifted it.
        assert_eq!(g.0.borrow().last(), Some(&(GuestKey::ControlLeft, false)));
    }

    #[test]
    fn ctrl_alt_f11_sends_a_bare_f11_and_keeps_capture() {
        let g = Rec::default();
        let mut d = pc();
        let mut prefs = GuiSettings::default();
        d.apply(DisplayAction::Capture, Some(&g), &mut prefs, 1.0);
        key(&mut d, &g, GuestKey::ControlLeft, true);
        key(&mut d, &g, GuestKey::AltLeft, true);
        g.0.borrow_mut().clear();
        key(&mut d, &g, GuestKey::F11, true);
        let sent = g.0.borrow().clone();
        assert_eq!(
            sent,
            [
                (GuestKey::ControlLeft, false),
                (GuestKey::AltLeft, false),
                (GuestKey::F11, true),
                (GuestKey::F11, false),
                (GuestKey::ControlLeft, true),
                (GuestKey::AltLeft, true),
            ]
        );
        key(&mut d, &g, GuestKey::AltLeft, false);
        key(&mut d, &g, GuestKey::ControlLeft, false);
        assert!(d.captured(), "a used chord does not release");
        assert!(!d.fullscreen());
    }

    #[test]
    fn plain_f11_toggles_full_screen_and_ctrl_alt_esc_releases() {
        let g = Rec::default();
        let mut d = pc();
        let mut prefs = GuiSettings::default();
        d.apply(DisplayAction::Capture, Some(&g), &mut prefs, 1.0);
        key(&mut d, &g, GuestKey::F11, true);
        assert!(d.fullscreen());
        assert!(g.0.borrow().is_empty(), "plain F11 never reaches the guest");
        key(&mut d, &g, GuestKey::ControlRight, true);
        key(&mut d, &g, GuestKey::AltRight, true);
        key(&mut d, &g, GuestKey::Escape, true);
        assert!(!d.captured());
    }

    #[test]
    fn focus_loss_releases_after_the_grace() {
        let g = Rec::default();
        let mut d = pc();
        let mut prefs = GuiSettings::default();
        d.apply(DisplayAction::Capture, Some(&g), &mut prefs, 1.0);
        d.apply(DisplayAction::Focus(false), Some(&g), &mut prefs, 1.0);
        d.tick(Some(&g));
        assert!(d.captured(), "a flicker does not release");
        d.unfocused_since = Some(Instant::now() - FOCUS_LOSS_GRACE);
        d.tick(Some(&g));
        assert!(!d.captured());
    }

    #[test]
    fn no_guest_no_capture() {
        let mut d = pc();
        let mut prefs = GuiSettings::default();
        d.apply(DisplayAction::Capture, None, &mut prefs, 1.0);
        assert!(!d.captured());
    }

    const FB: (f32, f32) = (1280.0, 1024.0);

    #[test]
    fn exact_fit_is_integer_and_nearest() {
        assert_eq!(draw_size(FB, FB, 1.0, 0.0, false), (FB, true));
        assert_eq!(draw_size(FB, FB, 2.0, 0.0, false), (FB, true));
        assert_eq!(draw_size((2560.0, 2048.0), FB, 1.0, 0.0, false), ((2560.0, 2048.0), true));
    }

    #[test]
    fn slightly_large_window_snaps_down_never_up() {
        assert_eq!(draw_size((1290.0, 1034.0), FB, 1.0, 0.0, false), (FB, true));
        let (size, nearest) = draw_size((1280.0, 1020.0), FB, 1.0, 0.0, false);
        assert!(!nearest);
        assert!(size.1 <= 1020.0);
    }

    #[test]
    fn fractional_fit_is_filtered_unless_integer_only() {
        assert_eq!(draw_size((1600.0, 1280.0), FB, 1.0, 0.0, false), ((1600.0, 1280.0), false));
        assert_eq!(draw_size((1600.0, 1280.0), FB, 1.0, 0.0, true), (FB, true));
    }

    #[test]
    fn larger_window_is_capped_at_the_scale() {
        assert_eq!(draw_size((1734.0, 1080.0), FB, 1.0, 1.0, false), (FB, true));
        assert_eq!(draw_size((3000.0, 2000.0), FB, 2.0, 1.0, false), (FB, true));
        assert_eq!(draw_size((3000.0, 2000.0), FB, 2.0, 0.5, false), ((640.0, 512.0), true));
        let (size, nearest) = draw_size((1000.0, 800.0), FB, 1.0, 1.0, false);
        assert!(!nearest);
        assert!(size.1 <= 800.0);
    }
}
