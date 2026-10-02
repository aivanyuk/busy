# Microsoft Store listing

What to enter in Partner Center for busy's first Store submission (store plan S5, `docs/plans/store.md`). Text in the fenced blocks is pasted as it is; keep it in step with README → Features when features change.

Limits, from Microsoft Learn's MSIX submission pages (checked 2026-10-01): description ≤ 10,000 characters of plain text, no URLs or HTML; short description ≤ 1,000 characters, of which some views show the first 270; product features ≤ 20, each ≤ 200 characters, without bullets of our own; screenshots PNG, desktop 1366 × 768 or larger (up to 3840 × 2160), ≤ 50 MB, 1 to 10, each with an optional caption ≤ 200 characters. Partner Center enforces them, so a rejected paste means they changed.

## Product name

Reserved under Product management → Product identity:

```
busy — system monitor
```

The package's `DisplayName` (store plan S2) is the same string, so the listing and the installed app match.

## Store listing (English)

### Description

```
busy shows live CPU, memory, GPU, network, disk, battery and temperature readings in the Windows 11 taskbar, next to the notification area or at the taskbar's left edge. Click a reading for a detailed flyout.

Readings in the taskbar
Each reading is drawn as text, a graph, a bar, or (network and disk) read and write rates, in its own color or colored by load, with or without its label, and updated at its own interval. Hover one for its value. Readings that don't fit are left out rather than covering your task buttons. The taskbar can be horizontal or vertical.

A flyout per reading
Click a reading for a chart of the last minutes (1 to 10, set in Settings) and the details:
CPU: System, User and Idle time, every logical processor, speed, temperature, processes, threads, handles, up time.
Memory: In use, Modified, Standby and Free, committed, compressed, paged and non-paged pool.
GPU: the busiest engines, dedicated and shared memory, temperatures, power, fan, clocks, driver and DirectX version.
Disk: volumes and free space, response time, temperature, bytes read and written.
Network: interface, Wi-Fi band and signal, addresses, totals sent and received.
Battery: time left, power draw, health, cycle count.
Sensors: every temperature, plus fans, power and voltages when a sensor tool provides them.
CPU, Memory, GPU and Disk list the busiest processes. Every flyout links to Task Manager and to that reading's settings.

Settings
Settings in the Windows 11 style, with search, keyboard navigation, screen-reader support, a live preview of each reading, and light and dark themes that follow Windows or are set by hand. Changes apply at once. Right-click the readings for Settings, Show on taskbar, Position and Exit. A short setup on first start lets you pick the readings and a side of the taskbar.

Temperatures
GPU temperatures work out of the box on NVIDIA and AMD graphics and on any WDDM 2.5 or later driver. CPU temperatures, fans and CPU power come from LibreHardwareMonitor or HWiNFO if you run one and turn on Settings, Advanced, Third-party sensor tools (off by default).

Private and light
busy collects nothing and, installed from the Microsoft Store, makes no network requests. It needs no administrator rights and no driver, and it pauses while the screen is locked or off. Native Win32 and Direct2D, a single small executable.

busy is open source under the MIT license.

Requires Windows 11 23H2 (build 22631) or later, x64.
```

### Product features

One per line in Partner Center, without bullets:

```
CPU, memory, GPU, network, disk, battery and temperature readings in the Windows 11 taskbar
Each reading as text, a graph, a bar or read/write rates, in its own color or colored by load
A flyout per reading with a chart of the last 1 to 10 minutes and the details
Per-core CPU load, memory lists, GPU engines and VRAM, disk response time, Wi-Fi signal, battery health
The busiest processes for CPU, memory, GPU and disk, with a link to Task Manager
GPU temperatures on NVIDIA, AMD and WDDM 2.5+ drivers; CPU temperatures from LibreHardwareMonitor or HWiNFO, opt-in
Windows 11 style settings with search, keyboard navigation, screen-reader support and live previews
Light and dark themes that follow Windows or are set by hand
Horizontal and vertical taskbars, next to the notification area or at the left edge
No account, no telemetry, no network access, no administrator rights
```

### Short description

```
Live CPU, memory, GPU, network, disk, battery and temperature readings in the Windows 11 taskbar, with a detailed flyout for each on click. No account, no telemetry, no network access.
```

### Screenshots

The images the README shows, in this order, each with its caption:

| File | Size (px) | Caption |
|---|---|---|
| `docs/images/taskbar.png` | 706 × 96 | Readings in the taskbar: CPU as a graph, memory as a bar, GPU as text, network and disk rates |
| `docs/images/flyout-cpu.png` | 720 × 964 | The CPU flyout: utilization chart, System / User / Idle, logical processors, speed, processes |
| `docs/images/flyout-memory.png` | 720 × 912 | The Memory flyout: usage chart, In use / Modified / Standby / Free, committed, compressed, pools |
| `docs/images/settings-dark.png` | 987 × 694 | Settings in the dark theme, with a live preview of each reading |
| `docs/images/settings-light.png` | 987 × 694 | Settings in the light theme: Start with Windows, position, update interval, history, theme |
| `docs/images/setup.png` | 680 × 553 | First-run setup: pick the readings and a side of the taskbar |

**None of them qualifies**: a desktop screenshot must be at least 1366 × 768, and these are crops (the widget, one flyout, one window). For the submission, take full-screen PNGs (1920 × 1080 or larger, up to 3840 × 2160) of the same six scenes on a clean desktop: the taskbar with readings, the CPU and Memory flyouts open, Settings dark and light, and setup; the captions above still fit. Keep what matters in the top two-thirds, as the Store may overlay text on the bottom third: the taskbar sits at the bottom, so show it with a flyout or the Settings window open above it. They are submission assets, uploaded in Partner Center and not committed.

