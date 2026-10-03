# Changelog

What changed for people who run busy, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/). Every pull request that changes what a user sees or gets adds its line under **Unreleased**; a release moves them under its version (`docs/releasing.md`).

## [Unreleased]

### Added
- The Network flyout lists the processes using the network most, by their open connections.

## [0.2.0] - 2026-10-01

### Added
- A Microsoft Store package of the same busy, which installs without the SmartScreen warning and is updated by the Store. Installed from there, busy keeps its own settings (apart from a portable busy's), starts with Windows through the package's startup task, and has no update check or Releases link, as the Store updates it.
- A vertical taskbar (Windows 11 26H2, on the left or right edge): readings stack top to bottom as narrow columns, their flyouts open beside the taskbar, and Settings, setup and the right-click menu speak of the top instead of the left edge.

### Fixed
- With left-aligned (or top-aligned) taskbar icons, readings placed after them no longer cover the last icons.
- Readings in the taskbar keep their width as their values change, so a CPU going from 9% to 10% or 100% no longer nudges every reading after it.

## [0.1.0] - 2026-09-30

The first release.

### Added
- Readings in the taskbar, next to the tray or at its left edge: CPU, memory, GPU, network, disk, battery and a temperature sensor, each as text, a graph, a bar or read/write rates, in its own color or colored by load, updated at its own interval.
- A flyout for each reading, opened by clicking it: a chart of the last minutes, the details (per-core load, memory composition, drive response time, Wi-Fi signal, battery health and more), the busiest processes, and links to Task Manager and the reading's settings.
- A Settings window drawn to the Windows 11 design, with search, keyboard navigation, a live preview of each reading, and screen-reader support (UI Automation).
- First-run setup: pick the readings and a side of the taskbar, and see the taskbar follow as you choose.
- Start with Windows, which keeps working when busy.exe is moved.
- Opt-in data sources, off until you turn them on, each with its risk stated: third-party sensor tools (LibreHardwareMonitor, HWiNFO) for CPU temperatures, fans and power.
- Light and dark themes following Windows, or set in Settings.
- The version in Settings → General → About, with a button to the releases page.
- An opt-in update check (Settings → Advanced, off by default): once a day it asks GitHub whether a newer release exists and names it in About. It never downloads or installs anything.

### Security
- No administrator rights, no driver. No network access unless you turn on the update check. DLLs load from System32 only.
