//! The stopped machine's page, from the app controller's `Overview`: its name
//! and model, its last screen with a large Start button over it, then Details
//! (warnings and the hardware) or Saved States (a timeline: the current state,
//! then each saved state with its picture). The toolbar holds everything else.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2::Message;
use objc2_app_kit::{
    NSBezelStyle, NSBox, NSBoxType, NSButton, NSColor, NSImage, NSImageScaling, NSImageView, NSLayoutAttribute,
    NSSegmentedControl, NSStackView, NSTextField, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::NSArray;
use iris_gui_core::controller::icons::Icon;
use iris_gui_core::controller::ids::{item, toolbar as tb};
use iris_gui_core::controller::overview::Overview;
use iris_gui_core::controller::session::Picture;
use iris_gui_core::controller::Severity;

use crate::glyphs;
use crate::ui::{button, heading, label, ns, secondary, set_id};

/// The Details / Saved States switch (reported through `app::setting`).
pub const TAB: &str = "summary.tab";
const WIDTH: f64 = 640.0;
/// The preview's height: a wide frame, as UTM's, that leaves room below.
const PREVIEW_H: f64 = 340.0;

fn view<T: objc2::Message + AsRef<NSView>>(v: &Retained<T>) -> Retained<NSView> {
    let r: &NSView = (**v).as_ref();
    r.retain()
}

fn stack(mtm: MainThreadMarker, views: &[Retained<NSView>], vertical: bool, spacing: f64) -> Retained<NSStackView> {
    let s = NSStackView::stackViewWithViews(&NSArray::from_retained_slice(views), mtm);
    s.setOrientation(if vertical {
        NSUserInterfaceLayoutOrientation::Vertical
    } else {
        NSUserInterfaceLayoutOrientation::Horizontal
    });
    s.setSpacing(spacing);
    s.setAlignment(if vertical { NSLayoutAttribute::Leading } else { NSLayoutAttribute::CenterY });
    s
}

fn icon_button(mtm: MainThreadMarker, title: &str, id: &str, icon: Icon) -> Retained<NSButton> {
    let b = button(mtm, title, id);
    b.setBezelStyle(NSBezelStyle::Push);
    b.setToolTip(Some(&ns(title)));
    if let Some(img) = glyphs::icon(icon, title) {
        b.setImage(Some(&img));
        b.setTitle(&ns(""));
    }
    b
}

fn load(p: &Picture) -> Option<Retained<NSImage>> {
    NSImage::initWithContentsOfFile(objc2::AnyThread::alloc(), &ns(&p.path.to_string_lossy()))
}

/// A picture in a frame of `w` x `h`, or the machine's glyph when there is none.
fn picture(mtm: MainThreadMarker, p: Option<&Picture>, w: f64, h: f64, fallback: &str) -> Retained<NSImageView> {
    let iv = NSImageView::new(mtm);
    iv.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    match p.and_then(load) {
        Some(img) => iv.setImage(Some(&img)),
        None => {
            if let Some(img) = glyphs::symbol(fallback, "") {
                iv.setImage(Some(&img));
                iv.setImageScaling(NSImageScaling::ScaleNone);
                iv.setSymbolConfiguration(Some(&objc2_app_kit::NSImageSymbolConfiguration::configurationWithPointSize_weight(
                    (h / 3.0).min(64.0),
                    0.0,
                )));
                iv.setContentTintColor(Some(&NSColor::tertiaryLabelColor()));
            }
        }
    }
    iv.widthAnchor().constraintEqualToConstant(w).setActive(true);
    iv.heightAnchor().constraintEqualToConstant(h).setActive(true);
    iv
}

/// The preview with a large Start button over its centre, in a dark frame
/// (the guest's screen, or nothing yet).
fn preview(mtm: MainThreadMarker, o: &Overview) -> Retained<NSView> {
    let h = PREVIEW_H;
    let holder = NSView::new(mtm);
    // The dark frame: a filled box behind the picture and the button.
    let frame = NSBox::new(mtm);
    frame.setBoxType(NSBoxType::Custom);
    frame.setBorderWidth(0.0);
    frame.setCornerRadius(8.0);
    frame.setFillColor(&NSColor::colorWithWhite_alpha(0.0, 0.85));
    frame.setTitlePosition(objc2_app_kit::NSTitlePosition::NoTitle);
    frame.setTranslatesAutoresizingMaskIntoConstraints(false);
    holder.addSubview(&frame);
    for (a, b) in [
        (frame.leadingAnchor(), holder.leadingAnchor()),
        (frame.trailingAnchor(), holder.trailingAnchor()),
    ] {
        a.constraintEqualToAnchor(&b).setActive(true);
    }
    frame.topAnchor().constraintEqualToAnchor(&holder.topAnchor()).setActive(true);
    frame.bottomAnchor().constraintEqualToAnchor(&holder.bottomAnchor()).setActive(true);
    let pic = match o.preview.as_ref() {
        Some(p) => picture(mtm, Some(p), WIDTH, h, ""),
        None => {
            let iv = NSImageView::new(mtm);
            iv.widthAnchor().constraintEqualToConstant(WIDTH).setActive(true);
            iv.heightAnchor().constraintEqualToConstant(h).setActive(true);
            iv
        }
    };
    holder.setTranslatesAutoresizingMaskIntoConstraints(false);
    pic.setTranslatesAutoresizingMaskIntoConstraints(false);
    holder.addSubview(&pic);
    holder.widthAnchor().constraintEqualToConstant(WIDTH).setActive(true);
    holder.heightAnchor().constraintEqualToConstant(h).setActive(true);
    pic.centerXAnchor().constraintEqualToAnchor(&holder.centerXAnchor()).setActive(true);
    pic.centerYAnchor().constraintEqualToAnchor(&holder.centerYAnchor()).setActive(true);
    let start = t!("tb-power-on");
    let play = button(mtm, "", tb::POWER_ON);
    play.setBordered(false);
    play.setToolTip(Some(&ns(&start)));
    match glyphs::symbol("play.circle.fill", &start) {
        Some(img) => {
            play.setImage(Some(&img));
            play.setSymbolConfiguration(Some(&objc2_app_kit::NSImageSymbolConfiguration::configurationWithPointSize_weight(
                96.0, 0.0,
            )));
            // White over the dark frame, as UTM's.
            play.setContentTintColor(Some(&NSColor::colorWithWhite_alpha(1.0, 0.9)));
        }
        None => play.setTitle(&ns(&start)),
    }
    play.setKeyEquivalent(&ns("\r"));
    play.setTranslatesAutoresizingMaskIntoConstraints(false);
    holder.addSubview(&play);
    play.centerXAnchor().constraintEqualToAnchor(&holder.centerXAnchor()).setActive(true);
    play.centerYAnchor().constraintEqualToAnchor(&holder.centerYAnchor()).setActive(true);
    holder
}

fn rule(mtm: MainThreadMarker, w: f64) -> Retained<NSView> {
    let b = NSBox::new(mtm);
    b.setBoxType(NSBoxType::Separator);
    b.widthAnchor().constraintEqualToConstant(w).setActive(true);
    view(&b)
}

fn details(mtm: MainThreadMarker, o: &Overview) -> Vec<Retained<NSView>> {
    let mut rows: Vec<Retained<NSView>> = Vec::new();
    for (sev, text) in &o.warnings {
        let icon = match sev {
            Severity::Error => Icon::Error,
            _ => Icon::Warning,
        };
        let mut parts: Vec<Retained<NSView>> = Vec::new();
        if let Some(img) = glyphs::icon(icon, text) {
            let iv = NSImageView::imageViewWithImage(&img, mtm);
            iv.setContentTintColor(Some(&NSColor::systemOrangeColor()));
            parts.push(view(&iv));
        }
        let l = NSTextField::wrappingLabelWithString(&ns(text), mtm);
        l.setPreferredMaxLayoutWidth(WIDTH - 30.0);
        parts.push(view(&l));
        rows.push(view(&stack(mtm, &parts, false, 6.0)));
    }
    for r in &o.rows {
        let mut parts: Vec<Retained<NSView>> = Vec::new();
        if let Some(img) = glyphs::icon(r.icon, &r.label) {
            let iv = NSImageView::imageViewWithImage(&img, mtm);
            iv.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
            iv.widthAnchor().constraintEqualToConstant(18.0).setActive(true);
            parts.push(view(&iv));
        }
        let name: Retained<NSTextField> = secondary(mtm, &r.label);
        name.widthAnchor().constraintEqualToConstant(110.0).setActive(true);
        parts.push(view(&name));
        let value = label(mtm, &r.value);
        value.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByTruncatingMiddle);
        value.setContentCompressionResistancePriority_forOrientation(1.0, objc2_app_kit::NSLayoutConstraintOrientation::Horizontal);
        parts.push(view(&value));
        if !r.note.is_empty() {
            parts.push(view(&secondary(mtm, &r.note)));
        }
        if r.browsable {
            if let Some(
                iris_gui_core::controller::session::LightTarget::Disk { key }
                | iris_gui_core::controller::session::LightTarget::CdRom { key },
            ) = &r.target
            {
                parts.push(view(&icon_button(mtm, &t!("ov-browse"), &item::id(item::BROWSE_DISK, key), Icon::BrowseFiles)));
            }
        }
        let line = stack(mtm, &parts, false, 8.0);
        line.widthAnchor().constraintLessThanOrEqualToConstant(WIDTH).setActive(true);
        rows.push(view(&line));
    }
    rows
}

