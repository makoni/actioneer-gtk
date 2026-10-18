use crate::services::omarchy_theme::{OmarchyTheme, contrasting_foreground, readable_foreground};
use gtk4::{self as gtk, gdk};
use libadwaita as adw;
use std::cell::RefCell;
use tracing::warn;

const APP_CSS: &str = r#"
.sidebar-surface,
.detail-surface {
    border-radius: 22px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.08);
}

.sidebar-surface > *,
.detail-surface > *,
.sidebar-surface scrolledwindow,
.detail-surface scrolledwindow,
.sidebar-surface listview,
.detail-surface listview,
.sidebar-surface viewport,
.detail-surface viewport {
    background-color: transparent;
}

.hoverless-row,
.hoverless-row:hover,
.hoverless-row:selected,
.hoverless-row:selected:hover,
.hoverless-row:focus,
.hoverless-row:focus-visible,
.hoverless-row:active {
    background-color: transparent;
    box-shadow: none;
}

.hoverless-list row,
.hoverless-list row:hover,
.hoverless-list row:selected,
.hoverless-list row:selected:hover,
.hoverless-list row:focus,
.hoverless-list row:focus-visible,
.hoverless-list row:active {
    background-color: transparent;
    box-shadow: none;
}

/* ------------------------------------------------------------------ */
/* Workflows redesign                                                  */
/* ------------------------------------------------------------------ */

/* Round status indicator: tinted circle with a symbolic glyph inside. */
.status-dot {
    border-radius: 999px;
    padding: 0;
}

.status-dot image {
    color: currentColor;
}

.status-dot.success {
    background-color: alpha(@success_color, 0.16);
}
.status-dot.success image {
    color: @success_color;
}

.status-dot.error {
    background-color: alpha(@error_color, 0.16);
}
.status-dot.error image {
    color: @error_color;
}

.status-dot.warning,
.status-dot.accent {
    background-color: alpha(@warning_color, 0.18);
}
.status-dot.warning image,
.status-dot.accent image {
    color: @warning_color;
}

.status-dot.idle {
    background-color: alpha(currentColor, 0.07);
}
.status-dot.idle image {
    color: alpha(currentColor, 0.55);
}

/* Single rounded card containing every workflow row. */
.workflows-card {
    background-color: @card_bg_color;
    border: 1px solid alpha(currentColor, 0.07);
    border-radius: 12px;
    box-shadow: 0 1px 3px alpha(black, 0.28);
}

.workflow-item:not(.workflow-item-first) {
    border-top: 1px solid alpha(currentColor, 0.06);
}

.workflow-item {
    transition: background-color 120ms ease-out;
}

.workflow-item:hover {
    background-color: alpha(currentColor, 0.045);
}

/* The expanded row is tinted with currentColor, not a black alpha: GTK's
   `prefers-color-scheme` query does not follow AdwStyleManager for app-level
   providers (so a @media block would apply in the wrong scheme, and is
   unsupported on the GTK 4.14 snap runtime), while a fixed black wash strong
   enough to read in dark stacks with the runs card nested inside it and pushes
   light-mode meta text below AA. currentColor lightens in dark and darkens in
   light, and is kept clearly stronger than the hover cue. */
.workflow-item.expanded {
    background-color: alpha(currentColor, 0.09);
}

.workflow-item.expanded .workflow-detail {
    background-color: transparent;
}

/* The expanded area is indented to line up with the workflow title, past the
   disclosure arrow and the status dot. A narrow detail pane (a phone, or the
   content half of a small window) cannot spare that, so `.narrow` — set by the
   pane's breakpoint — pulls it back to a plain inset. GTK CSS has no logical
   margins, hence the explicit :dir() pairs. */
.workflow-detail:dir(ltr) {
    margin-left: 62px;
    margin-right: 14px;
}

.workflow-detail:dir(rtl) {
    margin-right: 62px;
    margin-left: 14px;
}

.narrow .workflow-detail:dir(ltr) {
    margin-left: 12px;
    margin-right: 6px;
}

.narrow .workflow-detail:dir(rtl) {
    margin-right: 12px;
    margin-left: 6px;
}

/* The job cards of an expanded run sit under the run title, past its
   disclosure arrow and status dot (the title moved 6px in with the header's
   disclosure gap, the jobs with it). The trailing inset matches the leading
   one: different insets read as the block having slipped sideways. Narrow,
   every pixel goes to the job and step names instead. */
.run-jobs {
    margin-left: 42px;
    margin-right: 42px;
}

.narrow .run-jobs {
    margin-left: 6px;
    margin-right: 6px;
}

