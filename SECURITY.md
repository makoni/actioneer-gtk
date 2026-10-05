# Security Policy

## Supported versions

Only the latest release receives security fixes. Fixes ship as a new patch
release through every channel: Flathub, the Snap Store, and the AppImage and
other assets on the GitHub releases page.

| Version          | Supported |
| ---------------- | --------- |
| 1.1.x (latest)   | Yes       |
| Older than 1.1   | No        |

If you build from source, track the `develop` branch or the latest tag.

## Reporting a vulnerability

Please **do not open a public issue** for a security problem.

Report it privately through GitHub instead: open the repository's
[Security tab](https://github.com/makoni/actioneer-gtk/security) and choose
**Report a vulnerability**. Only the maintainer can see the report.

A useful report includes:

- the Actioneer version and how it was installed (Flatpak, Snap, AppImage,
  source build);
- the distribution and desktop environment;
- steps to reproduce, and what an attacker gains.

### What to expect

- An acknowledgement within 7 days.
- An assessment, confirmed or declined with the reasoning, within 30 days.
- For a confirmed issue: a fix in a patch release, and a published GitHub
  security advisory crediting you, unless you prefer to stay anonymous.

Actioneer is maintained by one person in their spare time, so these are
targets rather than guarantees. If a report seems to have gone unanswered,
a short nudge on the same private report is welcome.

## Scope

Actioneer signs in to GitHub with the OAuth device flow and requests the `repo`
and `workflow` scopes, so its token can read private repositories and trigger
or cancel workflow runs. Issues that could expose or misuse that token matter
most, for example:

- the token leaking into logs, crash reports, the cache, or any file outside
  its intended store;
- weaknesses in token storage: the system keyring on classic installs, or the
  encrypted file kept through the xdg-desktop-portal Secret interface inside
  the Flatpak and Snap sandboxes;
- requests sending the token to a host other than GitHub;
- sandbox permissions in the Flatpak or Snap packaging that are broader than
  the app needs.

Out of scope:

- vulnerabilities in GitHub itself (report those to
  [GitHub's bug bounty](https://bounty.github.com/));
- vulnerabilities in GTK, libadwaita, or other dependencies, unless Actioneer
  uses them in an unsafe way (report those upstream);
- attacks that already require control of the user's session or keyring.
