# TODO: Backlog

This file tracks **open** work only. The full history of completed work lives in
git commits (search the log) and in the previous long-form TODO in repo history.

Where the code lives and how to validate a change are documented once, in
`AGENTS.md` and `docs/agent-guide.md` — not repeated here, where the copy only
drifts.

## Open items (optional / low priority)

### Next release: say that the AppImage now needs glibc 2.43

CI moved to Ubuntu 26.04, so the AppImage and the standalone binary of the next
release link against glibc 2.43 and no longer run on Ubuntu 24.04, Debian 13,
Fedora 42 or RHEL 10 (1.1.2 still does). Call this out in the release notes and
point affected users to the Flatpak or the Snap. The next Flathub PR will also be
the first on the GNOME 51 runtime.
