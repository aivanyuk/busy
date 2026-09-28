# busy

A small, dependency-light system monitor for Windows 11, in the spirit of macOS [Stats](https://github.com/exelban/stats): live CPU, memory, GPU, network, disk, battery and sensor readings embedded directly in the taskbar, with a detailed flyout on click.

- Native Win32 + Direct2D, single `.exe`, no installer, no admin rights, no kernel driver.
- Only three crates: `windows`, `serde`, `serde_json`.
- CPU temperatures are read from [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) or HWiNFO (shared memory) if you run one; GPU temperatures work out of the box on NVIDIA/AMD/WDDM 2.5+.

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
