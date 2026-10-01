# Plan: Microsoft Store

Goal: busy installs from the Microsoft Store with no SmartScreen prompt, and keeps shipping as a portable exe on GitHub Releases. Builds on the release pipeline (`docs/plans/release.md`).

## Why the Store

A new unsigned download gets "Windows protected your PC" until it builds SmartScreen reputation, and every release is a new hash that starts over. Smart App Control blocks unsigned apps outright. A Store package is signed by Microsoft when it is published, so none of this applies to it. Signing the GitHub exe as well (Azure Artifact Signing, SignPath Foundation, an OV certificate in a CA's cloud HSM) is a separate decision; EV certificates no longer skip the reputation check (2024), so they aren't worth their price.

## Decisions

- **One exe, two channels.** The Store package (MSIX) and the GitHub zip carry the same `busy.exe`. It asks at runtime whether it runs from a package (`GetCurrentPackageFullName` succeeds) and adjusts the few things that differ; there are no build flags or features.
- **Listing name "busy — system monitor"**, reserved in Partner Center under an individual developer account. The package identity name and publisher come from Partner Center and go into the manifest as they are.
- **Full-trust desktop app** (`runFullTrust`, a restricted capability justified in the submission), **x64 only**, as the exe. Not an AppContainer: the widget is a child of explorer's `Shell_TrayWnd` and reads the same system APIs as the portable exe. Named kernel objects aren't isolated for full-trust apps, so the `Local\busy-single-instance` mutex keeps a Store and a portable busy from running at once.
- **Separate settings.** The packaged busy keeps `config.json` in its package's `LocalState` folder (`%LOCALAPPDATA%\Packages\<family name>\LocalState`), named explicitly rather than left to file-system virtualization: virtualization redirects only *new* files under `%APPDATA%`, and modifies existing ones in place, so a portable busy's `%APPDATA%\busy\config.json` would otherwise be shared. Uninstalling the package removes its settings.
- **Autostart through the package's startup task.** A packaged app's writes to `HKCU\...\Run` land in its private registry copy and start nothing. The manifest declares a `windows.startupTask` (`TaskId` "busy", disabled by default) and Settings → Start with Windows uses `Windows.ApplicationModel.StartupTask` when packaged; the Run-entry repair at startup (R2) is skipped. A task the user turned off in Windows Settings (`DisabledByUser`) can't be turned on by the app; the toggle's error message says where to turn it on instead.
- **No update check, no Releases link in the Store build.** The Store updates the package; Settings → General → About shows the version only, and Settings → Advanced doesn't list the update check. With that, the Store build makes no network requests.
- **Package version `X.Y.Z.0`**, from the workspace version (the Store reserves the fourth part). Every Store submission needs a higher version than the last, as tags already do.
- **The Store signs the package.** CI packs it unsigned; it isn't a Release asset, as nobody can install an unsigned MSIX. The first submission is made by hand in Partner Center; automating later ones (`msstore` CLI with a Partner Center app registration) is a later, optional step.
- No new crates: `StartupTask` and the package APIs are `windows` crate features (`ApplicationModel`, `Win32_Storage_Packaging_Appx`); `windows-future`, for waiting on a WinRT async call, is already in `Cargo.lock`. `makeappx` comes with the Windows SDK on the runner.

## Work, as PRs

### S1. Packaged or portable, decided at runtime (`win`, `core`, `settings`)
- `busy_win::package_family_name() -> Option<String>`, `None` when not packaged.
- `Config::path()`: the package's `LocalState\config.json` when packaged, `%APPDATA%\busy\config.json` otherwise.
- `autostart`: `StartupTask` when packaged (`GetAsync`, `RequestEnableAsync`, `Disable`, waited on by the settings worker, never the UI thread); the Run entry otherwise. `DisabledByUser` and `DisabledByPolicy` come back as the error the window already shows, naming Windows Settings → Apps → Startup.
- Settings: the update check and the Releases link are hidden when packaged; a stored `update_check: true` is ignored there.
- Unit tests for the path and the mapping of `StartupTask` states; `docs/areas/settings.md` and `core.md` gain the packaged cases.

### S2. Package layout (`build`, `tools`)
- `app/AppxManifest.xml`, a template next to `app.manifest`: identity `tmik.busysystemmonitor` / `CN=4F8212EC-…` and publisher display name `tmik` from Partner Center, `X.Y.Z.0`, `DisplayName` "busy — system monitor", `Windows.FullTrustApplication` entry point `busy.exe`, `runFullTrust`, the startup task, `Windows.Desktop` from 10.0.22631.0 (23H2), the oldest build busy supports (docs/testing.md).
- Logos (`Square44x44Logo` with `targetsize-16…256` and `altform-unplated`, `Square150x150Logo`, `StoreLogo`, each at scale 100–400) rasterized from the app glyph by the same code the build script draws the icon with and encoded by its PNG writer, so there are still no binary assets in the repository. The build script writes them and the filled-in manifest to `OUT_DIR/msix` (`app/build/msix.rs`).
- `tools/pack-msix.ps1 [-Out <msix>] [-Layout]`: builds the release exe, finds that layout through cargo's JSON messages, adds the exe, `LICENSE` and `THIRD-PARTY-LICENSES.txt`, indexes the logos' qualifiers into `resources.pri` (`makepri`; without it Windows would only find unqualified file names) and runs `makeappx pack`. The SDK tools come from an installed Windows SDK (GitHub's runners) or the `Microsoft.Windows.SDK.BuildTools` NuGet package unpacked under `target\sdk-buildtools`, which needs no install.

### S3. Release workflow (`ci`)
- `build` packs `busy-X.Y.Z-x64.msix` from the exe it just built and uploads it with the run's artifacts; it isn't attached to the Release.
- `docs/releasing.md`: after publishing the GitHub Release, submit the run's `.msix` in Partner Center (Packages → replace, then Submit).

### S4. Checked as a package (`docs`)
- Register the layout without signing (`Add-AppxPackage -Register <layout>\AppxManifest.xml`; needs Developer Mode, a Windows setting the user turns on), then check: the widget embeds in the taskbar and follows explorer restarts; the flyout and Settings open; settings land in `LocalState` and survive a restart; Start with Windows turns the startup task on and off and the next sign-in starts busy; a portable busy started alongside exits at once.
- Afterwards turn the startup task off and `Remove-AppxPackage`, as with the Run entry today.
- `docs/testing.md` gains these steps.

### S5. Store listing (`docs`)
- `PRIVACY.md`: busy collects nothing and, from the Store, contacts nothing; sensor readers (LibreHardwareMonitor, HWiNFO) are local.
- Listing text and screenshots (from `docs/`), the `runFullTrust` justification ("a taskbar system monitor: embeds its widget in the Windows taskbar and reads system performance counters"), age rating, category Utilities & tools, all in [docs/store-listing.md](../store-listing.md), ready to paste into Partner Center.
- README → Install: the Store first, the portable zip second.

## Order

S1 and S2 in either order; S3 needs S2; S4 needs S1–S3; S5 any time, submitted after S4. The first Store release is the next version after these land.
