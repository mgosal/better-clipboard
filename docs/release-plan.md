# Release Plan

## Current Recommendation

Do not tag a new release until the manual test script has been run against the app bundle built from the target commit.

The historical candidate for a first `v1.0.0` tag is still:

```text
fba8fc0 Initial Better Clipboard release
```

That commit is the first complete public baseline with the app bundle script, docs, license, CI, issue template, clipboard capture, shortcuts, paste flow, settings, and manual test script in place.

Current head is the better day-to-day candidate if the latest row layout, native Share, file-list capture, single image preview, and mouse-scroll behavior pass manual testing.

## Tag Command

When ready to publish that baseline as v1:

```sh
git tag -a v1.0.0 fba8fc0 -m "Better Clipboard v1.0.0"
git push origin v1.0.0
```

## Current Head

Current head should be treated as a release candidate only after these behaviors are manually verified:

- Row metadata sits bottom-left, action buttons sit bottom-right, and image tiles fill row height.
- Mouse wheel scrolling expands compact mode and does not snap back to the selected row.
- Share button and `S` shortcut open the native macOS share sheet and leave Better Clipboard open when the sheet is dismissed.
- Finder file-list clipboard entries are captured as file references, open with `O`, reveal with `F`, share with `S`, and paste as file URLs.
- Any remaining paste-focus edge cases found during manual testing.

## Next Release Candidate Criteria

A later `v1.1.0` or replacement `v1.0.0` candidate should pass:

- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `./scripts/build-macos-app.sh`
- Full manual test script, except for explicitly deferred known issues.
