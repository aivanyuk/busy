<#
.SYNOPSIS
Runs a debug busy.exe with BUSY_SELFTEST and checks its widget and flyout against explorer's taskbar.

.DESCRIPTION
Must run in the interactive session of the signed-in user (tools/vm/run.ps1 starts it through a scheduled task).
Each variant starts busy with its own config, reads the selftest report, compares the widget with the buttons
explorer exposes through UI Automation, opens every cell's flyout by posting clicks to busy's own widget,
takes screenshots, and exits busy by posting WM_CLOSE to its main window.

With -Vm, inside a Hyper-V VM only, a variant may also switch the taskbar alignment and the system theme, and
busy is checked across an explorer restart. Without -Vm nothing outside -Out is changed: variants whose
alignment differs from the current one are skipped and explorer is left alone (docs/testing.md).

Windows PowerShell 5.1 compatible: that is what a fresh VM has.
#>
param(
    [Parameter(Mandatory = $true)] [string] $Exe,
    [Parameter(Mandatory = $true)] [string] $Out,
    # anchor-theme-alignment-cells: anchor tray|left, theme dark|light, alignment center|left, cells default|all.
    [string[]] $Variants = @('tray-dark-center-default', 'left-light-center-default', 'tray-light-left-default',
        'left-dark-left-default', 'tray-dark-center-all'),
    [switch] $Vm
)

$ErrorActionPreference = 'Stop'
# run.ps1 passes the list as one comma-separated argument.
$Variants = @($Variants | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path

Add-Type -AssemblyName System.Drawing, UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class BusyNative {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr c);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindow(string cls, string name);
    // PowerShell passes $null to a string parameter as "", which FindWindow takes for an empty title.
    public static IntPtr FindClass(string cls) { return FindWindow(cls, null); }
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
}
'@
# Per-monitor v2: screenshots, UIA rects and posted click coordinates in physical pixels, like busy's.
$pmv2 = [IntPtr](-4)
if (-not [BusyNative]::SetProcessDpiAwarenessContext($pmv2)) { [BusyNative]::SetThreadDpiAwarenessContext($pmv2) | Out-Null }

$advanced = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced'
$personalize = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'

function Test-InVm {
    $cs = Get-CimInstance Win32_ComputerSystem
    $cs.Manufacturer -eq 'Microsoft Corporation' -and $cs.Model -eq 'Virtual Machine'
}

if ($Vm -and -not (Test-InVm)) { throw '-Vm restarts explorer and changes taskbar settings: only inside a Hyper-V VM.' }
if (Get-Process busy -ErrorAction SilentlyContinue) {
    if (-not $Vm) { throw 'busy is already running; exit it first (its single-instance mutex would stop ours).' }
    Get-Process busy | Stop-Process -Force
}

$checks = New-Object System.Collections.Generic.List[object]
function Add-Check($variant, $phase, $name, $status, $detail) {
    $checks.Add([pscustomobject]@{ variant = $variant; phase = $phase; check = $name; status = $status; detail = "$detail" })
    $color = @{ pass = 'Green'; warn = 'Yellow'; fail = 'Red' }[$status]
    Write-Host ("{0,-28} {1,-8} {2,-22} {3,-4} {4}" -f $variant, $phase, $name, $status, $detail) -ForegroundColor $color
}
function Pass-If($variant, $phase, $name, $ok, $detail) {
    Add-Check $variant $phase $name $(if ($ok) { 'pass' } else { 'fail' }) $detail
}

function Get-Tray { [BusyNative]::FindClass('Shell_TrayWnd') }

function Read-Report($dir) {
    try { Get-Content -Raw (Join-Path $dir 'selftest.json') | ConvertFrom-Json } catch { $null }
}

# Polls the report until $test accepts it; the last one read (or $null) on timeout.
function Wait-Report($dir, [scriptblock] $test, $seconds = 15) {
    $deadline = (Get-Date).AddSeconds($seconds)
    $r = $null
    do {
        $r = Read-Report $dir
        if ($r -and (& $test $r)) { return $r }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)
    $r
}

# The report's seq only grows; waiting for a later one means busy has seen what happened since.
function Wait-Fresh($dir, $after, [scriptblock] $fresh, $seconds = 15) {
    $seq = if ($after) { $after.seq } else { 0 }
    # Bound now: inside Wait-Report, `$fresh` and `$seq` would otherwise resolve dynamically.
    $accept = { param($r) $r.seq -gt $seq -and (& $fresh $r) }.GetNewClosure()
    Wait-Report $dir $accept $seconds
}

