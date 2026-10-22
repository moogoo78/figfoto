# Next steps

## Package for macOS and Windows

Installers can't be built on Linux: macOS bundles need a Mac, and Windows cross-compiling is
experimental (and `libheif-rs` with `embedded-libheif` builds C++ via CMake, which makes it worse).
Build on each OS, or let GitHub Actions do it.

`"targets": "all"` in `src-tauri/tauri.conf.json` already gives `.dmg`/`.app` on macOS and
NSIS `.exe` + `.msi` on Windows.

### 1. Fix the bundle icons

`bundle.icon` only lists PNGs, though `icon.icns` and `icon.ico` already exist in `src-tauri/icons/`:

```json
"icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]
```

### 2a. Build by hand

macOS:

```sh
brew install cmake
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm install
pnpm tauri build --target universal-apple-darwin
# src-tauri/target/universal-apple-darwin/release/bundle/dmg/FigFoto_<ver>_universal.dmg
```

Windows (Visual Studio Build Tools with C++, CMake, Rust; WebView2 ships with Windows 10/11):

```sh
pnpm install
pnpm tauri build
# src-tauri/target/release/bundle/nsis/FigFoto_<ver>_x64-setup.exe
# src-tauri/target/release/bundle/msi/FigFoto_<ver>_x64_en-US.msi
```

### 2b. Build with GitHub Actions (preferred from Linux)

`.github/workflows/release.yml`:

```yaml
name: release
on:
  push:
    tags: ['v*']
  workflow_dispatch:

jobs:
  build:
    permissions:
      contents: write
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: macos-latest
            args: --target universal-apple-darwin
          - platform: windows-latest
            args: ''
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with: { node-version: lts/*, cache: pnpm }
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.platform == 'macos-latest' && 'aarch64-apple-darwin,x86_64-apple-darwin' || '' }}
      - uses: swatinem/rust-cache@v2
        with: { workspaces: src-tauri }
      - run: pnpm install --frozen-lockfile
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: v__VERSION__
          releaseName: FigFoto v__VERSION__
          releaseDraft: true
          args: ${{ matrix.args }}
```

Release with `git tag v0.1.0 && git push --tags`; a draft GitHub Release gets the `.dmg`, `.exe`
and `.msi`. CMake is preinstalled on both runner images.

### 3. Code signing (optional)

Unsigned builds work but warn users:

- **macOS** — downloaded unsigned apps show "damaged / can't be opened". Workaround: right-click →
  Open, or `xattr -cr /Applications/FigFoto.app`. Proper fix: Apple Developer ID ($99/yr) +
  notarization; tauri-action handles it with the `APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`,
  `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` secrets.
- **Windows** — SmartScreen "Windows protected your PC" until signed. Azure Trusted Signing is the
  cheapest route.

Fine to skip for personal use or a few friends.
