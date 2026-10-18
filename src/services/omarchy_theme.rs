//! Reads the active Omarchy theme palette.
//!
//! Omarchy keeps the current theme in its XDG state directory:
//! `omarchy/current/theme.name` names the theme and
//! `omarchy/current/theme/colors.toml` holds the palette every themed
//! component derives from. This module turns that state into an
//! [`OmarchyTheme`] so the UI can follow the desktop's colors; it knows
//! nothing about GTK widgets.
//!
//! The Flatpak sandbox remaps `XDG_STATE_HOME` into the app's private state
//! directory, so the host location is probed directly as well. The manifest
//! grants read access to it (`--filesystem=~/.local/state/omarchy:ro`);
//! without the grant the probe finds nothing and the app keeps its own
//! theme settings.

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Which Adwaita color scheme the palette is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Dark,
    Light,
}

/// The subset of the Omarchy palette the app maps onto Adwaita's named
/// colors. `red`/`green`/`yellow` are optional so a theme missing a semantic
/// color falls back to Adwaita's own instead of failing to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmarchyTheme {
    pub name: String,
    pub mode: PaletteMode,
    pub background: String,
    pub dark_background: String,
    pub darker_background: String,
    pub lighter_background: String,
    pub foreground: String,
    pub accent: String,
    pub red: Option<String>,
    pub green: Option<String>,
    pub yellow: Option<String>,
}

#[derive(Deserialize)]
struct RawColors {
    mode: Option<String>,
    background: Option<String>,
    dark_background: Option<String>,
    darker_background: Option<String>,
    lighter_background: Option<String>,
    foreground: Option<String>,
    accent: Option<String>,
    red: Option<String>,
    green: Option<String>,
    yellow: Option<String>,
}

/// The state directories that may hold the active theme, in probe order.
///
/// `XDG_STATE_HOME` covers native installs; the literal `~/.local/state`
/// path covers the Flatpak sandbox, where the environment variable points
/// into the app's private state directory instead.
fn state_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(state_home) = dirs::state_dir() {
        dirs.push(state_home.join("omarchy/current"));
    }
    if let Some(home) = dirs::home_dir() {
        let home_state = home.join(".local/state/omarchy/current");
        if !dirs.contains(&home_state) {
            dirs.push(home_state);
        }
    }
    dirs
}

/// The first state directory that actually names a theme.
pub fn current_state_dir() -> Option<PathBuf> {
    state_dirs()
        .into_iter()
        .find(|dir| dir.join("theme.name").is_file())
}

/// The directory holding the active theme's palette, if a theme is present.
///
/// A theme switch replaces this directory (`rm -rf theme && mv next-theme
/// theme`), so callers that monitor it must re-arm the watch afterwards.
pub fn current_theme_dir() -> Option<PathBuf> {
    current_state_dir().map(|dir| dir.join("theme"))
}

/// Loads the active theme, or `None` when the desktop is not Omarchy, the
/// state is unreadable, or the palette fails validation.
pub fn load() -> Option<OmarchyTheme> {
    current_state_dir().and_then(|dir| load_from(&dir))
}

/// Loads the theme from an explicit state directory — the one holding
/// `theme.name`. [`load`] is the production entry point.
fn load_from(state_dir: &Path) -> Option<OmarchyTheme> {
    let name = fs::read_to_string(state_dir.join("theme.name")).ok()?;
    let name = name.trim().to_owned();
    if name.is_empty() {
        return None;
    }
    let raw = fs::read_to_string(state_dir.join("theme/colors.toml")).ok()?;
    let colors: RawColors = toml::from_str(&raw).ok()?;

    let background = colors.background.as_deref().and_then(valid_color)?;
    let foreground = colors.foreground.as_deref().and_then(valid_color)?;
    let accent = colors.accent.as_deref().and_then(valid_color)?;

    // The depth variants are pure polish: derive them from the background
    // when a theme does not ship them.
    let pick = |value: Option<&str>, fallback: &str| {
        value
            .and_then(valid_color)
            .unwrap_or_else(|| fallback.to_owned())
    };
    let dark_background = pick(colors.dark_background.as_deref(), &background);
    let darker_background = pick(colors.darker_background.as_deref(), &dark_background);
    let lighter_background = pick(colors.lighter_background.as_deref(), &background);

    Some(OmarchyTheme {
        mode: palette_mode(
            colors.mode.as_deref(),
            state_dir.join("theme/light.mode").is_file(),
            &background,
        ),
        name,
        background,
        dark_background,
        darker_background,
        lighter_background,
        foreground,
        accent,
        red: colors.red.as_deref().and_then(valid_color),
        green: colors.green.as_deref().and_then(valid_color),
        yellow: colors.yellow.as_deref().and_then(valid_color),
    })
}

