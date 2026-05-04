# Manual Test Script

Use this script for release testing before filing issues. The current release candidate should cover the core keyboard workflow, native Share, file-list capture, and the compact-to-expanded scroll behavior.

## Current Known Issues

- Native Share targets vary by macOS version and installed apps, so record item type and destination for Share failures.
- If automatic paste fails, the selected item should still remain on the clipboard for manual `Cmd+V`.

## Setup

- [ ] Build the app with `./scripts/build-macos-app.sh`.
- [ ] Open `dist/Better Clipboard.app`.
- [ ] Confirm Better Clipboard shows its own permission explanation before macOS opens System Settings or the Accessibility prompt.
- [ ] Grant Accessibility permission when prompted.
- [ ] Confirm the 📋 menu bar item appears.

## Clipboard Capture

- [ ] Copy plain text from any app.
- [ ] Copy a URL.
- [ ] Copy an existing local file path.
- [ ] Copy one or more files in Finder.
- [ ] Copy an email address.
- [ ] Copy a phone number.
- [ ] Copy an image.
- [ ] Open Better Clipboard with `Option+Space`.
- [ ] Confirm all copied items appear in newest-first order.
- [ ] Confirm text, URL, file path, file-list, email, and phone rows are left-aligned.
- [ ] Confirm each row shows an action button.
- [ ] Confirm clickable action buttons appear in the bottom-right of each row, with keyboard shortcuts underneath the icons.
- [ ] Confirm metadata appears bottom-left on the same bottom row as the action buttons.
- [ ] Confirm image row tiles fill the row height.
- [ ] Note any row spacing, clipping, alignment, or hierarchy issues for a GitHub layout issue.
- [ ] Copy a test credit card number such as `4111 1111 1111 1111`.
- [ ] Copy a test API key-shaped value such as `sk-proj-abcdefghijklmnopqrstuvwxyz123456`.
- [ ] Confirm sensitive-looking values are masked in the row summary.

## Keyboard Navigation

- [ ] Press `Down` repeatedly through a long history list.
- [ ] Confirm the selected row stays visible as the list scrolls.
- [ ] Press `Up` repeatedly.
- [ ] Confirm the selected row stays visible while moving upward.
- [ ] Reopen in compact mode, scroll the mouse wheel once, and confirm the palette expands past 3 items without snapping back to the selected row.
- [ ] Continue mouse scrolling and confirm the scroll position stays under mouse control.
- [ ] Select a URL item and press `O`.
- [ ] Confirm the URL opens in the default browser.
- [ ] Select a text item and press `C`.
- [ ] Confirm the item is copied without pasting, the palette closes, and the item does not move to the top of the history.
- [ ] Select a file path item and press `O`.
- [ ] Confirm the file opens.
- [ ] Select a file path item and press `F`.
- [ ] Confirm Finder reveals the file.
- [ ] Select a Finder file-list item and press `O`.
- [ ] Confirm the copied files open without Better Clipboard creating duplicate files.
- [ ] Select a Finder file-list item and press `F`.
- [ ] Confirm Finder reveals the first file.
- [ ] Select an email item and press `O`.
- [ ] Confirm the default mail app opens a composer.
- [ ] Select a phone item and press `O`.
- [ ] Confirm macOS opens the configured phone handler.
- [ ] Select any item and press `S`.
- [ ] Confirm the native macOS share sheet opens above Better Clipboard.
- [ ] Dismiss the share sheet and confirm Better Clipboard remains open.
- [ ] Click the bottom-right `Paste` / `Enter` action on an item.
- [ ] Confirm the item pastes into the original app.
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

## One-Click Actions

- [ ] Open Better Clipboard.
- [ ] Click the left text type tile on a non-newest text item.
- [ ] Confirm the item is copied to the clipboard without pasting.
- [ ] Confirm the text item moves to the top of history.
- [ ] Paste manually with `Cmd+V` and confirm the copied value is correct.
- [ ] Click the URL action button on a URL item.
- [ ] Confirm the URL opens in the default browser.
- [ ] Click the file path action button on a file item.
- [ ] Confirm Finder reveals the file.
- [ ] Click the email action button on an email item.
- [ ] Confirm the default mail app opens a composer.
- [ ] Click the phone action button on a phone item.
- [ ] Confirm macOS opens the configured phone handler.
- [ ] Click the image action button on an image item.
- [ ] Confirm the image preview opens.
- [ ] Reopen Better Clipboard and click the bottom-right `Copy` / `C` button on a non-newest text item.
- [ ] Confirm the palette closes and the item is copied without moving to the top of history.
- [ ] Reopen Better Clipboard and click the bottom-right `Share` / `S` button.
- [ ] Confirm the native macOS share sheet opens above Better Clipboard and the palette remains open after the sheet is dismissed.

## Known Issue Filing

- [ ] File one GitHub issue for row layout problems, with screenshots and notes about window size, selected item type, and whether compact or expanded mode was used.
- [ ] File one GitHub issue for Share behavior, with the exact item type, whether mouse or `S` was used, the destination app, and what happened to focus and clipboard contents.
- [ ] File one GitHub issue for file-list behavior if copied Finder files do not appear, open, reveal, share, or paste as expected.
- [ ] File separate issues for paste-focus problems, image-preview problems, install/startup problems, or permission problems.
- [ ] Use the manual test report template if reporting a full pass/fail run.

## Image Preview

- [ ] Select an image item.
- [ ] Press `Right Arrow`.
- [ ] Confirm an image-only floating preview opens centered at about 50% display size.
- [ ] Press `Right Arrow` again and confirm the preview remains a single fixed preview rather than zooming.
- [ ] Press `Left Arrow` and confirm the preview closes.
- [ ] Open the preview again, press `Escape`, and confirm only the preview closes.
- [ ] Open the preview again, then press `Enter`.
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
- Screenshots or screen recordings for visual/layout issues.
