# Better Clipboard

Better Clipboard is a small macOS clipboard history app written in Rust.

It watches `NSPasteboard.changeCount` and only reads clipboard contents when macOS reports a change. Text, URLs, file paths, email addresses, phone numbers, and images are stored locally, and the palette can be opened with `Option+Space`, `Cmd+Option+Space`, `Cmd+Option+\`, or from the 📋 menu bar icon.

Escape, pressing the shortcut again, or clicking into another app cancels the palette without copying or pasting. Use the menu bar icon's Quit item to exit the app.

When the palette opens, the newest item is selected. Press Enter or double-click an item to copy it, hide Better Clipboard, reactivate the app that was focused before the palette opened, and send `Cmd+V`. If macOS blocks the synthetic paste event, the item is still on the clipboard.

Click an item's action button to run its default action: text copies to the clipboard, URLs open in the default browser, file paths reveal in Finder, email addresses open a mail composer, phone numbers open the system phone handler, and images open a larger floating preview. This does not change row behavior: pressing `Enter` or double-clicking any row still pastes that item into the previously active app. For type-specific keyboard actions, use `O` to open URL/file/email/phone items, `F` to reveal file paths in Finder, and `S` to copy the selected item ready for sharing.

Sensitive-looking values such as Luhn-valid credit card numbers and common API key formats are masked in the palette display. The original value is still kept as the clipboard payload so paste, copy, and share-prep actions work correctly.

macOS does not expose an event queue of past clipboard contents, so very rapid clipboard changes can still collapse to the newest available payload. Better Clipboard checks the change counter every 100 ms to catch normal copy flows without continuously reading clipboard contents.

## Shortcuts

- `Option+Space`: open the palette; press again to cancel.
- `Cmd+Option+Space`: open the palette; press again to cancel.
- `Cmd+Option+\`: open the palette; press again to cancel.
- `Enter`: paste the selected item into the previously focused app.
- `Double-click`: paste that item into the previously focused app.
- `Escape`: cancel without changing the clipboard.
- `Up` / `Down`: expand the list and move selection.
- `Right Arrow`: preview the selected image item.
- `O`: open the selected URL, file path, email address, or phone number.
- `F`: reveal the selected file path in Finder.
- `S`: copy the selected item so it is ready to share.
- `Cmd+Down` or `Tab`: expand the list.
- `Cmd+Up`: collapse the list.

See [docs/help.md](docs/help.md) for a fuller operating guide and [docs/manual-test-script.md](docs/manual-test-script.md) for release testing.

## Settings

Open settings from the `⚙` button in the palette or from the 📋 menu bar menu. Better Clipboard supports light and dark themes, a configurable clipboard history limit, and a `Run at login` toggle.

The paste flow works at the app level: Better Clipboard records the previously frontmost application before it shows the palette, reactivates that app after selection, then posts paste. macOS does not expose a general public API for restoring the exact focused text field in another process, but reactivating the previous app usually restores that app's key window and focused control.

Accessibility permission is required for automatic paste. Better Clipboard requests it on first launch and shows a small permission window if it is still missing.

## Run

```sh
cargo run
```

Clipboard history and settings are stored under the app's local data directory. By default, Better Clipboard keeps the most recent 100 items.

## Installable App

Build a local macOS app bundle:

```sh
./scripts/build-macos-app.sh
```

The app bundle is written to `dist/Better Clipboard.app`. Move it to `/Applications` if you want to run it like a normal macOS app.

The release binary is written to `target/release/better-clipboard`.

## Philosophy

Better Clipboard is built around solving the problem directly in front of you: keep the clipboard history close, make recall fast, and avoid turning a small utility into a platform.

AI tools make this style of software easier to build because you can move in small, concrete loops: try the workflow, notice the friction, change the exact behavior, and test again. The goal is not to generate a large product surface. The goal is to keep shaving the tool until the common action feels obvious.

## License

Better Clipboard is released under the BSD 2-Clause License.