/// Picks readable text for a filled accent (or semantic) color: the
/// palette's own background when it has enough contrast — Omarchy's
/// components pair those two — and pure black or white otherwise.
pub fn contrasting_foreground(color: &str, background: &str) -> String {
    if contrast_ratio(color, background) >= 4.5 {
        return background.to_owned();
    }
    // 0.179 is where black and white reach equal contrast against a color.
    if relative_luminance(color) > 0.179 {
        "#000000".to_owned()
    } else {
        "#ffffff".to_owned()
    }
}

/// Picks a text-safe variant of a palette color for the surfaces it is used
/// as a *foreground* on: the color itself when it already clears WCAG AA
/// (4.5:1) against every surface, otherwise the same hue walked toward black
/// or white — whichever end the surfaces favor — until it does.
///
/// Omarchy's semantic colors are tuned for fills; on its light palettes
/// yellow, green and red sit well below AA as text, while its dark palettes
/// are already safe.
pub fn readable_foreground(color: &str, surfaces: &[&str]) -> String {
    let passes = |candidate: &str| {
        surfaces
            .iter()
            .all(|surface| contrast_ratio(candidate, surface) >= 4.5)
    };
    if surfaces.is_empty() || passes(color) {
        return color.to_owned();
    }

    let Some((red, green, blue)) = parse_rgb(color) else {
        return "#000000".to_owned();
    };
    // Black and white score equally against a surface at luminance 0.179;
    // past it the surfaces are light and dark text wins.
    let surface_luminance = surfaces
        .iter()
        .map(|surface| relative_luminance(surface))
        .sum::<f64>()
        / surfaces.len() as f64;
    let target = if surface_luminance > 0.179 {
        (0_u8, 0_u8, 0_u8)
    } else {
        (255_u8, 255_u8, 255_u8)
    };

    for step in 1..=20 {
        let amount = f64::from(step) / 20.0;
        let blend = |channel: u8, target: u8| {
            (f64::from(channel) * (1.0 - amount) + f64::from(target) * amount).round() as u8
        };
        let candidate = format!(
            "#{:02x}{:02x}{:02x}",
            blend(red, target.0),
            blend(green, target.1),
            blend(blue, target.2)
        );
        if passes(&candidate) {
            return candidate;
        }
    }
    // Reached only when the surfaces disagree on which end is readable; the
    // pure end is still the best available choice.
    if surface_luminance > 0.179 {
        "#000000".to_owned()
    } else {
        "#ffffff".to_owned()
    }
}

/// `mode` when the palette declares one; otherwise light when the theme
/// ships Omarchy's `light.mode` marker file, which is how Omarchy itself tells
/// light themes apart; otherwise inferred from how light the background is.
fn palette_mode(declared: Option<&str>, light_marker: bool, background: &str) -> PaletteMode {
    match declared {
        Some(mode) if mode.eq_ignore_ascii_case("light") => PaletteMode::Light,
        Some(mode) if mode.eq_ignore_ascii_case("dark") => PaletteMode::Dark,
        _ if light_marker => PaletteMode::Light,
        _ => {
            if relative_luminance(background) > 0.5 {
                PaletteMode::Light
            } else {
                PaletteMode::Dark
            }
        }
    }
}

/// Accepts `#rgb` and `#rrggbb`, lowercased with the short form expanded.
/// Alpha forms are rejected on purpose: the contrast math pairs opaque
/// colors, and a translucent palette entry could not be reasoned about.
/// Anything else is treated as absent — a value the math cannot trust must
/// not reach the stylesheet.
fn valid_color(value: &str) -> Option<String> {
    let digits = value.trim().strip_prefix('#')?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let digits = digits.to_ascii_lowercase();
    match digits.len() {
        3 => {
            let expanded: String = digits.chars().flat_map(|c| [c, c]).collect();
            Some(format!("#{expanded}"))
        }
        6 => Some(format!("#{digits}")),
        _ => None,
    }
}

fn parse_rgb(color: &str) -> Option<(u8, u8, u8)> {
    let color = valid_color(color)?;
    let digits = &color[1..];
    Some((
        u8::from_str_radix(&digits[0..2], 16).ok()?,
        u8::from_str_radix(&digits[2..4], 16).ok()?,
        u8::from_str_radix(&digits[4..6], 16).ok()?,
    ))
}