.workflow-title {
    font-weight: 700;
}

.workflow-file {
    font-family: monospace;
}

.mono {
    font-family: monospace;
}

/* Expanded workflow area: progress + recent runs card. The left inset that
   aligns it under the workflow title is applied in Rust, next to the widths it
   has to match. */
.workflow-progress trough {
    min-height: 4px;
}

.workflow-progress trough progress {
    min-height: 4px;
    background-color: @warning_color;
}

.runs-card {
    background-color: alpha(black, 0.14);
    border: 1px solid alpha(currentColor, 0.06);
    border-radius: 10px;
}

.run-item:not(.run-item-first) {
    border-top: 1px solid alpha(currentColor, 0.055);
}

.run-item:hover {
    background-color: alpha(currentColor, 0.04);
}

.run-item.expanded,
.run-item.expanded:hover {
    background-color: alpha(currentColor, 0.03);
}

.run-number {
    font-weight: 700;
}

/* `dim-label` is a 0.55 opacity utility; on the recessed run/job cards that puts
   the meta line below WCAG AA (measured 4.13:1 in dark, 2.56:1 in light). The
   text is the right thing to fix here — darkening the cards further would only
   trade one theme's contrast for the other's. */
.runs-card .dim-label,
.job-card .dim-label {
    opacity: 0.78;
}

/* Job cards inside an expanded run. */
.job-card {
    background-color: alpha(currentColor, 0.03);
    border: 1px solid alpha(currentColor, 0.07);
    border-radius: 9px;
}

.job-name {
    font-weight: 700;
}

/* Small ghost icon buttons used on workflow/run/job rows. */
.row-action-btn {
    min-width: 30px;
    min-height: 30px;
    padding: 0;
    border-radius: 7px;
    color: alpha(currentColor, 0.6);
}

.row-action-btn:hover {
    background-color: alpha(currentColor, 0.1);
    color: currentColor;
}

.row-action-btn.cancel-action {
    background-color: alpha(@error_color, 0.16);
    color: @error_color;
}

.row-action-btn.cancel-action:hover {
    background-color: alpha(@error_color, 0.3);
}

.row-action-btn.run-action {
    background-color: alpha(currentColor, 0.08);
    color: alpha(currentColor, 0.85);
}

.row-action-btn.run-action:hover {
    background-color: @accent_bg_color;
    color: @accent_fg_color;
}

/* Pane header action buttons (refresh / favorite). */
.header-action-btn {
    min-width: 32px;
    min-height: 32px;
    padding: 0;
    border-radius: 8px;
    background-color: alpha(currentColor, 0.08);
    color: alpha(currentColor, 0.85);
}

.header-action-btn:hover {
    background-color: alpha(currentColor, 0.13);
}

.header-action-btn:checked {
    background-color: alpha(@accent_color, 0.2);
    color: @accent_color;
}

/* Section labels like "WORKFLOWS" / "RECENT RUNS". */
.section-label {
    font-size: 0.82em;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: alpha(currentColor, 0.55);
}

/* Pane header: visibility badge next to the repo title. */
.visibility-badge {
    border: 1px solid alpha(currentColor, 0.16);
    border-radius: 5px;
    padding: 1px 8px;
    font-size: 0.68em;
    letter-spacing: 0.03em;
    color: alpha(currentColor, 0.65);
}

/* Segmented run-status filter with counts. */
.segmented-status-filter {
    border-radius: 8px;
}

.segmented-status-filter > .filter-segment {
    min-height: 30px;
    padding: 0 10px;
    border-radius: 0;
}

.segmented-status-filter > .filter-segment:first-child {
    border-top-left-radius: 8px;
    border-bottom-left-radius: 8px;
}

.segmented-status-filter > .filter-segment:last-child {
    border-top-right-radius: 8px;
    border-bottom-right-radius: 8px;
}

.filter-segment .segment-count {
    font-weight: 700;
}

.filter-segment {
    color: alpha(currentColor, 0.55);
}

.filter-segment.seg-success:hover {
    color: @success_color;
}
.filter-segment.seg-success:checked {
    background-color: alpha(@success_color, 0.2);
    color: @success_color;
}

.filter-segment.seg-running:hover {
    color: @warning_color;
}
.filter-segment.seg-running:checked {
    background-color: alpha(@warning_color, 0.2);
    color: @warning_color;
}

.filter-segment.seg-failed:hover {
    color: @error_color;
}
.filter-segment.seg-failed:checked {
    background-color: alpha(@error_color, 0.2);
    color: @error_color;
}

/* Favourite star: dim when off, accented when on — otherwise every repo looks
   favourited. Scoped so the selected row's own foreground still wins. */
