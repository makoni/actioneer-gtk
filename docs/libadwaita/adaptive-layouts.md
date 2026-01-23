# Adaptive Layouts (breakpoints, split/overlay views)

Core ideas
- Breakpoints: use AdwBreakpoint to change layout based on available width — used with AdwWindow/AdwApplicationWindow and AdwDialog.
- Split views: AdwNavigationSplitView (desktop layout) and AdwOverlaySplitView (overlay for narrow widths). Both accept sidebar and content children.
- NavigationView: use AdwNavigationView / AdwNavigationPage for single-pane navigation on small widths.
- Toolbar and header: use AdwToolbarView and AdwHeaderBar for correct integration with breakpoints and window controls.

Common patterns
- Sidebar + content: NavigationSplitView with breakpoint controlling `collapsed`.
- Triple-pane layouts: nest two split views or use a MultiLayoutView with breakpoints to swap layouts.
- Dialogs: use AdwDialog for adaptive dialogs (floating vs bottom-sheet behavior).

Practical: prefer AdwNavigationSplitView for this app's sidebar pattern; AdwToolbarView + AdwHeaderBar handle title/buttons correctly.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/adaptive-layouts.html
