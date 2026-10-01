# Privacy

busy collects nothing and sends nothing. It has no account, no telemetry, no analytics, no crash reporting and no ads. This applies to busy installed from the Microsoft Store and to the portable `busy.exe` from GitHub Releases.

## What busy reads

busy reads your computer's own performance data to show it: CPU, memory, GPU, disk, network and battery readings, temperatures, and the names of the busiest processes. It reads them from Windows and from your graphics driver, on your computer. The readings are drawn on your screen and kept in memory for the chart history; they are never written to disk and never leave your computer.

**Third-party sensor tools** (Settings → Advanced, off by default): when you turn this on, busy also reads the sensor data that LibreHardwareMonitor (through WMI) or HWiNFO (through shared memory) publishes, if you run one of them. This is read locally, from the program you installed; busy doesn't contact it over a network.

## Network

- **From the Microsoft Store:** busy makes no network requests. The Store updates it, so there is no update check.
- **Portable `busy.exe`:** busy makes no network requests unless you turn on Settings → Advanced → Check for updates (off by default). Then it asks `api.github.com` once a day whether a newer release exists. GitHub sees your IP address, as with any request to it; the request names busy and its version and carries nothing about you or your computer. Nothing is downloaded or installed. Turn the setting off and the requests stop.

The "Open Task Manager" link starts Windows' Task Manager. In the portable busy, the Releases button in Settings → General → About opens the releases page on GitHub in your browser.

## Settings

Your settings (which readings to show, colors, intervals, opt-ins) are stored in one file on your computer:

- From the Microsoft Store: `%LOCALAPPDATA%\Packages\<package family name>\LocalState\config.json`. Uninstalling busy removes it.
- Portable: `%APPDATA%\busy\config.json`. Delete it to remove your settings.

The file is never sent anywhere. Settings → General → Start with Windows uses the package's startup task from the Microsoft Store, and an entry under `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` for the portable busy.

## Contact

Questions or concerns: open an issue at https://github.com/aivanyuk/busy/issues.
