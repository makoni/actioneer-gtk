use gtk4::glib::{self, translate::from_glib};

pub fn try_remove_source(source_id: glib::SourceId) -> bool {
    unsafe { from_glib(glib::ffi::g_source_remove(source_id.as_raw())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    #[test]
    #[ignore = "requires GTK display"]
    fn removing_a_live_source_succeeds_and_removing_it_twice_does_not() {
        run_gtk_test("try_remove_source", || {
            let source_id = glib::timeout_add_local(std::time::Duration::from_secs(3600), || {
                glib::ControlFlow::Continue
            });

            // The whole point of this helper: `glib::source_remove` panics on an
            // already-removed id, which happens whenever a widget is dropped
            // between scheduling and teardown. This returns a bool instead.
            let raw = source_id.as_raw();
            assert!(try_remove_source(source_id), "the live source is removed");

            // Reconstruct the id that is now dead. This is what happens in the
            // app when a widget is dropped between scheduling and teardown.
            let stale: glib::SourceId = unsafe { from_glib(raw) };
            assert!(
                !try_remove_source(stale),
                "removing it again reports false rather than panicking"
            );
        });
    }
}
