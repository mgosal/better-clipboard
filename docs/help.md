# Better Clipboard Help

Better Clipboard is a macOS clipboard history palette. It keeps recent text, URLs, and images available from the menu bar and from global shortcuts.

## Core Workflow

1. Copy text, a URL, or an image in any app.
2. Press `Option+Space` to open Better Clipboard.
3. The newest clipboard item is selected automatically.
4. Press `Enter` to paste it back into the app you were using.
5. Press `Down` to expand the list and move through older items.

When an item is selected with `Enter` or double-click, Better Clipboard puts that item on the clipboard, hides the palette, reactivates the app that was focused before Better Clipboard opened, and sends `Cmd+V`. If paste input is blocked by macOS permissions, the item remains on the clipboard so you can paste manually.

Click an item's thumbnail or type glyph to copy it to the clipboard without pasting. That also moves the item to the top of the history so it behaves like the newest copied item.

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
| `Right Arrow` | Open a larger floating preview for the selected image item. |
| `Cmd+Down` | Expand the list. |
| `Tab` | Expand the list. |
| `Cmd+Up` | Collapse the list. |

## Palette

The compact palette shows the newest clipboard items with fixed-height, left-aligned rows and small thumbnails for images. The selected row is the item that will paste when you press `Enter`.

The list automatically scrolls as you move the selected row with the keyboard.

Use the chevron button to expand or collapse the list. Expanded mode shows more history at once, which is useful when searching visually through recent clips.

Use the `⚙` button to open settings.

Clicking into another app hides the palette.

## Image Preview

Select an image item and press `Right Arrow` to open a larger floating preview. The preview is scaled to about 50% of the original image size, capped so it stays practical on screen.

While the preview is open:

- `Escape` closes the preview and keeps the palette open.
- `Enter` closes the preview, hides the palette, and pastes the image into the previously focused app.

## Settings

Better Clipboard currently supports:

- Light or dark theme.
- Clipboard history limit from 10 to 1000 items.
- Run at login.

The history limit is applied immediately. If you reduce the limit, older entries beyond the new limit are removed, including saved image files for those entries.

Run at login writes or removes `~/Library/LaunchAgents/com.mgosal.better-clipboard.plist`.

## Permissions

Better Clipboard needs Accessibility permission for automatic paste. It requests that permission on first launch. If permission is still missing, open Settings and click `Request Accessibility Permission`.

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
