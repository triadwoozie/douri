# Releasing Dōri

This guide documents the end-to-end release checklist, package generation, and GitHub Release template for Dōri.

## 1. Prerequisites

Ensure the following tools are installed in `PATH` (`~/.cargo/bin` and `~/.local/bin`):

- Rust toolchains & targets:
  - `x86_64-unknown-linux-gnu`
  - `x86_64-pc-windows-gnu`
- `x86_64-w64-mingw32-gcc` (MinGW-w64 cross-compiler for Windows builds)
- `appimagetool` (for `.AppImage` installer packaging)
- `rpmbuild` (for `.rpm` packaging)
- `zip` and `sha256sum`
- `gh` (GitHub CLI, authenticated)

## 2. Version Bump & Changelog

1. Update the workspace crate version (`0.1.0-alpha.N`) across:
   - `crates/*/Cargo.toml`
   - `xtask/Cargo.toml`
   - `xtask/src/package_installers.rs` (`const VERSION`)
   - `crates/prowave-app/src/mcp_server.rs`
   - `crates/douri-installer/src/main.rs`
2. Add a new `## [0.1.0-alpha.N] - YYYY-MM-DD` section to `CHANGELOG.md`.
3. Refresh `Cargo.lock` and verify workspace gates:
   ```bash
   cargo fmt --all
   cargo test -p douri every_language_translates_every_text -- --nocapture
   cargo xtask check
   cargo xtask status
   ```

## 3. Build Release Artifacts

Run the `xtask` packaging targets from the repository root:

```bash
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
cargo xtask package-installers
cargo xtask package-rpm
```

This produces all release packages in `dist/`:

- `Dori-Setup-<VERSION>-x86_64.exe` (Windows x86_64 guided dark minimal installer)
- `Dori-<VERSION>-win64-portable.zip` (Windows x86_64 portable zip)
- `Dori-Setup-<VERSION>-x86_64.AppImage` (Linux x86_64 standalone AppImage installer)
- `Dori-<VERSION>-linux-x86_64-portable.tar.gz` (Linux x86_64 portable tarball)
- `dori-<VERSION>-1.x86_64.rpm` (Fedora / RHEL / openSUSE RPM)
- `SHA256SUMS.txt` (cryptographic SHA-256 checksums for all 5 packages)

Verify `dist/SHA256SUMS.txt` matches the generated files (`(cd dist && sha256sum -c SHA256SUMS.txt)`).

## 4. Commit, Tag, and Publish GitHub Release

1. Commit and tag:
   ```bash
   git add -A
   git commit -m "chore(release): bump version to <VERSION>"
   git tag -a "v<VERSION>" -m "Dōri v<VERSION>"
   git push origin main
   git push origin "v<VERSION>"
   ```
2. Update `dist/release-notes.md` using the standard release template:
   - Heading: `### Dōri v<VERSION>`
   - Intro: `Welcome to Dōri (道理) v<VERSION>, a pure-Rust cross-platform digital audio workstation.`
   - `#### Downloads & Release Packages` table listing all 6 assets (`Dori-Setup-<VERSION>-x86_64.exe`, `Dori-<VERSION>-win64-portable.zip`, `Dori-Setup-<VERSION>-x86_64.AppImage`, `Dori-<VERSION>-linux-x86_64-portable.tar.gz`, `dori-<VERSION>-1.x86_64.rpm`, `SHA256SUMS.txt`).
   - `## What's new in <VERSION>` grouped by feature area.
   - Footer: `Licensed under the GNU General Public License v3 or later.`
3. Publish the GitHub Release with all 6 assets attached:
   ```bash
   gh release create "v<VERSION>" \
     --title "Dōri v<VERSION>" \
     --notes-file dist/release-notes.md \
     dist/Dori-Setup-<VERSION>-x86_64.exe \
     dist/Dori-<VERSION>-win64-portable.zip \
     dist/Dori-Setup-<VERSION>-x86_64.AppImage \
     dist/Dori-<VERSION>-linux-x86_64-portable.tar.gz \
     dist/dori-<VERSION>-1.x86_64.rpm \
     dist/SHA256SUMS.txt
   ```
