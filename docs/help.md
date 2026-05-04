# Better Clipboard Help

Better Clipboard is a macOS clipboard history palette. It keeps recent text, URLs, file paths, email addresses, phone numbers, and images available from the menu bar and from global shortcuts.

## Core Workflow

1. Copy text, a URL, a file path, an email address, a phone number, or an image in any app.
2. Press `Option+Space` to open Better Clipboard.
3. The newest clipboard item is selected automatically.
4. Press `Enter` to paste it back into the app you were using.
5. Press `Down` to expand the list and move through older items.

When an item is selected with `Enter` or double-click, Better Clipboard puts that item on the clipboard, hides the palette, reactivates the app that was focused before Better Clipboard opened, and sends `Cmd+V`. If paste input is blocked by macOS permissions, the item remains on the clipboard so you can paste manually.

Click an item's action button to run its default action:

- Text copies to the clipboard without pasting. That also moves the item to the top of the history so it behaves like the newest copied item.
- URLs open in the default browser.
- File paths reveal in Finder.
- Email addresses open a mail composer.
- Phone numbers open the system phone handler.
- Images open the larger floating preview.

The action button is separate from row activation. Pressing `Enter` or double-clicking any row still pastes that item into the previously active app.

Each row includes compact hints for the selected item's useful actions, such as `O` to open a URL or file, `F` to reveal a file path in Finder, and `S` to copy the item ready for sharing.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `Option+Space` | Open the palette. If it is already open, cancel it. |
| `Cmd+Option+Space` | Open the palette. If it is already open, cancel it. |
| `Cmd+Option+\` | Open the palette. If it is already open, cancel it. |
| `Enter` | Paste the selected history item into the previously focused app. |
| `Double-click` | Paste the clicked history item into the previously focused app. |
| `Escape` | Close the palette without copying or pasting. |
| `Up` / `Down` | Expand the list and move the selected item. |
| `Right Arrow` | Open a 50% image preview, or zoom an open image preview to 100%. |
| `Left Arrow` | Step an image preview back from 100% to 50%, or close it from 50%. |
| `C` | Copy the selected item without pasting. |
| `O` | Open the selected URL, file path, email address, or phone number. |
| `F` | Reveal the selected file path in Finder. |
| `S` | Copy the selected item so it is ready to share. |
| `Cmd+Down` | Expand the list. |
| `Tab` | Expand the list. |
| `Cmd+Up` | Collapse the list. |

## Palette

The compact palette shows the newest clipboard items with fixed-height, left-aligned rows and an action button for each item. The selected row is the item that will paste when you press `Enter`.

Each row keeps the clipboard data on the left, item metadata underneath it, and compact action buttons in the bottom-right corner. Each button is clickable and includes its keyboard shortcut underneath the icon, so the row can be operated with either mouse or keyboard.

The list automatically scrolls as you move the selected row with the keyboard.

Use the chevron button to expand or collapse the list. Expanded mode shows more history at once, which is useful when searching visually through recent clips.

Use the `⚙` button to open settings.

Clicking into another app hides the palette.

## Item Types

Better Clipboard recognizes:

- Text: click the action button or press `S` to copy it without pasting.
- URL: click the action button or press `O` to open it.
- File path: click the action button or press `F` to reveal it in Finder; press `O` to open it.
- Email: click the action button or press `O` to compose an email.
- Phone: click the action button or press `O` to hand it to macOS as a `tel:` link.
- Image: click the action button or press `Right Arrow` to preview it.

`Enter` and double-click keep the normal clipboard-history behavior for every item type: copy the item, close the palette, reactivate the previous app, and paste.

## Sensitive Display

Better Clipboard masks sensitive-looking display summaries for common API key prefixes, long secret-like values next to labels such as `api_key` or `token`, and Luhn-valid credit card numbers.

Masking only affects the row text shown in the palette. The raw clipboard payload is kept locally so paste and copy actions still use the original value.

## Image Preview

Select an image item and press `Right Arrow` to open a centered image-only floating preview. The first preview is scaled to about 50% of the original display size, accounting for Retina scaling, and capped so it stays practical on screen. Press `Right Arrow` again to move to 100% scale.

While the preview is open:

- `Left Arrow` steps the preview from 100% back to 50%, then closes it from 50%.
- `Escape` follows the same path as `Left Arrow`.
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