# Waits until the widget's rect held for three ticks: its first layouts may still use ReBarWindow32 for the end
# of the task buttons, until busy's UI Automation scan answers (docs/areas/app.md).
function Wait-Settled($dir, $r, $seconds = 15) {
    $deadline = (Get-Date).AddSeconds($seconds)
    while ($r -and (Get-Date) -lt $deadline) {
        $before = $r
        $r = Wait-Fresh $dir $before { param($x) $x.seq -ge $before.seq + 3 }.GetNewClosure() 10
        if ($r -and ($r.widget.rect -join ',') -eq ($before.widget.rect -join ',')) { return $r }
    }
    $r
}

function Wait-NewTray($old, $seconds = 60) {
    $deadline = (Get-Date).AddSeconds($seconds)
    do {
        $t = Get-Tray
        if ($t -ne [IntPtr]::Zero -and $t -ne $old) { return $t }
        Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline)
    [IntPtr]::Zero
}

function Restart-Explorer {
    $old = Get-Tray
    Get-Process explorer -ErrorAction SilentlyContinue | Stop-Process -Force
    # Winlogon restarts the shell (AutoRestartShell); start it ourselves if it doesn't.
    Start-Sleep -Seconds 3
    if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
    $new = Wait-NewTray $old
    # The XAML taskbar fills in after the window exists.
    Start-Sleep -Seconds 5
    $new
}

function Get-ExplorerButtons($tray) {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($tray)
    $isButton = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $explorer = (Get-Process explorer | Select-Object -ExpandProperty Id)
    foreach ($e in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $isButton)) {
        $c = $e.Current
        $r = $c.BoundingRectangle
        if ($c.IsOffscreen -or $r.IsEmpty -or $r.Width -le 0 -or $explorer -notcontains $c.ProcessId) { continue }
        [pscustomobject]@{ name = $c.Name; id = $c.AutomationId; left = [int]$r.Left; top = [int]$r.Top
            right = [int]$r.Right; bottom = [int]$r.Bottom }
    }
}

function Save-Screen($path, $l, $t, $r, $b) {
    $w = $r - $l; $h = $b - $t
    if ($w -le 0 -or $h -le 0) { return }
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try {
        $g.CopyFromScreen($l, $t, 0, 0, (New-Object System.Drawing.Size $w, $h))
        $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $g.Dispose(); $bmp.Dispose() }
}

# Posted like a real click: WM_LBUTTONUP at the cell's center, in widget client pixels. Cell extents run along
# the taskbar: x, or y on a vertical one.
function Send-CellClick($r, $cell) {
    $widget = $r.widget
    $along = [int](($cell.left + $cell.right) / 2)
    if ($r.taskbar.vertical) { $x = [int](($widget.rect[2] - $widget.rect[0]) / 2); $y = $along }
    else { $x = $along; $y = [int](($widget.rect[3] - $widget.rect[1]) / 2) }
    [BusyNative]::PostMessage([IntPtr][long]$widget.hwnd, 0x0202, [IntPtr]::Zero, [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))) | Out-Null
}

function Overlaps($a, $b) { $a.left -lt $b.right -and $b.left -lt $a.right -and $a.top -lt $b.bottom -and $b.top -lt $a.bottom }
# (start, end) of rect `r` along the taskbar.
function Get-Span($r, $vertical) { if ($vertical) { $r.top; $r.bottom } else { $r.left; $r.right } }
function To-Rect($arr) { [pscustomobject]@{ left = $arr[0]; top = $arr[1]; right = $arr[2]; bottom = $arr[3] } }

