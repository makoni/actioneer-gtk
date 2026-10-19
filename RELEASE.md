# Actioneer 1.2.2

A small AppImage-only release. The Flatpak and the Snap are unchanged apart
from the version number.

## What's fixed

- **AppImage format.** The 1.2.1 AppImage was packaged in a newer image format
  (DwarFS) that some AppImage tools cannot open: AppImageLauncher could not
  integrate it, and the AppImage catalog could not list it. The AppImage is
  back in the standard SquashFS format and still brings its own C library, so
  it keeps running on older distributions such as Ubuntu 22.04.
- **Icons in the AppImage.** On systems without the Adwaita icon theme, some
  buttons showed placeholder images instead of their icons. The AppImage now
  includes the icons it needs. Your desktop's own icon theme still takes
  precedence where it has them.

## Before you update

If you update the AppImage with a tool such as Gear Lever or AppImageUpdate,
this update downloads nearly the whole file once more, because the format
changed back. Later updates download only what changed.

Your settings, favorites and sign-in carry over.
