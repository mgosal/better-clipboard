# Manual Test Script

Use this script for release testing before filing issues.

## Setup

- [ ] Build the app with `./scripts/build-macos-app.sh`.
- [ ] Open `dist/Better Clipboard.app`.
- [ ] Grant Accessibility permission when prompted.
- [ ] Confirm the 📋 menu bar item appears.

## Clipboard Capture

- [ ] Copy plain text from any app.
- [ ] Copy a URL.
- [ ] Copy an image.
- [ ] Open Better Clipboard with `Option+Space`.
- [ ] Confirm all copied items appear in newest-first order.
- [ ] Confirm text and URL rows are left-aligned.
- [ ] Confirm image rows show thumbnails.

## Keyboard Navigation

- [ ] Press `Down` repeatedly through a long history list.
- [ ] Confirm the selected row stays visible as the list scrolls.
- [ ] Press `Up` repeatedly.
- [ ] Confirm the selected row stays visible while moving upward.
- [ ] Press `Cmd+Down` or `Tab` to expand the list.
- [ ] Press `Cmd+Up` to collapse the list.
- [ ] Press `Escape` and confirm the palette closes without changing the clipboard.

## Paste Flow

- [ ] Focus a text field in another app.
- [ ] Open Better Clipboard with `Option+Space`.
- [ ] Press `Enter` on a text item.
- [ ] Confirm the item pastes into the original text field.
- [ ] Repeat with a URL.
- [ ] Repeat with an image in an app that accepts image paste.
- [ ] Double-click an item and confirm it pastes into the original app.

## One-Click Copy

- [ ] Open Better Clipboard.
- [ ] Click the type glyph or thumbnail on a non-newest item.
- [ ] Confirm the item is copied to the clipboard without pasting.
- [ ] Confirm the item moves to the top of history.
- [ ] Paste manually with `Cmd+V` and confirm the copied value is correct.

## Image Preview

- [ ] Select an image item.
- [ ] Press `Right Arrow`.
- [ ] Confirm a floating preview opens next to the palette.
- [ ] Press `Escape` and confirm only the preview closes.
- [ ] Open the preview again and press `Enter`.
- [ ] Confirm the preview and palette close, then the image is pasted into the original app.

## Settings

- [ ] Open settings with the `⚙` button.
- [ ] Switch between light and dark theme.
- [ ] Change the history limit and confirm it persists after restart.
- [ ] Enable `Run at login`.
- [ ] Confirm `~/Library/LaunchAgents/com.mgosal.better-clipboard.plist` is created.
- [ ] Disable `Run at login`.
- [ ] Confirm the LaunchAgent plist is removed.

## Tray Menu

- [ ] Right-click the 📋 menu bar item.
- [ ] Click `Show Better Clipboard` and confirm the palette opens.
- [ ] Click `Settings` and confirm settings opens directly.
- [ ] Click away from the palette and confirm it hides.
- [ ] Click `Quit Better Clipboard` and confirm the app exits.

## Issue Report Notes

When filing an issue, include:

- macOS version.
- Chip architecture, such as Apple silicon or Intel.
- Whether the app was run from `dist/Better Clipboard.app` or `cargo run`.
- Whether Accessibility permission was granted.
- Exact shortcut or click sequence.
- Expected result and actual result.
