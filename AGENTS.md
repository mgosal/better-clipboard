# Better Clipboard — Agent Rules

## Release Process

After any code change that fixes an issue or adds a feature:

1. **Bump version** in `Cargo.toml` (patch for fixes, minor for features).
2. **Update `Info.plist` version** in `scripts/build-macos-app.sh` to match.
3. **Run the full test suite**: `cargo test`
4. **Build the macOS app bundle**: `bash scripts/build-macos-app.sh`
5. **Create the release zip**: `cd dist && zip -r "Better-Clipboard-v<VERSION>-macOS.zip" "Better Clipboard.app"`
6. **Cut a GitHub release** with the zip attached:
   ```
   gh release create v<VERSION> dist/Better-Clipboard-v<VERSION>-macOS.zip \
     --title "Better Clipboard v<VERSION>" \
     --notes "<changelog>"
   ```
7. **Commit the version bump and AGENTS.md changes** before creating the release tag.

Every merged fix or feature must produce a GitHub release with a downloadable `.app` artifact. Do not skip this step.
