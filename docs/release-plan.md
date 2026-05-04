# Release Plan

## Current Recommendation

Do not tag the current head as `v1.0.0` while the row layout and Share action are known issues.

The best candidate for a first `v1.0.0` tag is:

```text
fba8fc0 Initial Better Clipboard release
```

That commit is the first complete public baseline with the app bundle script, docs, license, CI, issue template, clipboard capture, shortcuts, paste flow, settings, and manual test script in place. The later commits are useful product iterations, but they also introduced ongoing row-layout and Share-button work.

## Tag Command

When ready to publish that baseline as v1:

```sh
git tag -a v1.0.0 fba8fc0 -m "Better Clipboard v1.0.0"
git push origin v1.0.0
```

## Current Head

Current head is better for day-to-day keyboard workflow testing, but should be treated as post-v1 iteration until these issues are resolved:

- Row layout polish.
- Share button and `S` shortcut reliability.
- Any remaining paste-focus edge cases found during manual testing.

## Next Release Candidate Criteria

A later `v1.1.0` or replacement `v1.0.0` candidate should pass:

- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `./scripts/build-macos-app.sh`
- Full manual test script, except for explicitly deferred known issues.
