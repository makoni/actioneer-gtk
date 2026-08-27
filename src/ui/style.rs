use gtk4::{self as gtk, gdk};
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

.sidebar-surface listview row:hover:not(:selected) {
    background-color: alpha(currentColor, 0.06);
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

pub fn install_app_css() {
    let Some(display) = gdk::Display::default() else {
        warn!("No display available for CSS provider");
        return;
    };

    let provider = gtk::CssProvider::new();
    provider.load_from_bytes(&gtk::glib::Bytes::from(APP_CSS.as_bytes()));

    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