# The widget against the taskbar it is in: embedded, visible, on top, inside, on its side, covering nothing.
function Test-Widget($variant, $phase, $r, $anchor, $align) {
    $w = $r.widget
    $tr = To-Rect $r.taskbar.rect
    $vertical = [bool]$r.taskbar.vertical
    Add-Check $variant $phase 'orientation' 'pass' "$(if ($vertical) { 'vertical' } else { 'horizontal' }) taskbar $($r.taskbar.rect)"
    # Explorer without its XAML taskbar (no Start button, tray or task buttons): seen on a runner whose session was
    # not on the console. busy then rightly has no room; the cause is the machine, not busy.
    $populated = @(Get-ExplorerButtons ([IntPtr][long]$r.taskbar.hwnd)).Count
    Pass-If $variant $phase 'taskbar-populated' ($populated -gt 0) "$populated explorer buttons, XAML island $([bool]$r.taskbar.landmarks.DesktopWindowContentBridge)"
    if ($populated -eq 0 -and $r.flyout) {
        # The whole monitor (work area plus taskbar), to see what is showing instead.
        $wa = To-Rect $r.flyout.work
        Save-Screen (Join-Path $Out "$variant\screen-$phase.png") ([math]::Min($wa.left, $tr.left)) ([math]::Min($wa.top, $tr.top)) ([math]::Max($wa.right, $tr.right)) ([math]::Max($wa.bottom, $tr.bottom))
    }
    Pass-If $variant $phase 'embedded' ($w -and $w.parent -eq $r.taskbar.hwnd -and $w.visible) "parent $($w.parent), taskbar $($r.taskbar.hwnd), visible $($w.visible), room $($w.slot[1]) px"
    if (-not ($w -and $w.visible)) { return }
    Pass-If $variant $phase 'z-order' (-not $w.covered) 'no sibling above the widget'
    $wr = To-Rect $w.rect
    Pass-If $variant $phase 'inside-taskbar' ($wr.left -ge $tr.left -and $wr.right -le $tr.right -and $wr.top -ge $tr.top -and $wr.bottom -le $tr.bottom) "widget $($w.rect) taskbar $($r.taskbar.rect)"
    $drawn = @($w.cells).Count
    $status = if ($drawn -eq $r.configured) { 'pass' } elseif ($drawn -gt 0) { 'warn' } else { 'fail' }
    Add-Check $variant $phase 'cells' $status "$drawn of $($r.configured) drawn, room $($w.slot[1]) px"
    $buttons = @(Get-ExplorerButtons ([IntPtr][long]$r.taskbar.hwnd))
    # Where explorer::slot anchors, along the taskbar (y when vertical): NearTray ends at the notification area,
    # Left starts at the taskbar's start with centered icons, else 8 DIPs after the last task button before it.
    $ws, $we = Get-Span $wr $vertical
    $gap = 8 * $r.taskbar.dpi / 96
    if ($anchor -eq 'NearTray') {
        $tn = $r.taskbar.landmarks.TrayNotifyWnd
        $want = if ($tn) { (Get-Span (To-Rect $tn) $vertical)[0] } else { (Get-Span $tr $vertical)[1] }
        Pass-If $variant $phase 'anchored' ([math]::Abs($we - $want) -le 2) "NearTray: widget ends at $we, notification area starts at $want"
    } elseif ($align -eq 'center') {
        $want = (Get-Span $tr $vertical)[0]
        Pass-If $variant $phase 'anchored' ([math]::Abs($ws - $want) -le 2) "Left, centered icons: widget starts at $ws, taskbar at $want"
    } else {
        $ends = @($buttons | ForEach-Object { (Get-Span $_ $vertical)[1] } | Where-Object { $_ -le $ws + 2 })
        $last = ($ends | Measure-Object -Maximum).Maximum
        Pass-If $variant $phase 'anchored' ($ends.Count -gt 0 -and [math]::Abs($ws - $last - $gap) -le 2) "Left, left-aligned icons: widget starts at $ws, last task button ends at $last, want $gap after it"
    }
    $hit = @($buttons | Where-Object { Overlaps $wr $_ })
    Pass-If $variant $phase 'no-overlap' ($buttons.Count -gt 0 -and $hit.Count -eq 0) $(if ($hit) { 'covers ' + (($hit | ForEach-Object { "'$($_.name)'" }) -join ', ') } else { "$($buttons.Count) explorer buttons clear" })
    # The legacy windows `explorer::slot` reads must still match what is on screen.
    $start = $buttons | Where-Object { $_.id -eq 'StartButton' } | Select-Object -First 1
    $legacy = $r.taskbar.landmarks.Start
    if ($start -and $legacy) {
        $d = [math]::Max([math]::Max([math]::Abs($legacy[0] - $start.left), [math]::Abs($legacy[2] - $start.right)),
            [math]::Max([math]::Abs($legacy[1] - $start.top), [math]::Abs($legacy[3] - $start.bottom)))
        Add-Check $variant $phase 'start-landmark' $(if ($d -le 2) { 'pass' } else { 'warn' }) "Start window $($legacy -join ',') vs button $($start.left),$($start.top),$($start.right),$($start.bottom)"
    } else {
        Add-Check $variant $phase 'start-landmark' 'warn' "Start window $([bool]$legacy), UIA StartButton $([bool]$start)"
    }
    Add-Check $variant $phase 'tray-landmark' $(if ($r.taskbar.landmarks.TrayNotifyWnd) { 'pass' } else { 'warn' }) "TrayNotifyWnd $($r.taskbar.landmarks.TrayNotifyWnd -join ',')"
    $buttons | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $Out "$variant\buttons-$phase.json")
}

