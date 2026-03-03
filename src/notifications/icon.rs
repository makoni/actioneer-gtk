use gtk4::gio;
use gtk4::prelude::Cast;
use std::env;
use std::path::{Path, PathBuf};

pub(super) fn resolve_notification_icon(icon_name: &str) -> gio::Icon {
    if let Some(icon) = snap_icon(icon_name) {
        return icon;
    }

    gio::ThemedIcon::new(icon_name).upcast()
}

fn snap_icon(icon_name: &str) -> Option<gio::Icon> {
    let icon_path = snap_icon_path(icon_name)?;
    let file_icon = gio::FileIcon::new(&gio::File::for_path(icon_path));
    Some(file_icon.upcast())
}

fn snap_icon_path(icon_name: &str) -> Option<PathBuf> {
    let snap_root = env::var_os("SNAP")?;
    let snap_root = Path::new(&snap_root);

    let candidates = ["svg", "png"].into_iter().map(|ext| {
        snap_root
            .join("meta/gui")
            .join(format!("{}.{}", icon_name, ext))
    });

    candidates.into_iter().find(|path| path.exists())
}