/// One timeline row: a dot on the line, the picture, the words, the buttons.
fn timeline_row(mtm: MainThreadMarker, dot: &str, pic: Retained<NSImageView>, title: &str, lines: &[String], buttons: Vec<Retained<NSView>>) -> Retained<NSView> {
    let mark = NSImageView::new(mtm);
    if let Some(img) = glyphs::symbol(dot, "") {
        mark.setImage(Some(&img));
    }
    mark.setContentTintColor(Some(&NSColor::controlAccentColor()));
    mark.widthAnchor().constraintEqualToConstant(16.0).setActive(true);
    let mut text: Vec<Retained<NSView>> = vec![view(&heading(mtm, title, 14.0))];
    for l in lines {
        text.push(view(&secondary(mtm, l)));
    }
    let words = stack(mtm, &text, true, 2.0);
    let spacer = NSView::new(mtm);
    spacer.setContentHuggingPriority_forOrientation(1.0, objc2_app_kit::NSLayoutConstraintOrientation::Horizontal);
    let mut parts = vec![view(&mark), view(&pic), view(&words), spacer];
    parts.extend(buttons);
    let row = stack(mtm, &parts, false, 14.0);
    row.widthAnchor().constraintEqualToConstant(WIDTH).setActive(true);
    view(&row)
}

