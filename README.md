# busy

A small, dependency-light system monitor for Windows 11, in the spirit of macOS [Stats](https://github.com/exelban/stats): live CPU, memory, GPU, network, disk, battery and sensor readings embedded directly in the taskbar, with a detailed flyout on click.

- Native Win32 + Direct2D, single `.exe`, no installer, no admin rights, no kernel driver, no network access unless you turn on the update check.
- Only the `windows` crates, `serde` and `serde_json`.
- CPU temperatures are read from [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) or HWiNFO (shared memory) if you run one and turn on Settings → Advanced → Third-party sensor tools; GPU temperatures work out of the box on NVIDIA/AMD/WDDM 2.5+.

## Install

1. Download `busy-X.Y.Z-x64.zip` (or just `busy.exe`) from [Releases](https://github.com/aivanyuk/busy/releases).
2. Optionally check the download: compare `certutil -hashfile busy.exe SHA256` with `SHA256SUMS`, or run `gh attestation verify busy.exe --repo aivanyuk/busy`, which proves it was built by this repository's release workflow.
3. Put `busy.exe` where it can stay, for example `%LOCALAPPDATA%\Programs\busy\`, and run it. The first start opens a short setup: pick the readings, pick a side of the taskbar.

Releases aren't code-signed yet, so for a new download Windows may show "Windows protected your PC": choose **More info → Run anyway**.

**Upgrade:** right-click the readings → Exit, replace `busy.exe`, start it again. Settings carry over, and "Start with Windows" follows the exe if you moved it. Settings → General → About shows your version; turn on Settings → Advanced → Check for updates to have it name a newer release (busy then asks GitHub once a day; it never downloads anything).

**Uninstall:** turn off Settings → General → Start with Windows, right-click the readings → Exit, then delete `busy.exe` and `%APPDATA%\busy`.

## Build

Requires the MSVC Rust toolchain (pinned in `rust-toolchain.toml`) and Visual Studio Build Tools.

```
cargo build -p busy --release
target\release\busy.exe
```

Right-click the widget for Settings / Exit. Config lives in `%APPDATA%\busy\config.json`.

## Contributing

See [docs/code-quality.md](docs/code-quality.md), [docs/testing.md](docs/testing.md), [docs/commits.md](docs/commits.md) and [docs/architecture.md](docs/architecture.md).

## License

[MIT](LICENSE)
