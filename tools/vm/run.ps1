<#
.SYNOPSIS
Runs tools/vm/guest.ps1 in each Hyper-V test VM and collects the results (docs/testing.md section Windows versions).

.DESCRIPTION
For each VM: reverts it to its checkpoint (a running, signed-in desktop), copies busy.exe and guest.ps1 in over
PowerShell Direct, starts guest.ps1 in the signed-in user's session through a scheduled task (PowerShell Direct
itself has no desktop), waits for it, copies the results back and turns the VM off. The VM is reverted again on
the next run, so nothing a run changes in it lasts.

Needs an elevated shell on a host with Hyper-V, and a debug build: `cargo build -p busy`.

.EXAMPLE
tools\vm\run.ps1 -VMName busy-22631, busy-26100, busy-26200 -Credential (Get-Credential tester)
#>
param(
    [Parameter(Mandatory = $true)] [string[]] $VMName,
    # A local administrator in the VMs; also the account that is signed in at the checkpoint.
    [Parameter(Mandatory = $true)] [pscredential] $Credential,
    [string] $Checkpoint = 'busy-ready',
    [string] $Exe = (Join-Path $PSScriptRoot '..\..\target\debug\busy.exe'),
    [string] $Out = (Join-Path $PSScriptRoot "..\..\target\vm\$(Get-Date -Format yyyyMMdd-HHmmss)"),
    [string[]] $Variants,
    [int] $TimeoutMinutes = 20
)

$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$guestDir = 'C:\busy-test'

function Wait-Guest($vm) {
    $deadline = (Get-Date).AddMinutes(5)
    while ((Get-Date) -lt $deadline) {
        try {
            # The checkpoint is a signed-in desktop: wait for its shell, not only for the VM to answer.
            $up = Invoke-Command -VMName $vm -Credential $Credential -ErrorAction Stop {
                [bool](Get-Process explorer -ErrorAction SilentlyContinue)
            }
            if ($up) { return }
        } catch { }
        Start-Sleep -Seconds 5
    }
    throw "$vm did not come up with a signed-in desktop"
}

foreach ($vm in $VMName) {
    Write-Host "== $vm" -ForegroundColor Cyan
    $dest = Join-Path $Out $vm
    New-Item -ItemType Directory -Force $dest | Out-Null
    $session = $null
    try {
        Stop-VM -Name $vm -TurnOff -Force -ErrorAction SilentlyContinue
        Restore-VMCheckpoint -VMName $vm -Name $Checkpoint -Confirm:$false
        Start-VM -Name $vm
        Wait-Guest $vm
        $session = New-PSSession -VMName $vm -Credential $Credential
        Invoke-Command -Session $session { param($d) Remove-Item $d -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory $d | Out-Null } -ArgumentList $guestDir
        Copy-Item $Exe, (Join-Path $PSScriptRoot 'guest.ps1') -Destination $guestDir -ToSession $session

        Invoke-Command -Session $session {
            param($d, $variants)
            # The interactive user, whose desktop the checkpoint holds.
            $user = (Get-CimInstance Win32_ComputerSystem).UserName
            if (-not $user) { throw 'nobody is signed in at the console' }
            $arg = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File $d\guest.ps1 -Exe $d\busy.exe -Out $d\out -Vm"
            if ($variants) { $arg += ' -Variants ' + ($variants -join ',') }
            $action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arg
            $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive
            Register-ScheduledTask -TaskName 'busy-selftest' -Action $action -Principal $principal -Force | Out-Null
            Start-ScheduledTask -TaskName 'busy-selftest'
        } -ArgumentList $guestDir, $Variants

        $deadline = (Get-Date).AddMinutes($TimeoutMinutes)
        do {
            Start-Sleep -Seconds 10
            $done = Invoke-Command -Session $session { param($d) Test-Path "$d\out\done.txt" } -ArgumentList $guestDir
        } while (-not $done -and (Get-Date) -lt $deadline)
        if (-not $done) { Write-Warning "$vm`: guest.ps1 did not finish in $TimeoutMinutes minutes" }
        Copy-Item "$guestDir\out\*" -Destination $dest -Recurse -FromSession $session -ErrorAction SilentlyContinue
    } catch {
        Write-Warning "$vm`: $($_.Exception.Message)"
    } finally {
        if ($session) { Remove-PSSession $session }
        Stop-VM -Name $vm -TurnOff -Force -ErrorAction SilentlyContinue
    }
}

# One line per VM and variant; the failures and warnings underneath.
$rows = foreach ($vm in $VMName) {
    $f = Join-Path $Out "$vm\summary.json"
    if (-not (Test-Path $f)) { [pscustomobject]@{ vm = $vm; build = '?'; variant = '-'; pass = 0; warn = 0; fail = 1; notes = 'no summary.json' }; continue }
    $s = Get-Content -Raw $f | ConvertFrom-Json
    foreach ($g in @($s.checks) | Group-Object variant) {
        $bad = @($g.Group | Where-Object { $_.status -ne 'pass' })
        [pscustomobject]@{
            vm = $vm; build = $s.build; variant = $g.Name
            pass = @($g.Group | Where-Object { $_.status -eq 'pass' }).Count
            warn = @($bad | Where-Object { $_.status -eq 'warn' }).Count
            fail = @($bad | Where-Object { $_.status -eq 'fail' }).Count
            notes = ($bad | ForEach-Object { "$($_.phase)/$($_.check): $($_.detail)" }) -join '; '
        }
    }
}
$rows | Format-Table vm, build, variant, pass, warn, fail -AutoSize
$rows | Where-Object { $_.notes } | ForEach-Object { Write-Host "$($_.vm) $($_.variant): $($_.notes)" }
$rows | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $Out 'matrix.json')
Write-Host "Results: $Out"
exit [int](@($rows | Where-Object { $_.fail -gt 0 }).Count -gt 0)