.sidebar-fav {
    opacity: 0.45;
}

.sidebar-fav:checked {
    opacity: 1;
    color: @accent_color;
}

.sidebar-fav:hover:not(:checked) {
    opacity: 0.8;
}

.sidebar-surface listview row:selected .sidebar-fav {
    opacity: 0.55;
}

.sidebar-surface listview row:selected .sidebar-fav:checked {
    opacity: 1;
}

.sidebar-surface listview row:selected .sidebar-fav:hover:not(:checked) {
    opacity: 0.8;
}

/* Sidebar polish: pill filters + solid-accent selection. */
.filter-pill {
    border-radius: 999px;
    min-height: 24px;
    padding: 0 12px;
    font-size: 0.85em;
    background-color: alpha(currentColor, 0.06);
    color: alpha(currentColor, 0.7);
}

.filter-pill:checked {
    background-color: alpha(@accent_color, 0.22);
    color: @accent_color;
    font-weight: 700;
}

/* `@accent_color` is the *foreground* accent and is lightened in dark mode, so
   using it as a fill with hardcoded white text drops below WCAG AA. The
   background/foreground pair keeps contrast correct in both schemes. */
.sidebar-surface listview row:selected,
.sidebar-surface listview row:selected:hover {
    background-color: @accent_bg_color;
    color: @accent_fg_color;
    border-radius: 9px;
}

.sidebar-surface listview row:selected .dim-label {
    color: alpha(@accent_fg_color, 0.72);
}

.sidebar-surface listview row:selected image {
    color: @accent_fg_color;
}

/* Hover paints the whole row, and only rows that do something: `.activatable`
   is set on the `ListItemWidget` by `set_activatable(true)`, which the factory
   grants repository rows and withholds from the section and owner headers — so
   the headers stay flat without needing `:has()`, which GTK CSS lacks. Painting
   a box *inside* the row instead would show as a second, smaller highlight on
   top of the theme's own. `:not(:selected)` keeps a selected row's solid accent
   clean. */
.sidebar-surface listview row.activatable:hover:not(:selected) {
    background-color: alpha(currentColor, 0.09);
    border-radius: 9px;
}

.sidebar-owner-label {
    font-size: 0.72em;
    font-weight: 700;
    letter-spacing: 0.07em;
    color: alpha(currentColor, 0.55);
}

.section-header,
.section-header:hover,
.section-header:focus,
.section-header:focus-visible,
.owner-header,
.owner-header:hover,
.owner-header:focus,
.owner-header:focus-visible {
    background-color: transparent;
    box-shadow: none;
}

/* Scripts where tracking is harmful (Arabic, Hebrew, Devanagari, Bengali, Thai,
   CJK) opt out of it. This block must stay last: it has the same specificity as
   the heading rules it overrides, so source order is what decides. */
/* Job picker in the logs window. The reader must see at a glance whose log is
   on the right, so it reuses the repository sidebar's accent pair. The status
   dot keeps its own colour: it is the one thing selection must not repaint. */
.job-sidebar > row:selected,
.job-sidebar > row:selected:hover {
    background-color: @accent_bg_color;
    color: @accent_fg_color;
    border-radius: 9px;
}

.job-sidebar > row:selected .dim-label {
    color: alpha(@accent_fg_color, 0.72);
}

.job-sidebar > row:hover:not(:selected) {
    background-color: alpha(currentColor, 0.06);
    border-radius: 9px;
}

.no-tracking {
    letter-spacing: 0;
}
"#;

/// The square-corner variant of the app stylesheet, loaded on top of
/// [`APP_CSS`] while the app follows the Omarchy desktop theme.
///
/// Hyprland desktops — Omarchy ships `decoration:rounding = 0` by default —
/// are all right angles, so following the theme squares the app's own
/// surfaces and the libadwaita chrome instead of GNOME's rounded shapes.
/// Shapes that are round by meaning (status dots, switches, radio buttons,
/// sliders, spinners) keep their radius; circular icon buttons — window
/// controls, the dialog close button, the token copy button — square like
/// every other button.
const SQUARE_CSS: &str = r#"
/* ------------------------------------------------------------------ */
/* Hyprland-style square corners (following Omarchy)                   */
/* ------------------------------------------------------------------ */

/* App surfaces, cards, chips and rows: same selectors as the stylesheet
   above, declared later, so they win within this provider. */
