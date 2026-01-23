# AdwNavigationSplitView

Purpose
- Two-pane adaptive container with `sidebar` and `content` children. Collapses into a navigation view on narrow widths.

When to use
- Desktop-style master/detail layouts with a persistent sidebar on wide screens and a single-pane navigation on narrow screens.

Practical tips
- Control collapsed state with `AdwBreakpoint` (toggle `collapsed` property for narrow widths).
- Use `AdwNavigationPage` for both sidebar and content children. `AdwHeaderBar` will handle back button automatically.
- For triple-pane layouts, nest split views or use `AdwMultiLayoutView`.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/class.NavigationSplitView.html