### Store logos

Optional. Without a 1:1 app tile icon (300 × 300) the Store uses the package's logos (store plan S2).

### Search terms

Up to 7, each ≤ 30 characters, ≤ 21 words in all (these are 12):

```
system monitor
taskbar
cpu usage
ram usage
gpu monitor
network speed
temperature
```

### Copyright and trademark info

≤ 200 characters; the copyright line of `LICENSE`, and busy has no registered trademark:

```
Copyright (c) 2026 busy contributors. Open source under the MIT License.
```

### Additional license terms

Paste `LICENSE` (the MIT License) as it is. Left empty, the Store's standard application license terms apply, which grant less than the MIT License does; with it, a copy from the Store comes with the same rights as one from GitHub.

## Properties

| Field | Value |
|---|---|
| Category | Utilities & tools (subcategory: the closest one Partner Center offers, if it asks) |
| Privacy policy URL | https://github.com/aivanyuk/busy/blob/main/PRIVACY.md |
| Website | https://github.com/aivanyuk/busy |
| Support contact info | https://github.com/aivanyuk/busy/issues |
| Product declarations | Leave the accessibility declaration unchecked: busy supports keyboard and UI Automation, but hasn't been tested against the accessibility guidelines that declaration names |
| System requirements | Windows 11 23H2 (build 22631) or later, x64 (README → Install), the package manifest's `TargetDeviceFamily MinVersion` (10.0.22631.0, `app/AppxManifest.xml`) |

## Age ratings (IARC questionnaire)

The first question picks the category; choose the one for an app that is not a game, a social network or a browser (utility / productivity). IARC words its questions differently over time; answer by substance:

| Question | Answer | Why |
|---|---|---|
| Violence, fear, sexual content, nudity, crude humor, profanity, drugs, alcohol, tobacco, gambling | No | It shows system readings only |
| Does the app let users interact or exchange content with each other? | No | No accounts, chat or sharing |
| Does the app share the user's location with other users? | No | It reads no location |
| Does the app share personal information with third parties? | No | It collects nothing (`PRIVACY.md`) |
| Does the app allow digital purchases? | No | Free, no in-app purchases, no ads |
| Does the app give unrestricted access to the internet (a browser or search)? | No | From the Store it makes no network requests; it has no web view |
| Is the app primarily a news or educational product? | No | |

Expected rating: the lowest in every market (3+ / Everyone / PEGI 3). Partner Center shares the publisher display name and email with IARC; IARC sends the certificate by email after publishing.

## Pricing and availability

Free, all markets, no trial, discoverable in the Store.

## Submission options

### Restricted capabilities: `runFullTrust`

The field ("Why do you need the runFullTrust capability, and how will it be used in your product?") takes about 500 characters, so it gets the short form:

```
busy is a system monitor whose widget lives inside the Windows taskbar. It needs runFullTrust because its window is a child of the taskbar (explorer's Shell_TrayWnd), placed from the taskbar's own windows and UI Automation, and because it reads system-wide performance data: PDH counters, NtQuerySystemInformation, network, battery and GPU APIs, and the GPU vendors' libraries (NVML, ADL) from System32. It runs as the user: no admin rights, no driver or service, no network access.
```

The long form, for certification if it asks for more:

```
busy is a full-trust Win32 desktop app (one native executable, no runtime) that shows system performance readings in the Windows taskbar. It needs runFullTrust because:

1. Its widget is a child window of the Windows taskbar. It creates its window with explorer's Shell_TrayWnd as parent, positions it from the taskbar's own windows and, with left-aligned taskbar icons, finds the end of the task buttons through UI Automation on the taskbar. This is the same window and the same process as the portable busy, which works as an ordinary desktop app rather than in an app container.

2. It reads system-wide performance data with Win32 APIs: Performance Data Helper counters (Processor Information, PhysicalDisk, GPU Engine, GPU Adapter Memory), NtQuerySystemInformation for the process list, memory page lists and compressed memory, GlobalMemoryStatusEx and GetPerformanceInfo, GetIfTable2/GetIfEntry2 and the Wi-Fi API (wlanapi) for network rates and signal, the battery device through SetupAPI and the IOCTL_BATTERY queries, and DXGI with D3DKMTQueryAdapterInfo for GPUs.

3. It reads GPU temperatures, fans, power and clocks from the graphics vendors' driver libraries, NVML (nvml.dll) and ADL (atiadlxx.dll), loaded only from System32.

4. When the user turns on Settings > Advanced > Third-party sensor tools (off by default), it reads sensor data published by LibreHardwareMonitor through WMI (root\LibreHardwareMonitor) or by HWiNFO through a global shared-memory section.

It runs as the user, needs no administrator rights, installs no driver or service, and makes no network requests in the Store build. Start with Windows uses the package's startup task (windows.startupTask), disabled until the user turns it on.
```

### Notes for certification

```
busy has no main window: its readings appear inside the taskbar, next to the notification area or at the taskbar's left edge. The first start opens a short setup (pick the readings, pick a side of the taskbar). Click a reading for its flyout; right-click the readings for Settings, Show on taskbar, Position and Exit. No account or sign-in. busy starts at sign-in only after the user turns on Settings > General > Start with Windows. CPU temperatures need LibreHardwareMonitor or HWiNFO running and Settings > Advanced > Third-party sensor tools turned on; without them only GPU temperatures are shown, where the graphics driver provides them. Only one busy runs at a time: a second start exits at once.
```