.sidebar-surface,
.detail-surface,
.workflows-card,
.runs-card,
.job-card,
.row-action-btn,
.header-action-btn,
.visibility-badge,
.segmented-status-filter,
.segmented-status-filter > .filter-segment:first-child,
.segmented-status-filter > .filter-segment:last-child,
.filter-pill,
.sidebar-surface listview row:selected,
.sidebar-surface listview row:selected:hover,
.sidebar-surface listview row.activatable:hover:not(:selected),
.job-sidebar > row:selected,
.job-sidebar > row:selected:hover,
.job-sidebar > row:hover:not(:selected) {
    border-radius: 0;
}

/* libadwaita chrome. The selectors mirror where the theme actually draws a
   radius: dialog surfaces on `sheet` (15px floating sheets, 18px alerts),
   preference groups on `list.boxed-list` (12px), window controls on the
   button's image (100%), the scrollbar track on `range > trough` (99px),
   and the alert body's scrolling viewport (18px top corners). */
window,
sheet,
.card,
list.boxed-list,
list.content,
popover > contents,
modelbutton,
toast,
tooltip,
button,
entry,
spinbutton,
dropdown > button,
checkbutton > check,
windowcontrols > button > image,
progressbar trough,
progressbar progress,
scrollbar slider,
scrollbar > range > trough,
.app-version,
viewport,
row {
    border-radius: 0;
}
"#;

thread_local! {
    /// One provider carries the app stylesheet for the whole display.
    static APP_PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    /// The square-corner rules, in a provider of their own one step above the
    /// app stylesheet, so they win over its rounded radii whatever order the
    /// two were installed in. Empty while the app keeps its rounded corners.
    static SQUARE_PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    /// Whether the square rules are loaded, so an unchanged apply re-parses
    /// nothing.
    static SQUARE_APPLIED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn install_app_css() {
    let Some(display) = gdk::Display::default() else {
        warn!("No display available for CSS provider");
        return;
    };

    APP_PROVIDER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let provider = slot.get_or_insert_with(|| {
            let provider = gtk::CssProvider::new();
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            provider
        });
        provider.load_from_string(APP_CSS);
    });
}

/// Squares the app's corners to match the Hyprland/Omarchy desktop look.
pub fn apply_square_corners() {
    set_square_corners(true);
}

/// Returns the app to its default rounded corners.
pub fn clear_square_corners() {
    set_square_corners(false);
}

fn set_square_corners(square: bool) {
    if SQUARE_APPLIED.with(|applied| applied.get()) == square {
        return;
    }
    let Some(display) = gdk::Display::default() else {
        warn!("No display available for the square-corner rules");
        return;
    };
    SQUARE_PROVIDER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let provider = slot.get_or_insert_with(|| {
            let provider = gtk::CssProvider::new();
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
            provider
        });
        provider.load_from_string(if square { SQUARE_CSS } else { "" });
    });
    SQUARE_APPLIED.with(|applied| applied.set(square));
}

thread_local! {
    /// One provider carries the Omarchy palette for the whole display: every
    /// apply redefines its contents instead of registering a new provider, so
    /// switching themes cannot pile up registrations.
    static OMARCHY_PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    /// The palette stylesheet last loaded, so re-applying the same theme —
    /// a preferences round trip, a wallpaper change in the watched directory —
    /// re-parses nothing.
    static OMARCHY_LOADED: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Re-colors the app with the active Omarchy theme's palette.
///
/// The definitions land in a provider at `STYLE_PROVIDER_PRIORITY_APPLICATION`
/// — the priority `APP_CSS` already uses, above the theme stylesheet — so the
/// `@define-color` names override Adwaita's own and every widget that reads
/// them (`@accent_bg_color`, `@card_bg_color`, …) follows the desktop theme.
pub fn apply_omarchy_palette(theme: &OmarchyTheme) {
    let Some(display) = gdk::Display::default() else {
        warn!("No display available for the Omarchy palette");
        return;
    };
    let css = omarchy_css(theme, libadwaita_derives_standalone_colors());
    load_omarchy_css(&display, &css);
}

/// Drops the palette overrides so Adwaita's own colors return.
pub fn clear_omarchy_palette() {
    if let Some(display) = gdk::Display::default() {
        load_omarchy_css(&display, "");
    }
}

fn load_omarchy_css(display: &gdk::Display, css: &str) {
    if OMARCHY_LOADED.with(|loaded| *loaded.borrow() == css) {
        return;
    }
    OMARCHY_PROVIDER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let provider = slot.get_or_insert_with(|| {
            let provider = gtk::CssProvider::new();
            gtk::style_context_add_provider_for_display(
                display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            provider
        });
        provider.load_from_string(css);
    });
    OMARCHY_LOADED.with(|loaded| *loaded.borrow_mut() = css.to_owned());
}

