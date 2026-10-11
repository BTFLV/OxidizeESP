# Building and releasing

## Local builds

From `apps/desktop`:

```sh
npm run tauri build                                              # current platform
npm run tauri build -- --target aarch64-apple-darwin --bundles dmg
npm run tauri build -- --target x86_64-apple-darwin --bundles dmg
npm run tauri build -- --target x86_64-pc-windows-msvc --bundles nsis   # on Windows
```

Install the Rust target first, for example `rustup target add x86_64-apple-darwin`. Both macOS architectures can
be built on either type of Mac. Installers are written to `target/<target>/release/bundle/`.

Separate DMGs are produced for Apple Silicon and Intel; no universal binary is built.

## Rolling releases

`.github/workflows/rolling-release.yml` publishes the `rolling` prerelease named **OxidizeESP Rolling Release**:

1. The workflow starts when CI succeeds for a push to `main` (or manually, in which case it runs the CI checks
   itself).
2. The three platforms are built in parallel from the validated commit and uploaded as workflow artifacts.
   A newer commit cancels builds of an older one.
3. After all builds succeeded, the publish job verifies that every installer is present, computes
   `SHA256SUMS.txt` and checks that the commit is still the newest on `main`.
4. All files are uploaded to a draft release first. Only when the draft is complete is the `rolling` tag moved to
   the commit, the previous rolling release removed and the draft published.

Publishing jobs never run concurrently and are never cancelled midway. A failed build leaves the previous release
untouched, and an older commit never replaces a newer one. The workflow only needs the repository's
`GITHUB_TOKEN` with `contents: write`, which works for private repositories.

Published files:

| File | Platform |
| --- | --- |
| `OxidizeESP-macos-arm64.dmg` | macOS, Apple Silicon |
| `OxidizeESP-macos-x64.dmg` | macOS, Intel |
| `OxidizeESP-windows-x64-setup.exe` | Windows x64 (NSIS, per-user install) |
| `SHA256SUMS.txt` | SHA-256 checksums of the files above |

## Code signing

The macOS release builds are signed and notarized with the credentials described below. Without credentials, the
macOS app is only ad-hoc signed (`signingIdentity: "-"`) and not notarized, so Gatekeeper warns on first launch.
The Windows installer is never signed.

### macOS signing and notarization

Add these repository secrets; the workflow exports them for the Tauri bundler only when they are set:

| Secret | Content |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` export of a Developer ID Application certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Password of the `.p12` file |
| `APPLE_SIGNING_IDENTITY` | Identity name, for example `Developer ID Application: Name (TEAMID)` |
| `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_CONTENT` | Notarization with an App Store Connect API key: key ID, issuer ID and the full text of the `.p8` file (preferred) |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Alternative notarization with an Apple ID, app-specific password and team ID |

See the [Tauri macOS signing guide](https://v2.tauri.app/distribute/sign/macos/).

### Windows

The Windows installer is not code signed. SmartScreen may warn about an unrecognized app when it is run for the
first time.
