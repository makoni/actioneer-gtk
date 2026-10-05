# TODO: Backlog

This file tracks **open** work only. The full history of completed work lives in
git commits (search the log) and in the previous long-form TODO in repo history.

Where the code lives and how to validate a change are documented once, in
`AGENTS.md` and `docs/agent-guide.md` — not repeated here, where the copy only
drifts.

## Open items (optional / low priority)

### Flatpak: move to the GNOME 51 runtime

The Flathub linter flags `runtime-update-available-to-org.gnome.Platform-51` on
the 1.1.2 build (flathub/me.spaceinbox.actioneer#19). A warning today; Flathub
says such warnings may become errors.

- `runtime-version` in `flatpak/me.spaceinbox.actioneer.yaml.in`, then re-render
  `flatpak/me.spaceinbox.actioneer.yaml` (it is tracked).
- The `flatpak-github-actions:gnome-50` container in `flatpak-ci.yml`.
- Build in CI, then ship it in a Flathub PR with the next release.

### Release workflow: draft the tag on the built commit

`publish.yml` drafts the release with the default `target_commitish`, i.e. the
branch. The tag is created only on publish, so if the branch moves in between,
the tag lands on a later commit than the one the assets (and the Flathub
manifest) were built from. Pass `target_commitish: ${{ github.sha }}` to
`softprops/action-gh-release`. For 1.1.2 this was fixed by hand.

### Stale documentation

- `README.md`: "Architecture Overview" describes `src/api/`, `src/auth/`,
  `src/storage/`, which the layering refactor removed; it also links a
  `CONFIGURATION_GUIDE.md` that does not exist.
- `docs/TESTING.md`: the unit-test list points at the same pre-refactor paths.
- `docs/flatpak.md` says the rendered manifest is not committed, but
  `flatpak/me.spaceinbox.actioneer.yaml` is tracked. Decide which is true.
- `docs/version-bump.md` says to run `cargo generate-lockfile`, which bumps
  every dependency. `cargo update -w` updates only the crate's own entry.