/// WCAG relative luminance (0 = black, 1 = white).
fn relative_luminance(color: &str) -> f64 {
    let Some((red, green, blue)) = parse_rgb(color) else {
        return 0.0;
    };
    fn linear(channel: u8) -> f64 {
        let channel = f64::from(channel) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
}

fn contrast_ratio(first: &str, second: &str) -> f64 {
    let (first, second) = (relative_luminance(first), relative_luminance(second));
    let (high, low) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    (high + 0.05) / (low + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Tokyo Night palette as Omarchy ships it.
    const TOKYO_NIGHT: &str = r##"
mode = "dark"

accent = "#7aa2f7"
selection = "#292e42"
muted = "#414868"

background = "#1a1b26"
dark_background = "#13141c"
darker_background = "#0e0e14"
lighter_background = "#24283b"

foreground = "#a9b1d6"
dark_foreground = "#565f89"
light_foreground = "#b4bee6"
bright_foreground = "#c0caf5"

red = "#f7768e"
yellow = "#e0af68"
orange = "#eb927b"
green = "#9ece6a"
cyan = "#449dab"
blue = "#7aa2f7"
magenta = "#ad8ee6"
brown = "#75493d"
"##;

    fn write_state(dir: &Path, name: &str, colors: &str) {
        fs::create_dir_all(dir.join("theme")).expect("theme dir");
        fs::write(dir.join("theme.name"), name).expect("theme.name");
        fs::write(dir.join("theme/colors.toml"), colors).expect("colors.toml");
    }

    #[test]
    fn loads_a_valid_state_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_state(dir.path(), "tokyo-night\n", TOKYO_NIGHT);

        let theme = load_from(dir.path()).expect("theme loads");
        assert_eq!(theme.name, "tokyo-night");
        assert_eq!(theme.mode, PaletteMode::Dark);
        assert_eq!(theme.background, "#1a1b26");
        assert_eq!(theme.dark_background, "#13141c");
        assert_eq!(theme.darker_background, "#0e0e14");
        assert_eq!(theme.lighter_background, "#24283b");
        assert_eq!(theme.foreground, "#a9b1d6");
        assert_eq!(theme.accent, "#7aa2f7");
        assert_eq!(theme.red.as_deref(), Some("#f7768e"));
        assert_eq!(theme.green.as_deref(), Some("#9ece6a"));
        assert_eq!(theme.yellow.as_deref(), Some("#e0af68"));
    }

    #[test]
    fn rejects_a_palette_without_a_background() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_state(
            dir.path(),
            "broken",
            "foreground = \"#ffffff\"\naccent = \"#123456\"\n",
        );
        assert_eq!(load_from(dir.path()), None);
    }

    #[test]
    fn rejects_colors_gtk_cannot_parse() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_state(
            dir.path(),
            "broken",
            "background = \"not-a-color\"\nforeground = \"#ffffff\"\naccent = \"#123456\"\n",
        );
        assert_eq!(load_from(dir.path()), None);
    }

    #[test]
    fn rejects_an_empty_theme_name() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_state(dir.path(), "  \n", TOKYO_NIGHT);
        assert_eq!(load_from(dir.path()), None);
    }

    #[test]
    fn missing_state_directory_loads_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(load_from(&dir.path().join("absent")), None);
    }

    #[test]
    fn depth_variants_fall_back_to_the_background() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_state(
            dir.path(),
            "minimal",
            "background = \"#101010\"\nforeground = \"#eeeeee\"\naccent = \"#7aa2f7\"\n",
        );

        let theme = load_from(dir.path()).expect("theme loads");
        assert_eq!(theme.dark_background, "#101010");
        assert_eq!(theme.darker_background, "#101010");
        assert_eq!(theme.lighter_background, "#101010");
        assert_eq!(theme.red, None);
        assert_eq!(theme.green, None);
        assert_eq!(theme.yellow, None);
    }

    #[test]
    fn infers_the_mode_when_the_palette_omits_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_state(
            dir.path(),
            "dark-inferred",
            "background = \"#1a1b26\"\nforeground = \"#ffffff\"\naccent = \"#123456\"\n",
        );
        assert_eq!(
            load_from(dir.path()).map(|theme| theme.mode),
            Some(PaletteMode::Dark)
        );

        write_state(
            dir.path(),
            "light-inferred",
            "background = \"#fffcf0\"\nforeground = \"#100f0f\"\naccent = \"#205ea6\"\n",
        );
        assert_eq!(
            load_from(dir.path()).map(|theme| theme.mode),
            Some(PaletteMode::Light)
        );
    }

    #[test]
    fn a_declared_mode_wins_over_the_background() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_state(
            dir.path(),
            "declared",
            "mode = \"light\"\nbackground = \"#1a1b26\"\nforeground = \"#ffffff\"\naccent = \"#123456\"\n",
        );
        assert_eq!(
            load_from(dir.path()).map(|theme| theme.mode),
            Some(PaletteMode::Light)
        );
    }

    #[test]
    fn the_light_mode_marker_wins_over_the_background() {
        let dir = tempfile::tempdir().expect("temp dir");
        // A light theme whose palette keeps a dark-ish background.
        write_state(
            dir.path(),
            "marked",
            "background = \"#1a1b26\"\nforeground = \"#ffffff\"\naccent = \"#123456\"\n",
        );
        fs::write(dir.path().join("theme/light.mode"), "").expect("light.mode");
        assert_eq!(
            load_from(dir.path()).map(|theme| theme.mode),
            Some(PaletteMode::Light)
        );

        // A declared mode still wins over the marker.
        write_state(
            dir.path(),
            "marked",
            "mode = \"dark\"\nbackground = \"#fffcf0\"\nforeground = \"#100f0f\"\naccent = \"#205ea6\"\n",
        );
        assert_eq!(
            load_from(dir.path()).map(|theme| theme.mode),
            Some(PaletteMode::Dark)
        );
    }

    #[test]
    fn short_hex_colors_are_expanded_and_lowercased() {
        assert_eq!(valid_color("#ABC"), Some("#aabbcc".to_owned()));
        assert_eq!(valid_color(" #123456 "), Some("#123456".to_owned()));
        assert_eq!(valid_color("123456"), None);
        assert_eq!(valid_color("#12345"), None);
        assert_eq!(valid_color("#gggggg"), None);
    }

    #[test]
    fn alpha_hex_forms_are_rejected() {
        // The palette is opaque and the contrast math pairs opaque colors, so
        // a translucent entry falls back to Adwaita instead of reaching the
        // stylesheet.
        assert_eq!(valid_color("#abcd"), None);
        assert_eq!(valid_color("#aabbccdd"), None);
    }

    #[test]
    fn contrasting_foreground_prefers_the_theme_background() {
        // Tokyo Night: the accent is bright enough that the theme's own
        // background reads as the text color on filled accents.
        assert_eq!(contrasting_foreground("#7aa2f7", "#1a1b26"), "#1a1b26");
        // Flexoki Light: dark accent, near-white background.
        assert_eq!(contrasting_foreground("#205ea6", "#fffcf0"), "#fffcf0");
    }

    #[test]
    fn contrasting_foreground_falls_back_to_black_or_white() {
        // Same color on both sides: no usable palette pairing.
        assert_eq!(contrasting_foreground("#7aa2f7", "#7aa2f7"), "#000000");
        assert_eq!(contrasting_foreground("#0e0e14", "#0e0e14"), "#ffffff");
    }

    #[test]
    fn readable_foreground_keeps_colors_that_already_pass() {
        // Tokyo Night's palette is already AA as text on its own surfaces.
        let surfaces = ["#1a1b26", "#24283b"];
        for color in ["#7aa2f7", "#f7768e", "#9ece6a", "#e0af68"] {
            assert_eq!(
                readable_foreground(color, &surfaces),
                color,
                "{color} already clears AA"
            );
        }
    }

    #[test]
    fn readable_foreground_adjusts_light_palettes_that_fail_as_text() {
        // Flexoki Light's fills as text: yellow 2.31:1, green 3.04:1 and red
        // 4.21:1 against the background — all below AA on the window surface
        // and worse on the card.
        let surfaces = ["#fffcf0", "#e6e4d9"];
        for color in ["#d0a215", "#879a39", "#d14d41"] {
            let readable = readable_foreground(color, &surfaces);
            assert_ne!(readable, color, "{color} must not pass through");
            for surface in surfaces {
                assert!(
                    contrast_ratio(&readable, surface) >= 4.5,
                    "{readable} vs {surface} must clear AA"
                );
            }
        }

        // Catppuccin Latte's accent sits just under AA on its background.
        let readable = readable_foreground("#1e66f5", &["#eff1f5", "#ccd0da"]);
        assert_ne!(readable, "#1e66f5");
        assert!(contrast_ratio(&readable, "#eff1f5") >= 4.5);
    }

    #[test]
    fn readable_foreground_reaches_a_readable_end_for_degenerate_palettes() {
        // The color is the surface itself: the walk must still terminate on a
        // value that clears AA.
        for color in ["#101010", "#f0f0f0"] {
            let readable = readable_foreground(color, &[color]);
            assert!(
                contrast_ratio(&readable, color) >= 4.5,
                "{readable} vs {color} must clear AA"
            );
        }
    }

    #[test]
    fn state_dirs_probe_the_flatpak_visible_home_path() {
        let dirs = state_dirs();
        assert!(!dirs.is_empty());
        assert!(dirs.iter().all(|dir| dir.ends_with("omarchy/current")));
        let home_state = dirs::home_dir()
            .expect("home dir")
            .join(".local/state/omarchy/current");
        assert!(dirs.contains(&home_state));
    }
}
