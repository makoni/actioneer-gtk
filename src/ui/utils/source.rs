use gtk4::glib::{self, translate::from_glib};

pub fn try_remove_source(source_id: glib::SourceId) -> bool {
    unsafe { from_glib(glib::ffi::g_source_remove(source_id.as_raw())) }
}