/// From libadwaita 1.6 the standalone text colors are CSS variables derived
/// from the fills (`--accent-color: oklab(from var(--accent-bg-color) …)`),
/// and its widgets read those, not `@accent_color`. Redefining the named
/// color then reaches only this app's own stylesheet; the variables have to
/// be set as well. GTK reads CSS variables from 4.16, which 1.6 requires.
fn libadwaita_derives_standalone_colors() -> bool {
    (adw::major_version(), adw::minor_version()) >= (1, 6)
}

/// Builds the `@define-color` block that re-points Adwaita's named colors at
/// the Omarchy palette, and with `css_variables` the `:root` block libadwaita
/// 1.6+ reads its standalone colors from (see
/// `libadwaita_derives_standalone_colors`). Every value went through the
/// palette's color validation, so interpolation cannot smuggle arbitrary CSS
/// into the stylesheet.
fn omarchy_css(theme: &OmarchyTheme, css_variables: bool) -> String {
    let mut css = String::from("/* Colors follow the active Omarchy theme. */\n");
    let mut variables = String::new();
    let mut define = |name: &str, value: &str| {
        css.push_str(&format!("@define-color {name} {value};\n"));
        // Only the standalone text colors: libadwaita derives them itself,
        // while the fills and surfaces it takes from the named colors.
        if let Some(variable) = STANDALONE_COLOR_VARIABLES
            .iter()
            .find_map(|(named, variable)| (*named == name).then_some(*variable))
        {
            variables.push_str(&format!("  {variable}: {value};\n"));
        }
    };

    // The standalone `*_color` names are text and icon colors: Omarchy's
    // fills are tuned for filled surfaces and can sit below AA as text on its
    // light palettes, so they get a readable variant — readable on every
    // surface text is drawn on, the sidebars included. `*_bg_color` keeps the
    // raw fill and pairs it with a contrasting foreground.
    let text_surfaces: [&str; 4] = [
        &theme.background,
        &theme.lighter_background,
        &theme.dark_background,
        &theme.darker_background,
    ];

    define("window_bg_color", &theme.background);
    define("window_fg_color", &theme.foreground);
    define("view_bg_color", &theme.background);
    define("view_fg_color", &theme.foreground);
    define("headerbar_bg_color", &theme.background);
    define("headerbar_fg_color", &theme.foreground);
    // A foreground color: libadwaita mixes it into transparency at the
    // border opacity, so a background shade would vanish.
    define("headerbar_border_color", &theme.foreground);
    define("headerbar_backdrop_color", &theme.background);
    define("sidebar_bg_color", &theme.dark_background);
    define("sidebar_fg_color", &theme.foreground);
    define("sidebar_backdrop_color", &theme.dark_background);
    define("secondary_sidebar_bg_color", &theme.darker_background);
    define("secondary_sidebar_fg_color", &theme.foreground);
    define("secondary_sidebar_backdrop_color", &theme.darker_background);
    define("card_bg_color", &theme.lighter_background);
    define("card_fg_color", &theme.foreground);
    define("dialog_bg_color", &theme.background);
    define("dialog_fg_color", &theme.foreground);
    define("popover_bg_color", &theme.lighter_background);
    define("popover_fg_color", &theme.foreground);

    let accent_foreground = contrasting_foreground(&theme.accent, &theme.background);
    define("accent_bg_color", &theme.accent);
    define("accent_fg_color", &accent_foreground);
    define(
        "accent_color",
        &readable_foreground(&theme.accent, &text_surfaces),
    );
    // `theme_selected_*` predate libadwaita's accent names and still drive
    // GTK's selection styling, so they follow the accent too. Shade colors
    // stay Adwaita's: they are mode-tuned translucent overlays for borders
    // and shadows, not palette colors.
    define("theme_selected_bg_color", &theme.accent);
    define("theme_selected_fg_color", &accent_foreground);

    if let Some(red) = &theme.red {
        let foreground = contrasting_foreground(red, &theme.background);
        let text = readable_foreground(red, &text_surfaces);
        define("destructive_bg_color", red);
        define("destructive_fg_color", &foreground);
        define("destructive_color", &text);
        define("error_bg_color", red);
        define("error_fg_color", &foreground);
        define("error_color", &text);
    }
    if let Some(green) = &theme.green {
        let foreground = contrasting_foreground(green, &theme.background);
        define("success_bg_color", green);
        define("success_fg_color", &foreground);
        define("success_color", &readable_foreground(green, &text_surfaces));
    }
    if let Some(yellow) = &theme.yellow {
        let foreground = contrasting_foreground(yellow, &theme.background);
        define("warning_bg_color", yellow);
        define("warning_fg_color", &foreground);
        define(
            "warning_color",
            &readable_foreground(yellow, &text_surfaces),
        );
    }
    if css_variables && !variables.is_empty() {
        css.push_str(&format!(":root {{\n{variables}}}\n"));
    }
    css
}

