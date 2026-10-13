# Manual Test Script

Use this script for release testing before filing issues. The current release candidate should cover the core keyboard workflow, native Share, file-list capture, the compact-to-expanded scroll behavior, image capture, sensitive value masking, image preview, search, starred items, the text viewer, and the local API.

## Current Known Issues

- Native Share targets vary by macOS version and installed apps, so record item type and destination for Share failures.
- If automatic paste fails, the selected item should still remain on the clipboard for manual `Cmd+V`.

## Setup

- [ ] Download `Better-Clipboard-vX.X.X-macOS.zip` from the [Releases page](https://github.com/mgosal/better-clipboard/releases) and unzip it, **or** build from source with `./scripts/build-macos-app.sh`.
- [ ] Move `Better Clipboard.app` to `/Applications` and open it from there.
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
- [ ] Copy an image (e.g. right-click an image in Safari → Copy Image).
- [ ] Open Better Clipboard with `Option+Space`.
- [ ] Confirm all copied items appear in newest-first order.
- [ ] Confirm text, URL, file path, file-list, email, and phone rows are left-aligned.
- [ ] Confirm each row shows an action tile on the left and action buttons on the bottom-right.
- [ ] Confirm keyboard shortcut labels on action buttons appear in **bold**.
- [ ] Confirm metadata (type · time) appears bottom-left on the same row as the action buttons.
- [ ] Confirm the footer lists the shortcuts for the selected item, and that it changes when you select a URL, a file, an image, and a text item.
- [ ] Confirm image row tiles fill the row height.
- [ ] Note any row spacing, clipping, alignment, or hierarchy issues for a GitHub layout issue.
- [ ] Copy a test credit card number such as `4111 1111 1111 1111`.
- [ ] Confirm it is masked as `•••• •••• •••• 1111` in the row summary.
- [ ] Copy a test API key such as `sk-proj-abcdefghijklmnopqrstuvwxyz123456`.
- [ ] Confirm it is masked as `sk-proj-...3456` in the row summary.
- [ ] Confirm the original unmasked value is pasted when pressing `Enter`.

## Keyboard Navigation

- [ ] Press `Down` repeatedly through a long history list.
- [ ] Confirm the selected row stays visible as the list scrolls.
- [ ] Press `Up` repeatedly.
- [ ] Confirm the selected row stays visible while moving upward.
- [ ] Reopen in compact mode (default on open), scroll the mouse wheel once, and confirm the palette expands past 3 items without snapping back to the selected row.
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

## Starred Items

- [ ] Select an item and press `8`. Confirm a gold `★` appears in the row and the footer shows `8 Unstar`.
- [ ] Press `8` again and confirm the star clears.
- [ ] Click the `★` in another row and confirm it is starred.
- [ ] Click the `★` button in the header. Confirm only starred items are listed and the header says `N starred`.
- [ ] Click it again and confirm all items return.
- [ ] Set the history limit to 10, star an old item, then copy more than 10 new items. Confirm the starred item is still in history.
- [ ] Copy the starred item's content again and confirm the new top row is still starred.

## Text Viewer

- [ ] Dictate or copy several paragraphs of text.
- [ ] Select the item and press `Space`. Confirm a larger window shows the full text, with word and character counts in the header.
- [ ] Scroll to the end and confirm nothing is cut off.
- [ ] Select some words with the mouse and press `⌘C`. Confirm only the selection is on the clipboard.
- [ ] Press `Escape` and confirm only the viewer closes.
- [ ] Click **View** on a text row and confirm the viewer opens.
- [ ] Press `Enter` in the viewer and confirm the full text pastes into the original app.
- [ ] Open the viewer again, press `C`, and confirm the full text is copied and the palette closes.

## Local API

Use the curl commands in [local-api.md](local-api.md). Set `TOKEN` to the value from Settings → Copy.

- [ ] Open Settings and confirm **Let local apps send items** is off by default and `curl http://127.0.0.1:47823/v1/health` fails to connect.
- [ ] Turn it on. Confirm the status reads `Listening on 127.0.0.1:47823` and a masked token is shown.
- [ ] Allow notifications when macOS asks.
- [ ] Click **Copy** and paste the token into a terminal. Confirm the token does not appear in clipboard history.
- [ ] `GET /v1/health` with the token returns `200` and the app version.
- [ ] `POST /v1/clip` with `{"text":"482913","source":"Mail","label":"Example verification code","kind":"one-time-code","notify":true}` returns `201`.
- [ ] Confirm `482913` is on the system clipboard (`⌘V` in any app).
- [ ] Confirm a notification titled `482913 · copied to clipboard` appears, with the body `Example verification code (from Mail)`.
- [ ] Click the notification. Confirm the palette opens with the code selected, the tile shows `482913`, and the row reads `Code · from Mail · Example verification code · <time>`.
- [ ] Press `Enter` to paste it. Reopen the palette and confirm the code is gone from history.
- [ ] Send another code, then copy some other text. Confirm the code is removed from history.
- [ ] Send another code and wait 10 minutes. Confirm it is removed from history.
- [ ] Send `{"text":"hello from a script","source":"Terminal"}` and confirm it appears as a normal text row labelled `from Terminal`.
- [ ] Without the `Authorization` header, or with a wrong token, the request returns `401`.
- [ ] With `-H "Origin: https://example.com"`, the request returns `403`.
- [ ] Click **Regenerate**. Confirm the old token now returns `401` and the new one works.
- [ ] Turn the API off and on again quickly. Confirm the status returns to `Listening`.
- [ ] Turn the API off and confirm `/v1/health` fails to connect.

## Search

- [ ] Open Better Clipboard with `Option+Space`.
- [ ] Press `/` to activate search.
- [ ] Confirm the palette expands, a search bar appears with focus, and the `/` character does not appear in the search field.
- [ ] Type a query that matches a known clipboard item.
- [ ] Confirm matching items are shown and non-matching items are hidden.
- [ ] Confirm search matches against both the summary text and the raw clipboard content (e.g. a URL that was truncated in the summary).
- [ ] Press `Down` and `Up` to navigate filtered results.
- [ ] Press `Enter` to paste the selected filtered item into the previous app.
- [ ] Reopen and activate search again.
- [ ] Type a query, then press `Escape`.
- [ ] Confirm the search query clears but the palette stays open.
- [ ] Press `Escape` again.
- [ ] Confirm the palette closes.
- [ ] Reopen, activate search, type a query with no matches.
- [ ] Confirm a "No matches" message appears.

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

## Image Preview

- [ ] Select an image item.
- [ ] Press `Space` and confirm the preview opens; press `Space` again and confirm it closes.
- [ ] Press `Right Arrow`.
- [ ] Confirm an image-only floating preview opens centred at the image's native pixel size.
- [ ] Confirm a `✕ Esc` close button is visible in the top-right corner of the preview.
- [ ] Click the `✕ Esc` button and confirm the preview closes without closing the palette.
- [ ] Reopen the preview with `Right Arrow`.
- [ ] Press `Right Arrow` again and confirm the preview remains a single fixed window rather than zooming or opening a second preview.
- [ ] Press `Left Arrow` and confirm the preview closes.
- [ ] Open the preview again, press `Escape`, and confirm only the preview closes.
- [ ] Open the preview again, then press `Enter`.
- [ ] Confirm the preview and palette both close and the image is pasted into the original app.

## Settings

- [ ] Open settings with the `⚙` button.
- [ ] Switch between light and dark theme and confirm the palette updates immediately.
- [ ] Change the history limit and confirm it persists after restart.
- [ ] Enable `Run at login`.
- [ ] Confirm `~/Library/LaunchAgents/com.mgosal.better-clipboard.plist` is created.
- [ ] Disable `Run at login`.
- [ ] Confirm the LaunchAgent plist is removed.
- [ ] See [Local API](#local-api) for the API toggle and token controls.

## Tray Menu

- [ ] Right-click the 📋 menu bar item.
- [ ] Click `Show Better Clipboard` and confirm the palette opens.
- [ ] Click `Settings` and confirm settings opens directly.
- [ ] Click away from the palette and confirm it hides.
- [ ] Click `Quit Better Clipboard` and confirm the app exits.

## Issue Report Notes

When filing an issue, include:

- macOS version.
- Chip architecture (Apple silicon or Intel).
- App source: release zip from the Releases page, `dist/Better Clipboard.app` built locally, or `cargo run`.
- Whether Accessibility permission was granted.
- Exact shortcut or click sequence that triggered the issue.
- Expected result and actual result.
- Screenshots or screen recordings for visual or layout issues.
