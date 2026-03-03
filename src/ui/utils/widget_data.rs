use gtk4::glib::{self, object::IsA, prelude::ObjectExt};

pub fn set_data<T: 'static, O: IsA<glib::Object>>(obj: &O, key: &'static str, value: T) {
    // SAFETY: centralizes ObjectExt::set_data usage behind typed helpers.
    unsafe {
        obj.as_ref().set_data(key, value);
    }
}

pub fn get_data_clone<T: Clone + 'static, O: IsA<glib::Object>>(
    obj: &O,
    key: &'static str,
) -> Option<T> {
    // SAFETY: centralizes ObjectExt::data usage behind typed helpers.
    unsafe { obj.as_ref().data::<T>(key).map(|ptr| ptr.as_ref().clone()) }
}

pub fn get_data_copy<T: Copy + 'static, O: IsA<glib::Object>>(
    obj: &O,
    key: &'static str,
) -> Option<T> {
    // SAFETY: centralizes ObjectExt::data usage behind typed helpers.
    unsafe { obj.as_ref().data::<T>(key).map(|ptr| *ptr.as_ref()) }
}

pub fn steal_data<T: 'static, O: IsA<glib::Object>>(obj: &O, key: &'static str) -> Option<T> {
    // SAFETY: centralizes ObjectExt::steal_data usage behind typed helpers.
    unsafe { obj.as_ref().steal_data::<T>(key) }
}

#[cfg(test)]
mod tests {
    use super::{get_data_clone, get_data_copy, set_data, steal_data};
    use gtk4::glib;

    #[test]
    fn widget_data_helpers_round_trip_values() {
        let obj = glib::Object::new::<glib::Object>();

        set_data(&obj, "actioneer-test-int", 42_i64);
        assert_eq!(
            get_data_copy::<i64, _>(&obj, "actioneer-test-int"),
            Some(42)
        );
        assert_eq!(
            steal_data::<i64, _>(&obj, "actioneer-test-int"),
            Some(42_i64)
        );
        assert_eq!(get_data_copy::<i64, _>(&obj, "actioneer-test-int"), None);

        set_data(&obj, "actioneer-test-string", String::from("hello"));
        assert_eq!(
            get_data_clone::<String, _>(&obj, "actioneer-test-string"),
            Some(String::from("hello"))
        );
    }
}
