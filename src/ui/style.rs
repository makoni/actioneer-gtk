use gtk4::{self as gtk, gdk};
use tracing::warn;

const APP_CSS: &str = r#"
.sidebar-surface,
.detail-surface {
    border-radius: 22px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.08);
    overflow: hidden;
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

.workflow-card {
    border-radius: 18px;
    overflow: hidden;
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

.run-row,
.run-row:hover,
.run-row:focus,
.run-row:focus-visible,
.job-row,
.job-row:hover,
.job-row:focus,
.job-row:focus-visible,
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
"#;

pub fn install_app_css() {
    let Some(display) = gdk::Display::default() else {
        warn!("No display available for CSS provider");
        return;
    };

    let provider = gtk::CssProvider::new();
    provider.load_from_data(APP_CSS);

    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
