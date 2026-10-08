# TODO: Backlog

This file tracks **open** work only. The full history of completed work lives in
git commits (search the log) and in the previous long-form TODO in repo history.

Where the code lives and how to validate a change are documented once, in
`AGENTS.md` and `docs/agent-guide.md` — not repeated here, where the copy only
drifts.

## Open items

### Phone support (the rest of it)

The main window adapts down to 360×294 (see "Adaptive layout" in `AGENTS.md`).
Still desktop-sized:

- The job logs window: a `gtk::Paned` with a 220 px job list. Make it a
  `NavigationSplitView` too, or swap the list for a drop-down when narrow.
- Help and Keyboard Shortcuts: plain `adw::Window`s with fixed default sizes
  (620×520, 460×340). Move them to `adw::Dialog`, which becomes a bottom sheet
  on a phone.
- The crash-report popover asks for 640×420 (`window_actions.rs`).
- The rate limit is hidden on narrow windows and shown nowhere else.

Once those are done, declare it — not before, since stores and Phosh act on it:

- `data/me.spaceinbox.actioneer.desktop`: `X-Purism-FormFactor=Workstation;Mobile;`
  (Phosh hides apps without it from its default app list).
- `data/metainfo.xml.in`: `<requires><display_length compare="ge">360</display_length></requires>`
  and `<supports>` with `touch`, `pointing` and `keyboard` controls (Flathub and
  GNOME Software mark the app adaptive from these).
