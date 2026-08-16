/// Small-caps section headings are a Latin typographic device: `to_uppercase`
/// is a no-op for Arabic, Hebrew, Devanagari, Bengali and CJK, and letter
/// spacing actively harms them — it breaks Arabic/Urdu letter joining and
/// detaches Devanagari and Bengali matras from their base glyphs.
///
/// Returns the text to display, and whether the caller should suppress the
/// tracking (letter-spacing) that the section styles apply by default.
pub fn section_heading(text: &str) -> (String, bool) {
    if text.is_ascii() {
        (text.to_uppercase(), false)
    } else {
        (text.to_string(), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_headings_are_uppercased_and_tracked() {
        assert_eq!(section_heading("Workflows"), ("WORKFLOWS".into(), false));
    }

    #[test]
    fn complex_scripts_keep_their_glyphs_and_drop_tracking() {
        // Arabic has no case, and tracking would break its cursive joining.
        assert_eq!(
            section_heading("سير العمل"),
            ("سير العمل".to_string(), true)
        );
        // Cyrillic does have case, but tracking is still safe — only the
        // non-ASCII branch matters here, so it is treated conservatively.
        let (text, suppress) = section_heading("Воркфлоу");
        assert_eq!(text, "Воркфлоу");
        assert!(suppress);
    }
}
