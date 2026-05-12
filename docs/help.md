# Better Clipboard Help

Better Clipboard is a macOS clipboard history palette. It keeps recent text, URLs, file paths, file-list clipboard entries, email addresses, phone numbers, and images available from the menu bar and from a global shortcut.

## Current Status

The main device workflow is the supported path right now: open the palette, move through history with the keyboard, press `Enter`, and paste into the previously focused app.

Native Share now opens the macOS share sheet above the palette. File-list clipboard entries are stored as references to the original files, so Better Clipboard does not make extra file copies.

## Core Workflow

1. Copy text, a URL, a file path, files from Finder, an email address, a phone number, or an image in any app.
2. Press `Option+Space` to open Better Clipboard.
3. The newest clipboard item is selected automatically.
4. Press `Enter` to paste it back into the app you were using.
5. Press `Down` to expand the list and move through older items.

When an item is selected with `Enter` or double-click, Better Clipboard puts that item on the clipboard, hides the palette, reactivates the app that was focused before Better Clipboard opened, and sends `Cmd+V`. If paste input is blocked by macOS permissions, the item remains on the clipboard so you can paste manually.

Click an item's left type tile to run its default action:

- Text copies to the clipboard without pasting. That also moves the item to the top of the history so it behaves like the newest copied item.
- URLs open in the default browser.
- File paths and Finder file-list entries reveal in Finder.
- Email addresses open a mail composer.
- Phone numbers open the system phone handler.
- Images open the larger floating preview.

The action button is separate from row activation. Pressing `Enter` or double-clicking any row still pastes that item into the previously active app.

Each row also includes bottom-right action buttons for the selected item's useful actions, such as Paste/`Enter`, Copy/`C`, Open/`O`, Finder/`F`, Preview/`Right`, and Share/`S`. Share opens the macOS share sheet and keeps Better Clipboard open after the sheet is dismissed.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `Option+Space` | Open the palette. If it is already open, cancel it. |
| `/` | Open the search bar to filter clipboard history. |
| `Enter` | Paste the selected history item into the previously focused app. |
| `Double-click` | Paste the clicked history item into the previously focused app. |
| `Escape` | Clear search query, or close the palette if search is empty. |
| `Up` / `Down` | Expand the list and move the selected item. |
| `Right Arrow` | Open a centered image preview for the selected image item. |
| `Left Arrow` | Close the image preview. |
| `C` | Copy the selected item without pasting, then close the palette. |
| `O` | Open the selected URL, file path, email address, or phone number. |
| `F` | Reveal the selected file path or file-list item in Finder. |
| `S` | Open the macOS share sheet for the selected item. |
| `Cmd+Down` | Expand the list. |
| `Tab` | Expand the list. |
| `Cmd+Up` | Collapse the list. |

## Search

Press `/` while the palette is open to activate the search bar. The palette expands automatically and the search field receives focus.

Type to filter clipboard history. Search matches against both the display summary and the raw clipboard text, so URLs, code snippets, and other content that may be truncated or masked in the summary row are still findable.

While search is active:

- `Up` / `Down` navigate through the filtered results.
- `Enter` pastes the selected match into the previously focused app.
- `Escape` clears the search query. Press `Escape` again (or when the query is already empty) to close the palette.

A `/ to search` hint is shown in the top-right corner of the palette when search is not active.

## Palette

The compact palette shows the newest clipboard items with fixed-height, left-aligned rows and an action button for each item. The selected row is the item that will paste when you press `Enter`.

Each row keeps the clipboard data on the left, then places item metadata in the bottom-left strip and compact action buttons in the bottom-right strip. Each button is clickable and includes its keyboard shortcut underneath the icon, so the row can be operated with either mouse or keyboard.

The list automatically scrolls as you move the selected row with the keyboard. Mouse wheel scrolling expands the compact palette on the first scroll and then leaves the scroll position under your control.

Use the chevron button to expand or collapse the list. Expanded mode shows more history at once, which is useful when searching visually through recent clips.

Use the `⚙` button to open settings.

Clicking into another app hides the palette.

## Item Types

Better Clipboard recognizes:

- Text: click the left type tile to copy it and move it to the top of history, or press `C` to copy it without moving it to the top.
- URL: click the action button or press `O` to open it.
- File path: click the action button or press `F` to reveal it in Finder; press `O` to open it.
- Files: copy files from Finder to store file references; press `F` to reveal the first file in Finder, `O` to open the files, or `S` to share them.
- Email: click the action button or press `O` to compose an email.
- Phone: click the action button or press `O` to hand it to macOS as a `tel:` link.
- Image: click the action button or press `Right Arrow` to preview it.

`Enter` and double-click keep the normal clipboard-history behavior for every item type: copy the item, close the palette, reactivate the previous app, and paste.

## Sensitive Display

Better Clipboard masks sensitive-looking display summaries for common API key prefixes, long secret-like values next to labels such as `api_key` or `token`, and Luhn-valid credit card numbers.

Masking only affects the row text shown in the palette. The raw clipboard payload is kept locally so paste and copy actions still use the original value.

## Image Preview

Select an image item and press `Right Arrow` to open a centered image-only floating preview. The preview is scaled to 100% of the original display size, accounting for Retina scaling, and capped so it stays practical on screen.

While the preview is open:

- `Left Arrow` closes the preview.
- `Escape` closes the preview.
- `Enter` closes the preview, hides the palette, and pastes the image into the previously focused app.

## Settings

Better Clipboard currently supports:

- Light or dark theme.
- Clipboard history limit from 10 to 1000 items.
- Run at login.

The history limit is applied immediately. If you reduce the limit, older entries beyond the new limit are removed, including saved image files for those entries.

Run at login writes or removes `~/Library/LaunchAgents/com.mgosal.better-clipboard.plist`.

## Permissions

Better Clipboard needs Accessibility permission for automatic paste. On first launch, it shows its own permission window first, then asks macOS for Accessibility permission after that window is visible. If permission is still missing later, open Settings and click `Request Accessibility Permission`.

No special permission is needed to read the clipboard change count. macOS may still show its own clipboard privacy prompts depending on system version and launch context.

## Menu Bar

The menu bar item uses the 📋 icon. Open it to show Better Clipboard, open settings, or quit the app.

Closing the palette window only hides it. Use the menu bar Quit item to exit the background clipboard watcher.

## Paste And Focus Behavior

Better Clipboard records the frontmost application immediately before showing the palette. On paste, it hides itself, reactivates that application, waits briefly for focus to settle, and posts `Cmd+V`.

macOS does not provide a general public API for saving and restoring the exact focused text field inside another app. In practice, the previous app usually restores its key window and focused control when reactivated. This is the same style of workflow used by launcher tools: keep the previous app identity, close the launcher, reactivate the app, then synthesize paste.

If paste does not happen automatically, check System Settings > Privacy & Security > Accessibility and allow Better Clipboard or the terminal/app used to launch it. Without that permission, macOS can ignore simulated keyboard input.

## Clipboard Monitoring

Better Clipboard uses `NSPasteboard.changeCount` rather than continuously reading the clipboard. macOS increments that counter when the pasteboard changes, so Better Clipboard polls the counter every 100 ms and only reads contents after a change.

macOS does not expose a complete event queue of clipboard payloads. If another app writes several values extremely quickly between checks, only the newest available payload may be captured.

## Known Issues

- Native Share should be tested across item types because available share targets are controlled by macOS and installed apps.
- If automatic paste fails, the selected item should still be on the clipboard for manual `Cmd+V`.
