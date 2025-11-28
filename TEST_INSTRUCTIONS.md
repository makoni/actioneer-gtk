# Test Instructions for Snap Build Fix

## Quick Start - Manual Testing Required

The snap build workflow has been fixed and is ready for testing. Due to security restrictions, automated workflow triggers are not available from this environment, so manual testing is required.

## How to Test

### Step 1: Trigger the Workflow
1. Go to: **https://github.com/makoni/actioneer-gtk/actions/workflows/snap-ci.yml**
2. Click the blue **"Run workflow"** button on the right
3. In the dropdown:
   - Branch: Select **`copilot/fix-snap-build-errors`**
4. Click the green **"Run workflow"** button
5. The page will refresh and show the new workflow run

### Step 2: Monitor the Build
The workflow will create two parallel jobs:
- **Build snap (amd64)** - should complete in ~10-15 minutes
- **Build snap (arm64)** - should complete in ~15-20 minutes (cross-compilation is slower)

Click on each job to see the live build logs.

### Step 3: Verify Success
Once both jobs complete, verify:

✅ **Both jobs show green checkmarks**
- No red X marks indicating failure

✅ **Artifacts are uploaded**
- At the bottom of the workflow run page, check for:
  - `snap-amd64` artifact
  - `snap-arm64` artifact

✅ **No errors in logs**
- Open each job's log
- Search for "error" or "failed"
- Confirm no pkg-config errors about glib-2.0

### Step 4: Download and Test (Optional)
If you want to test the actual snap files:

1. Download both artifacts from the workflow run page
2. Extract the .snap files
3. Test installation (requires Linux):
```bash
sudo snap install --dangerous actioneer_1.0.0_amd64.snap
# or for arm64 on arm64 system:
sudo snap install --dangerous actioneer_1.0.0_arm64.snap
```

## What Changed

### Before (Broken)
- 181 lines of complex workflow code
- Manual Rust cross-compilation setup
- pkg-config errors: "glib-2.0 not found"
- Build always failed

### After (Fixed)
- 50 lines of simple workflow code
- Snapcraft handles all compilation automatically
- No manual pkg-config configuration
- Should build successfully

## Expected Timeline
- **Workflow start**: Immediate
- **amd64 build**: ~10-15 minutes
- **arm64 build**: ~15-20 minutes
- **Total time**: ~20 minutes for both to complete

## Success Indicators

### Green Checkmarks ✅
Look for these in the workflow run:
```
✓ Build snap (amd64)
✓ Build snap (arm64)
```

### Artifacts Present 📦
```
snap-amd64 (contains actioneer_1.0.0_amd64.snap)
snap-arm64 (contains actioneer_1.0.0_arm64.snap)
```

### No Errors in Logs 📝
The build logs should NOT contain:
- ❌ "The system library `glib-2.0` required by crate `glib-sys` was not found"
- ❌ "pkg-config exited with status code 1"
- ❌ "error: failed to run custom build command for `glib-sys`"

## If Tests Fail

### Possible Issues

**1. LXD Container Issues**
- Symptom: "Failed to setup LXD" or similar
- This is rare but possible
- Solution: Re-run the workflow

**2. Cross-Compilation Toolchain Missing**
- Symptom: arm64 build fails with "cannot find -lglib-2.0"
- Check if conditional packages were installed
- Review the "Build snap for arm64" log

**3. Network/Download Issues**
- Symptom: Failures downloading dependencies
- Temporary network issues
- Solution: Re-run the workflow

### How to Debug
1. Click on the failed job
2. Expand the "Build snap for [arch]" step
3. Search the log for "error" or "fatal"
4. Check specifically around "Pulling actioneer" and "Building actioneer"

## Alternative: Local Testing

If you have a Linux machine with snapcraft installed:

```bash
# Clone the branch
git clone https://github.com/makoni/actioneer-gtk.git
cd actioneer-gtk
git checkout copilot/fix-snap-build-errors

# Test amd64 build (if on amd64 machine)
snapcraft --build-for=amd64

# Test arm64 cross-compilation (on amd64 machine)
snapcraft --build-for=arm64
```

## Files Changed

1. **`.github/workflows/snap-ci.yml`** - Simplified from 181 to 50 lines
2. **`snap/snapcraft.yaml`** - Added cross-compilation support
3. **`SNAP_BUILD_FIX.md`** - Detailed documentation
4. **`TEST_INSTRUCTIONS.md`** - This file

## Secret Storage Regression Checks

Run these quick manual checks whenever the snap's secret handling changes:
1. **Classic session (outside the snap sandbox):** run `ACTIONEER_LOG=info cargo run` and make sure the log reports `TokenStorage initialized (keyring backend)`; sign in, restart, and confirm the token persists via the host keyring.
2. **Confined snap:** install the snap (`snap install --dangerous actioneer_*.snap`), start it with `ACTIONEER_LOG=info actioneer`, and verify the log shows `Using secret portal storage`. Complete the OAuth flow once, restart the snap, and ensure you're still signed in (the encrypted portal file lives under `$SNAP_USER_COMMON/.config/actioneer/secret-portal`).
3. **Fallback signal:** temporarily stop the portal service (`systemctl --user stop xdg-desktop-portal.service`) and confirm the snap now falls back to the keyring backend with a warning in the log—this is the scenario you must report in the Snap Store review request.

## Need Help?

If the tests fail or you need assistance:
1. Check the workflow logs for specific error messages
2. Review `SNAP_BUILD_FIX.md` for troubleshooting tips
3. Compare the logs to the "Expected Results" section

## Next Steps After Successful Test

Once both builds succeed:
1. ✅ Mark this issue as resolved
2. ✅ Merge the PR to main/develop branch
3. ✅ (Optional) Configure Snap Store credentials for auto-publishing
4. ✅ (Optional) Enable the workflow to run on pushes to main

---

**Ready to test!** Follow Step 1 above to begin. ⬆️