fn saved_states(mtm: MainThreadMarker, o: &Overview) -> Vec<Retained<NSView>> {
    let (tw, th) = (120.0, 90.0);
    let mut rows: Vec<Retained<NSView>> = Vec::new();
    // The current state: what Start runs. Saving needs the machine running.
    let now = timeline_row(
        mtm,
        "circle.circle",
        picture(mtm, o.preview.as_ref(), tw, th, glyphs::machine_symbol(o.icon)),
        &t!("ov-current-state"),
        &[t!("ov-not-running"), t!("ov-save-while-running")],
        Vec::new(),
    );
    rows.push(now);
    if o.states.is_empty() {
        rows.push(rule(mtm, WIDTH));
        rows.push(view(&secondary(mtm, &t!("ov-no-states"))));
    }
    for s in &o.states {
        rows.push(rule(mtm, WIDTH));
        let when = s.saved.map(format_time).unwrap_or_default();
        let buttons = vec![
            view(&icon_button(mtm, &t!("ov-restore"), &item::id(item::RESTORE_STATE, &s.name), Icon::Restore)),
            view(&icon_button(mtm, &t!("ov-rename"), &item::id(item::RENAME_STATE, &s.name), Icon::Rename)),
            view(&icon_button(mtm, &t!("ov-delete"), &item::id(item::DELETE_STATE, &s.name), Icon::Delete)),
        ];
        rows.push(timeline_row(
            mtm,
            "circle",
            picture(mtm, s.picture.as_ref(), tw, th, "square.stack.3d.up"),
            &s.name,
            &[t!("ov-saved-at", when = when.as_str()), human(s.bytes)],
            buttons,
        ));
    }
    rows
}

/// Build the page for `o` (or the empty state when there is no machine);
/// `tab` 0 shows Details, 1 Saved States.
pub fn build(mtm: MainThreadMarker, o: Option<&Overview>, tab: usize) -> Retained<NSView> {
    let Some(o) = o else {
        let l = secondary(mtm, &t!("ov-no-machine"));
        return view(&stack(mtm, &[view(&l)], true, 0.0));
    };
    let mut rows: Vec<Retained<NSView>> = Vec::new();
    // The name and model are the window's title and subtitle.
    if let Some(s) = &o.state {
        rows.push(view(&secondary(mtm, s)));
    }
    rows.push(preview(mtm, o));

    // Details | Saved States (with the count).
    let states = if o.states.is_empty() { t!("ov-saved-states") } else { format!("{} ({})", t!("ov-saved-states"), o.states.len()) };
    let seg = NSSegmentedControl::new(mtm);
    seg.setSegmentCount(2);
    seg.setLabel_forSegment(&ns(&t!("ov-details")), 0);
    seg.setLabel_forSegment(&ns(&states), 1);
    seg.setSelectedSegment(tab.min(1) as isize);
    crate::settings::wire(&seg, TAB);
    let seg_row = stack(mtm, &[view(&seg)], false, 0.0);
    seg_row.widthAnchor().constraintEqualToConstant(WIDTH).setActive(true);
    seg_row.setAlignment(NSLayoutAttribute::CenterX);
    rows.push(view(&seg_row));

    rows.extend(if tab == 1 { saved_states(mtm, o) } else { details(mtm, o) });
    let page = stack(mtm, &rows, true, 14.0);
    page.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 20.0, left: 28.0, bottom: 24.0, right: 28.0 });
    set_id(&page, "summary.page");
    view(&page)
}

fn human(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.1} GB", n as f64 / (1u64 << 30) as f64)
    } else if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    } else {
        format!("{} KB", n >> 10)
    }
}

/// A date in the user's locale.
fn format_time(secs: u64) -> String {
    let date = objc2_foundation::NSDate::dateWithTimeIntervalSince1970(secs as f64);
    objc2_foundation::NSDateFormatter::localizedStringFromDate_dateStyle_timeStyle(
        &date,
        objc2_foundation::NSDateFormatterStyle::MediumStyle,
        objc2_foundation::NSDateFormatterStyle::ShortStyle,
    )
    .to_string()
}