/// The standalone text colors and the CSS variables libadwaita 1.6+ derives
/// them into.
const STANDALONE_COLOR_VARIABLES: [(&str, &str); 5] = [
    ("accent_color", "--accent-color"),
    ("destructive_color", "--destructive-color"),
    ("error_color", "--error-color"),
    ("success_color", "--success-color"),
    ("warning_color", "--warning-color"),
];

/// Applies the default text direction for the effective UI language.
///
/// Lives here rather than in `kernel/` because it drives a GTK widget default;
/// the kernel must not name the toolkit.
pub fn apply_text_direction_for_language() {
    let direction = if crate::kernel::i18n::current_language_is_rtl() {
        gtk::TextDirection::Rtl
    } else {
        gtk::TextDirection::Ltr
    };
    gtk::Widget::set_default_direction(direction);
}

#[cfg(test)]
mod tests {
    use super::{
        APP_CSS, SQUARE_CSS, apply_omarchy_palette, apply_square_corners, clear_omarchy_palette,
        clear_square_corners, omarchy_css,
    };
    use crate::services::omarchy_theme::{OmarchyTheme, PaletteMode};
    use crate::ui::test_helpers::run_gtk_test;
    use gtk4 as gtk;
    use gtk4::gdk;
    use gtk4::prelude::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// The Tokyo Night palette as Omarchy ships it.
    fn sample_theme() -> OmarchyTheme {
        OmarchyTheme {
            name: "tokyo-night".to_owned(),
            mode: PaletteMode::Dark,
            background: "#1a1b26".to_owned(),
            dark_background: "#13141c".to_owned(),
            darker_background: "#0e0e14".to_owned(),
            lighter_background: "#24283b".to_owned(),
            foreground: "#a9b1d6".to_owned(),
            accent: "#7aa2f7".to_owned(),
            red: Some("#f7768e".to_owned()),
            green: Some("#9ece6a".to_owned()),
            yellow: Some("#e0af68".to_owned()),
        }
    }

    /// `css` without `/* … */` comments, which may legitimately mention a
    /// selector.
    fn strip_comments(css: &str) -> String {
        let mut out = String::new();
        let mut css = css;
        while let Some(start) = css.find("/*") {
            out.push_str(&css[..start]);
            match css[start..].find("*/") {
                Some(end) => css = &css[start + end + 2..],
                None => return out,
            }
        }
        out.push_str(css);
        out
    }

    /// Selectors in `css`, one per line, comments and bodies stripped.
    fn selectors_in(css: &str) -> Vec<String> {
        let mut out = Vec::new();
        for block in strip_comments(css).split('}') {
            if let Some((head, _)) = block.split_once('{') {
                for selector in head.split(',') {
                    let selector = selector.trim();
                    if !selector.is_empty() {
                        out.push(selector.to_string());
                    }
                }
            }
        }
        out
    }

    /// Selectors in `APP_CSS`, one per line, comments and bodies stripped.
    fn selectors() -> Vec<String> {
        selectors_in(APP_CSS)
    }

    /// Selectors in `css` whose rule body rounds a corner with a non-zero
    /// value.
    fn rounded_selectors(css: &str) -> Vec<String> {
        let mut out = Vec::new();
        for block in strip_comments(css).split('}') {
            let Some((head, body)) = block.split_once('{') else {
                continue;
            };
            let rounds = body
                .lines()
                .any(|line| line.contains("radius") && !line.trim_end().ends_with(": 0;"));
            if !rounds {
                continue;
            }
            for selector in head.split(',') {
                let selector = selector.trim();
                if !selector.is_empty() {
                    out.push(selector.to_string());
                }
            }
        }
        out
    }

