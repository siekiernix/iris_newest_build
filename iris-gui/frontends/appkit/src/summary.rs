//! The stopped machine's summary, from the app controller's `Overview`:
//! name and spec, Power On and Edit Settings, warnings, the saved states as a
//! list, and the hardware rows with a browse button on each disk.

use objc2::rc::Retained;
use objc2::Message;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSBezelStyle, NSButton, NSColor, NSFont, NSImageView, NSLayoutAttribute, NSStackView, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::NSArray;
use iris_gui_core::controller::ids::{item, toolbar as tb};
use iris_gui_core::controller::overview::Overview;
use iris_gui_core::controller::Severity;

use crate::glyphs;
use crate::ui::{button, heading, label, ns, secondary};

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
    if vertical {
        s.setAlignment(NSLayoutAttribute::Leading);
    }
    s
}

fn small_button(mtm: MainThreadMarker, title: &str, id: &str, icon: Option<iris_gui_core::controller::icons::Icon>) -> Retained<NSButton> {
    let b = button(mtm, title, id);
    b.setBezelStyle(NSBezelStyle::Push);
    b.setToolTip(Some(&ns(title)));
    if let Some(img) = icon.and_then(|i| glyphs::icon(i, title)) {
        b.setImage(Some(&img));
        b.setTitle(&ns(""));
    }
    b
}

/// Build the summary for `o` (or the empty state when there is no machine).
pub fn build(mtm: MainThreadMarker, o: Option<&Overview>) -> Retained<NSView> {
    let Some(o) = o else {
        let l = secondary(mtm, &t!("ov-no-machine"));
        return view(&stack(mtm, &[view(&l)], true, 0.0));
    };
    let mut rows: Vec<Retained<NSView>> = Vec::new();

    let picture = NSImageView::new(mtm);
    if let Some(img) = glyphs::symbol(glyphs::machine_symbol(o.icon), &o.spec) {
        picture.setImage(Some(&img));
        picture.setSymbolConfiguration(Some(&objc2_app_kit::NSImageSymbolConfiguration::configurationWithPointSize_weight(
            40.0, 0.0,
        )));
    }
    picture.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    let mut title_line: Vec<Retained<NSView>> = vec![view(&heading(mtm, &o.name, 26.0))];
    if let Some(s) = &o.state {
        title_line.push(view(&secondary(mtm, s)));
    }
    let title = stack(mtm, &title_line, false, 10.0);
    let spec = secondary(mtm, &o.spec);
    spec.setFont(Some(&NSFont::systemFontOfSize(14.0)));
    let power = button(mtm, &t!("ov-power-on"), tb::POWER_ON);
    power.setKeyEquivalent(&ns("\r"));
    let edit = button(mtm, &t!("ov-edit-settings"), tb::MACHINE_SETTINGS);
    let buttons = stack(mtm, &[view(&power), view(&edit)], false, 8.0);
    let head_text = stack(mtm, &[view(&title), view(&spec), view(&buttons)], true, 8.0);
    rows.push(view(&stack(mtm, &[view(&picture), view(&head_text)], false, 16.0)));

    for (sev, text) in &o.warnings {
        let icon = match sev {
            Severity::Error => iris_gui_core::controller::icons::Icon::Error,
            _ => iris_gui_core::controller::icons::Icon::Warning,
        };
        let mut parts: Vec<Retained<NSView>> = Vec::new();
        if let Some(img) = glyphs::icon(icon, text) {
            let iv = NSImageView::imageViewWithImage(&img, mtm);
            iv.setContentTintColor(Some(&NSColor::systemOrangeColor()));
            parts.push(view(&iv));
        }
        parts.push(view(&label(mtm, text)));
        rows.push(view(&stack(mtm, &parts, false, 6.0)));
    }

    rows.push(view(&heading(mtm, &t!("ov-saved-states"), 13.0)));
    if o.states.is_empty() {
        rows.push(view(&secondary(mtm, &t!("ov-no-states"))));
    }
    for s in &o.states {
        let when = s.saved.map(|t| format_time(t)).unwrap_or_default();
        let size = human(s.bytes);
        let line = stack(
            mtm,
            &[
                view(&label(mtm, &s.name)),
                view(&secondary(mtm, &format!("{when}  {size}"))),
                view(&small_button(mtm, &t!("ov-restore"), &item::id(item::RESTORE_STATE, &s.name), Some(iris_gui_core::controller::icons::Icon::Restore))),
                view(&small_button(mtm, &t!("ov-rename"), &item::id(item::RENAME_STATE, &s.name), Some(iris_gui_core::controller::icons::Icon::Rename))),
                view(&small_button(mtm, &t!("ov-delete"), &item::id(item::DELETE_STATE, &s.name), Some(iris_gui_core::controller::icons::Icon::Delete))),
            ],
            false,
            8.0,
        );
        rows.push(view(&line));
    }

    rows.push(view(&heading(mtm, &t!("ov-hardware"), 13.0)));
    for r in &o.rows {
        let mut parts: Vec<Retained<NSView>> = Vec::new();
        if let Some(img) = glyphs::icon(r.icon, &r.label) {
            let iv = NSImageView::imageViewWithImage(&img, mtm);
            iv.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
            parts.push(view(&iv));
        }
        let name: Retained<NSTextField> = secondary(mtm, &r.label);
        name.setFrameSize(objc2_foundation::NSSize::new(110.0, 18.0));
        name.widthAnchor().constraintEqualToConstant(110.0).setActive(true);
        parts.push(view(&name));
        parts.push(view(&label(mtm, &r.value)));
        if !r.note.is_empty() {
            parts.push(view(&secondary(mtm, &r.note)));
        }
        if r.browsable {
            if let Some(iris_gui_core::controller::session::LightTarget::Disk { key } | iris_gui_core::controller::session::LightTarget::CdRom { key }) = &r.target {
                parts.push(view(&small_button(
                    mtm,
                    &t!("ov-browse"),
                    &item::id(item::BROWSE_DISK, key),
                    Some(iris_gui_core::controller::icons::Icon::BrowseFiles),
                )));
            }
        }
        rows.push(view(&stack(mtm, &parts, false, 8.0)));
    }
    let page = stack(mtm, &rows, true, 12.0);
    page.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 24.0, left: 28.0, bottom: 24.0, right: 28.0 });
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
