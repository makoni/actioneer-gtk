# Styles & Appearance

Highlights
- Color scheme: AdwStyleManager exposes color-scheme controls (prefer system / force dark / force light). Use AdwStyleManager for programmatic control.
- Accent color: use CSS variables (`--accent-bg-color`, `--accent-fg-color`, `--accent-color`) and AdwStyleManager properties to query accent programmatically.
- High contrast: supported automatically; use helper variables and media queries to support it.

Custom styles
- Add `style.css`, `style-dark.css`, `style-hc.css` resources to your app if you ship custom theming; AdwApplication will load them automatically.

Practical tips
- Use CSS variables instead of hardcoded colors when possible. Prefer AdwStyleManager for system-preference queries.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/styles-and-appearance.html
