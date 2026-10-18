use gtk4::gio;
use gtk4::prelude::Cast;
use std::env;
use std::path::{Path, PathBuf};

pub(super) fn resolve_notification_icon(icon_name: &str) -> gio::Icon {
    // The notification server looks a themed icon up in its own theme, which
    // has no entry for an app it did not install: hand it the bundled file.
    if let Some(icon_path) = snap_icon_path(icon_name).or_else(|| appimage_icon_path(icon_name)) {
        return gio::FileIcon::new(&gio::File::for_path(icon_path)).upcast();
    }

    gio::ThemedIcon::new(icon_name).upcast()
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

fn appimage_icon_path(icon_name: &str) -> Option<PathBuf> {
    env::var_os("APPIMAGE")?;
    bundled_icon_path(Path::new(&env::var_os("APPDIR")?), icon_name)
}

/// The icon inside a mounted AppImage, which stays readable while it runs.
fn bundled_icon_path(appdir: &Path, icon_name: &str) -> Option<PathBuf> {
    let hicolor = appdir.join("share/icons/hicolor");
    [
        hicolor.join(format!("scalable/apps/{icon_name}.svg")),
        hicolor.join(format!("256x256/apps/{icon_name}.png")),
    ]
    .into_iter()
    .find(|path| path.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn bundled_icon_prefers_the_scalable_icon() {
        let dir = tempfile::tempdir().expect("temp dir");
        let scalable = dir.path().join("share/icons/hicolor/scalable/apps");
        let raster = dir.path().join("share/icons/hicolor/256x256/apps");
        fs::create_dir_all(&scalable).expect("scalable dir");
        fs::create_dir_all(&raster).expect("raster dir");
        fs::write(raster.join("app.png"), b"").expect("png");
        assert_eq!(
            bundled_icon_path(dir.path(), "app"),
            Some(raster.join("app.png"))
        );

        fs::write(scalable.join("app.svg"), b"").expect("svg");
        assert_eq!(
            bundled_icon_path(dir.path(), "app"),
            Some(scalable.join("app.svg"))
        );
    }

    #[test]
    fn bundled_icon_is_none_without_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(bundled_icon_path(dir.path(), "app"), None);
    }
}