    #[test]
    fn sidebar_hover_is_limited_to_activatable_rows() {
        // Regression guard for "headers read as clickable". The list row wraps
        // every item — repository rows *and* the non-interactive section/owner
        // headers — so a bare `row:hover` repaints the headers too, and GTK CSS
        // has no `:has()` to exclude them. `.activatable` is the discriminator:
        // the factory sets it on repository rows only.
        //
        // Note this checks what the rule selects, not what it paints: that the
        // headers stay flat under the pointer is verified by hovering the real
        // window, not by this test.
        let all = selectors();
        let offenders: Vec<&str> = all
            .iter()
            .filter(|selector| selector.contains("listview"))
            .filter(|selector| {
                // the `:hover` belongs to the last compound in the selector
                let Some((_, last)) = selector.rsplit_once(char::is_whitespace) else {
                    return false;
                };
                last.starts_with("row")
                    && last.contains(":hover")
                    && !last.contains(":selected")
                    && !last.contains(".activatable")
            })
            .map(String::as_str)
            .collect();
        assert!(
            offenders.is_empty(),
            "a sidebar hover rule reaches non-activatable rows, i.e. the section \
             and owner headers: {offenders:?}"
        );

        assert!(
            all.iter()
                .any(|selector| selector.contains("row.activatable:hover")),
            "the repository row's hover rule is gone"
        );
    }

    #[test]
    fn omarchy_css_repoints_adwaita_named_colors() {
        let css = omarchy_css(&sample_theme(), false);
        assert!(css.contains("@define-color window_bg_color #1a1b26;"));
        assert!(css.contains("@define-color view_bg_color #1a1b26;"));
        assert!(css.contains("@define-color headerbar_border_color #a9b1d6;"));
        assert!(css.contains("@define-color sidebar_bg_color #13141c;"));
        assert!(css.contains("@define-color secondary_sidebar_bg_color #0e0e14;"));
        assert!(css.contains("@define-color card_bg_color #24283b;"));
        assert!(css.contains("@define-color popover_bg_color #24283b;"));
        assert!(css.contains("@define-color accent_bg_color #7aa2f7;"));
        assert!(css.contains("@define-color accent_fg_color #1a1b26;"));
        assert!(css.contains("@define-color accent_color #7aa2f7;"));
        assert!(css.contains("@define-color theme_selected_bg_color #7aa2f7;"));
        assert!(css.contains("@define-color theme_selected_fg_color #1a1b26;"));
        assert!(css.contains("@define-color destructive_bg_color #f7768e;"));
        assert!(css.contains("@define-color error_color #f7768e;"));
        assert!(css.contains("@define-color success_color #9ece6a;"));
        assert!(css.contains("@define-color warning_color #e0af68;"));
    }

    #[test]
    fn omarchy_css_keeps_standalone_colors_readable_on_light_themes() {
        // Flexoki Light: the fills are fine as fills but below AA as text, so
        // the standalone colors are adjusted while `*_bg_color` keeps them.
        let mut theme = sample_theme();
        theme.mode = PaletteMode::Light;
        theme.background = "#fffcf0".to_owned();
        theme.lighter_background = "#e6e4d9".to_owned();
        theme.foreground = "#100f0f".to_owned();
        theme.accent = "#205ea6".to_owned();
        theme.red = Some("#d14d41".to_owned());
        theme.green = Some("#879a39".to_owned());
        theme.yellow = Some("#d0a215".to_owned());

        let css = omarchy_css(&theme, false);
        assert!(css.contains("@define-color destructive_bg_color #d14d41;"));
        assert!(css.contains("@define-color error_bg_color #d14d41;"));
        assert!(css.contains("@define-color success_bg_color #879a39;"));
        assert!(css.contains("@define-color warning_bg_color #d0a215;"));
        assert!(!css.contains("@define-color destructive_color #d14d41;"));
        assert!(!css.contains("@define-color error_color #d14d41;"));
        assert!(!css.contains("@define-color success_color #879a39;"));
        assert!(!css.contains("@define-color warning_color #d0a215;"));
    }

    #[test]
    fn omarchy_css_omits_semantic_colors_a_theme_does_not_ship() {
        let mut theme = sample_theme();
        theme.red = None;
        theme.green = None;
        theme.yellow = None;
        let css = omarchy_css(&theme, false);
        assert!(!css.contains("destructive"));
        assert!(!css.contains("success"));
        assert!(!css.contains("warning"));
        assert!(!css.contains("error"));
    }

    #[test]
    fn square_corners_cover_every_rounded_app_surface() {
        // Every rule that rounds an app surface must be flattened in square
        // mode; the status dot is round by meaning and stays. A new rounded
        // surface without a SQUARE_CSS counterpart fails here.
        let square = selectors_in(SQUARE_CSS);
        let rounded = rounded_selectors(APP_CSS);
        let missing: Vec<&str> = rounded
            .iter()
            .map(String::as_str)
            .filter(|selector| *selector != ".status-dot")
            .filter(|selector| !square.iter().any(|candidate| candidate == selector))
            .collect();
        assert!(
            missing.is_empty(),
            "square mode leaves these rounded selectors behind: {missing:?}"
        );
    }

