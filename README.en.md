# ZDiskCleaner Pro · Windows Storage Management

[中文说明](README.md) · [Windows releases](https://github.com/yusheng266186-beep/zdisk-cleaner-pro/releases) · [Changelog](CHANGELOG.md)

A Windows disk management application for finding large or duplicate files, reviewing cleanup candidates, relocating applications, and recovering staged changes.

## Install and start

Download the installer or portable ZIP from the matching release. The desktop UI requires WebView2. Begin with a scan and review each candidate against the shown location, rule, and estimated size. For cleanup that supports staging, use the vault and check the resulting history entry before deciding on restore or permanent removal.

The app stores its journals and settings on the local machine. Cleanup rules and recovery design are explained in [docs/rules.md](docs/rules.md) and [docs/HANDOVER.md](docs/HANDOVER.md).

## Repository map

| Path | Responsibility |
| --- | --- |
| <code>crates/zc-core</code> | Scan, guard, apply, and recovery logic |
| <code>crates/zc-rules</code> | Built-in rule definitions and validation |
| <code>crates/zc-cli</code> | Headless interface for the core |
| <code>src-tauri/</code> | Windows integration, commands, and progress events |
| <code>ui/</code> | React interface and local demonstration data |

Build the command-line application with <code>cargo build --release -p zc-cli</code>. Preview the front end with <code>pnpm --dir ui install</code> followed by <code>pnpm --dir ui dev</code>. Use an isolated Windows test environment and synthetic files when verifying cleanup, relocation, or restore. See the Chinese README for tooling, exit codes, and release verification. The code uses the [MIT License](LICENSE).
