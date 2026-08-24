/// Scripts where inter-letter tracking is harmful: it breaks the cursive
/// joining of Arabic, Hebrew and Urdu, and detaches Devanagari, Bengali and Thai
/// matras from their base glyphs. CJK and Hangul are included because tracking
/// reads as broken spacing rather than emphasis there.
fn tracking_is_harmful(ch: char) -> bool {
    matches!(ch as u32,
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic
        | 0x0750..=0x077F // Arabic Supplement
        | 0x0900..=0x097F // Devanagari
        | 0x0980..=0x09FF // Bengali
        | 0x0E00..=0x0E7F // Thai
        | 0x1100..=0x11FF // Hangul Jamo
        | 0x3000..=0x30FF // CJK punctuation, Hiragana, Katakana
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xAC00..=0xD7AF // Hangul Syllables
        | 0xFB50..=0xFDFF // Arabic Presentation Forms-A
        | 0xFE70..=0xFEFF // Arabic Presentation Forms-B
    )
}

/// Prepares a translated string for use as a small-caps section heading.
///
/// Uppercasing is applied unconditionally: Rust's `to_uppercase` is Unicode
/// correct, so it handles accented Latin and Cyrillic properly and is a no-op
/// for scripts without case (Arabic, Devanagari, CJK …). Testing for ASCII
/// instead would leave "Öffentlich" and "Приватный" in mixed case next to
/// "PRIVAT" and "WORKFLOWS" on the same screen.
///
/// Returns the text to display, and whether the caller should suppress the
/// tracking (letter-spacing) that the section styles apply by default.
pub fn section_heading(text: &str) -> (String, bool) {
    let suppress_tracking = text.chars().any(tracking_is_harmful);
    (text.to_uppercase(), suppress_tracking)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_headings_are_uppercased_and_tracked() {
        assert_eq!(section_heading("Workflows"), ("WORKFLOWS".into(), false));
    }

    #[test]
    fn accented_latin_and_cyrillic_are_uppercased_too() {
        // These used to fall through an `is_ascii()` gate and render in mixed
        // case beside uppercased siblings on the same screen.
        assert_eq!(section_heading("Öffentlich").0, "ÖFFENTLICH");
        assert_eq!(
            section_heading("Exécutions récentes").0,
            "EXÉCUTIONS RÉCENTES"
        );
        assert_eq!(section_heading("Приватный"), ("ПРИВАТНЫЙ".into(), false));
    }

    #[test]
    fn cursive_and_uncased_scripts_drop_tracking() {
        // Uppercasing is a no-op for these, but tracking would damage them.
        for text in ["سير العمل", "पसंदीदा", "প্রিয়", "工作流", "پسندیدہ"]
        {
            let (rendered, suppress) = section_heading(text);
            assert_eq!(rendered, text, "{text} should be left as-is");
            assert!(suppress, "{text} must not be tracked");
        }
    }
}
