# CSS Variables (colors, fonts, helpers)

Overview
- Libadwaita exposes many CSS variables for UI colors (accent, success, warning, error), window/view/sidebar palettes, fonts (`--document-font-family`, `--monospace-font-family`) and helpers (opacities, border colors, window radius).
- Use these variables in `style.css` and widget CSS to remain consistent with Adwaita and system accent.

Examples
- Use `var(--accent-bg-color)` / `var(--accent-fg-color)` for accent-themed widgets.
- Use `--view-bg-color` / `--view-fg-color` for primary content backgrounds and text.

Notes
- Many variables have light+dark variants; AdwApplication loads the right ones according to style and high-contrast.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/css-variables.html