function Write-Config($dir, $anchor, $theme, $cells) {
    $modules = foreach ($m in 'Cpu', 'Memory', 'Gpu', 'Network', 'Disk', 'Sensors', 'Battery') {
        @{ module = $m; taskbar = ($cells -eq 'all' -or $m -in 'Cpu', 'Memory', 'Gpu', 'Network') }
    }
    $cfg = @{ onboarded = $true; anchor = $anchor; theme = $theme; modules = @($modules) + @(@{ module = 'Processes' }) }
    New-Item -ItemType Directory -Force (Join-Path $dir 'busy') | Out-Null
    $cfg | ConvertTo-Json -Depth 4 | Set-Content -Encoding ASCII (Join-Path $dir 'busy\config.json')
}

function Set-RegDword($path, $name, $value) {
    $cur = (Get-ItemProperty $path -Name $name -ErrorAction SilentlyContinue).$name
    if ($cur -ne $value) { Set-ItemProperty $path -Name $name -Value $value -Type DWord; return $true }
    $false
}

function Invoke-Variant($variant) {
    $anchorKey, $themeKey, $align, $cells = $variant -split '-'
    $anchor = @{ tray = 'NearTray'; left = 'Left' }[$anchorKey]
    $dir = Join-Path $Out $variant
    New-Item -ItemType Directory -Force $dir | Out-Null
    $centered = (Get-ItemProperty $advanced -Name TaskbarAl -ErrorAction SilentlyContinue).TaskbarAl -ne 0
    if ($Vm) {
        $light = [int]($themeKey -eq 'light')
        $changed = Set-RegDword $advanced 'TaskbarAl' ([int]($align -eq 'center'))
        $changed = (Set-RegDword $personalize 'SystemUsesLightTheme' $light) -or $changed
        $changed = (Set-RegDword $personalize 'AppsUseLightTheme' $light) -or $changed
        if ($changed) { Restart-Explorer | Out-Null }
        $theme = 'System'
    } elseif ($centered -ne ($align -eq 'center')) {
        Add-Check $variant 'setup' 'skipped' 'warn' "taskbar alignment is not $align (only -Vm changes it)"
        return
    } else {
        $theme = @{ dark = 'Dark'; light = 'Light' }[$themeKey]
    }

    $appdata = Join-Path $dir 'appdata'
    Write-Config $appdata $anchor $theme $cells
    Remove-Item (Join-Path $dir 'selftest.json') -ErrorAction SilentlyContinue
    $env:APPDATA = $appdata; $env:BUSY_SELFTEST = $dir; $env:BUSY_DUMP = $dir; $env:BUSY_FAKE = '1'; $env:BUSY_PIN_FLYOUT = '1'
    $proc = Start-Process $Exe -PassThru
    $tray = Get-Tray
    try {
        # Drawn once the first sample is in; a widget with no room stays hidden, which a few ticks settle.
        $r = Wait-Report $dir { param($r) ($r.widget.visible -and @($r.widget.cells).Count -gt 0) -or $r.seq -ge 8 } 30
        if (-not $r) { Add-Check $variant 'start' 'report' 'fail' 'no selftest.json (is it a debug build?)'; return }
        $r = Wait-Settled $dir $r
        $r | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $dir 'report-start.json')
        Test-Widget $variant 'start' $r $anchor $align
        $tb = $r.taskbar.rect
        Save-Screen (Join-Path $dir 'taskbar.png') $tb[0] $tb[1] $tb[2] $tb[3]

        $margin = 12 * $r.taskbar.dpi / 96
        foreach ($cell in @($r.widget.cells)) {
            Send-CellClick $r $cell
            $f = Wait-Fresh $dir $r { param($x) $x.flyout.visible -and $x.flyout.module -eq $cell.module } 5
            $ok = $f -and $f.flyout.visible -and $f.flyout.module -eq $cell.module
            Pass-If $variant 'flyout' "open-$($cell.module)" $ok "visible $($f.flyout.visible), module $($f.flyout.module)"
            if ($ok) {
                $fr = To-Rect $f.flyout.rect; $wa = To-Rect $f.flyout.work
                Pass-If $variant 'flyout' "inside-$($cell.module)" ($fr.left -ge $wa.left -and $fr.right -le $wa.right -and $fr.top -ge $wa.top -and $fr.bottom -le $wa.bottom) "flyout $($f.flyout.rect) work area $($f.flyout.work)"
                # Beside the taskbar, in the corner at the widget's end of it (flyout/mod.rs place).
                $tb = To-Rect $f.taskbar.rect
                $nearTray = $anchor -eq 'NearTray'
                if ($f.taskbar.vertical) { $right = $tb.left -ge $wa.right; $bottom = $nearTray }
                else { $right = $nearTray; $bottom = $tb.top -ge $wa.bottom }
                $gx = if ($right) { $wa.right - $fr.right } else { $fr.left - $wa.left }
                $gy = if ($bottom) { $wa.bottom - $fr.bottom } else { $fr.top - $wa.top }
                $corner = "$(if ($bottom) { 'bottom' } else { 'top' })-$(if ($right) { 'right' } else { 'left' })"
                Pass-If $variant 'flyout' "placed-$($cell.module)" ([math]::Abs($gx - $margin) -le 2 -and [math]::Abs($gy - $margin) -le 2) "$gx, $gy px from the $corner corner, want $margin"
                Pass-If $variant 'flyout' "backdrop-$($cell.module)" $f.flyout.backdrop 'DWMSBT_TRANSIENTWINDOW applied'
                Start-Sleep -Milliseconds 600
                Save-Screen (Join-Path $dir "flyout-$($cell.module).png") $fr.left $fr.top $fr.right $fr.bottom
            }
            # A click on the open module's cell closes it.
            Send-CellClick $r $cell
            $r = Wait-Fresh $dir $f { param($x) -not $x.flyout.visible } 5
            Pass-If $variant 'flyout' "close-$($cell.module)" ($r -and -not $r.flyout.visible) "visible $($r.flyout.visible)"
        }
        if ((Get-Tray) -ne $tray) { Add-Check $variant 'flyout' 'explorer-alive' 'fail' 'the taskbar was re-created while busy ran' }

        if ($Vm) {
            $tray = Restart-Explorer
            Pass-If $variant 'restart' 'new-taskbar' ($tray -ne [IntPtr]::Zero) "Shell_TrayWnd $tray"
            $t = [long]$tray
            $r = Wait-Fresh $dir $r { param($x) $x.taskbar.hwnd -eq $t -and $x.widget.parent -eq $t -and $x.widget.visible } 20
            if ($r) {
                $r = Wait-Settled $dir $r
                $r | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $dir 'report-restart.json')
                Test-Widget $variant 'restart' $r $anchor $align
                $tb = $r.taskbar.rect
                Save-Screen (Join-Path $dir 'taskbar-restart.png') $tb[0] $tb[1] $tb[2] $tb[3]
            }
        }

        $main = [BusyNative]::FindClass('busy.main')
        [BusyNative]::PostMessage($main, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        Pass-If $variant 'exit' 'clean-exit' ($proc.WaitForExit(10000)) 'WM_CLOSE to busy.main'
        Pass-If $variant 'exit' 'explorer-alive' ((Get-Tray) -eq $tray) "Shell_TrayWnd $(Get-Tray), was $tray"
    } finally {
        if (-not $proc.HasExited) { $proc | Stop-Process -Force }
    }
}

$os = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$info = [ordered]@{
    build = "$($os.CurrentBuild).$($os.UBR)"; version = $os.DisplayVersion; edition = $os.EditionID
    arch = $env:PROCESSOR_ARCHITECTURE; vm = [bool]$Vm; exe = $Exe; started = (Get-Date).ToString('s')
}
Write-Host "Windows $($info.build) ($($info.version), $($info.edition), $($info.arch))"
try {
    foreach ($v in $Variants) {
        try { Invoke-Variant $v } catch { Add-Check $v 'error' 'exception' 'fail' $_.Exception.Message }
    }
} finally {
    $info.checks = $checks
    $info | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $Out 'summary.json')
    Set-Content (Join-Path $Out 'done.txt') (Get-Date).ToString('s')
}
$failed = @($checks | Where-Object { $_.status -eq 'fail' }).Count
Write-Host "$($checks.Count) checks, $failed failed"
exit [int]($failed -gt 0)