    #[test]
    fn square_corners_cover_the_libadwaita_chrome() {
        // Checked against libadwaita's compiled stylesheet: the theme rounds
        // dialog surfaces on `sheet` (15px floating sheets, 18px alerts),
        // preference groups on `list.boxed-list` (12px), window controls on
        // the button's image (100%), the scrollbar track on `range > trough`
        // (99px) and the alert body's scrolling viewport (18px top corners).
        // The app-surface test above cannot see any of these — they are theme
        // selectors, not APP_CSS ones — so a regression here must fail this
        // list.
        let square = selectors_in(SQUARE_CSS);
        for selector in [
            "window",
            "sheet",
            ".card",
            "list.boxed-list",
            "list.content",
            "popover > contents",
            "modelbutton",
            "toast",
            "tooltip",
            "button",
            "entry",
            "spinbutton",
            "dropdown > button",
            "checkbutton > check",
            "windowcontrols > button > image",
            "progressbar trough",
            "progressbar progress",
            "scrollbar slider",
            "scrollbar > range > trough",
            ".app-version",
            "viewport",
            "row",
        ] {
            assert!(
                square.iter().any(|candidate| candidate == selector),
                "square mode misses the libadwaita chrome selector {selector}"
            );
        }
    }

    #[test]
    fn square_corners_leave_round_affordances_alone() {
        // Switches, radios, sliders and the status dot are round by meaning;
        // flattening them would read as a rendering bug. The square layer
        // must not name them at all.
        for selector in ["switch", "radio", "scale", "spinner", ".status-dot"] {
            assert!(
                !SQUARE_CSS.contains(selector),
                "square mode must not touch {selector}"
            );
        }
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn square_corners_stylesheet_parses_and_reloads() {
        run_gtk_test("square_corners_stylesheet_parses_and_reloads", || {
            let provider = gtk::CssProvider::new();
            let errors = Rc::new(RefCell::new(Vec::<String>::new()));
            let collected = Rc::clone(&errors);
            provider.connect_parsing_error(move |_, _, error| {
                collected.borrow_mut().push(error.to_string());
            });
            provider.load_from_string(&format!("{APP_CSS}{SQUARE_CSS}"));
            assert!(
                errors.borrow().is_empty(),
                "the square stylesheet has parse errors: {:?}",
                errors.borrow()
            );

            // The real plumbing: square, then back to the rounded default —
            // which leaves nothing behind for the tests after this one (they
            // share the display), the stylesheet the run installs once
            // included.
            crate::ui::test_helpers::ensure_app_css();
            apply_square_corners();
            clear_square_corners();
        });
    }

    #[test]
    fn omarchy_css_sets_libadwaitas_standalone_color_variables_when_it_reads_them() {
        let theme = sample_theme();
        let legacy = omarchy_css(&theme, false);
        assert!(!legacy.contains(":root"), "1.5 and older have no variables");

        let css = omarchy_css(&theme, true);
        let root = css.split(":root").nth(1).expect("a :root block");
        for variable in [
            "--accent-color",
            "--destructive-color",
            "--error-color",
            "--success-color",
            "--warning-color",
        ] {
            assert!(root.contains(variable), "{variable} is set: {root}");
        }
        // The fills come from the named colors, which libadwaita reads.
        assert!(!root.contains("--accent-bg-color"));
        // A variable carries the same value as its named color.
        let named = legacy
            .lines()
            .find(|line| line.starts_with("@define-color accent_color "))
            .and_then(|line| line.trim_end_matches(';').rsplit(' ').next())
            .expect("accent_color is defined");
        assert!(root.contains(&format!("--accent-color: {named};")));
    }

    #[test]
    #[ignore = "requires GTK display"]
    // `style_context().lookup_color()` is the only API that resolves named
    // colors; GTK deprecated it in 4.10 without a replacement, and this test
    // is the one place that pins the override precedence.
    #[allow(deprecated)]
    fn omarchy_palette_overrides_libadwaita_named_colors() {
        run_gtk_test("omarchy_palette_overrides_libadwaita_named_colors", || {
            let expected = gdk::RGBA::parse("#7aa2f7").expect("parse the accent");
            apply_omarchy_palette(&sample_theme());
            let window = gtk::Window::new();
            assert_eq!(
                window.style_context().lookup_color("accent_bg_color"),
                Some(expected),
                "the palette definition must override Adwaita's accent"
            );
            clear_omarchy_palette();
            assert_ne!(
                window.style_context().lookup_color("accent_bg_color"),
                Some(expected),
                "clearing the palette must restore Adwaita's accent"
            );
        });
    }
}
