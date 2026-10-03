<#
.SYNOPSIS
Builds busy.exe (release) and packs it as the Microsoft Store package: target\msix\busy-X.Y.Z-x64.msix.

.DESCRIPTION
The build script writes the package layout (AppxManifest.xml with the version, the logos) to its OUT_DIR\msix
(app/build/msix.rs); this finds it through cargo's JSON messages, adds busy.exe, LICENSE and
THIRD-PARTY-LICENSES.txt, indexes the logos' scale and target-size variants and the manifest's translated strings
into resources.pri (makepri) and packs it (makeappx). The package is unsigned: the Store signs it (docs/plans/store.md).

-Layout stops after the layout, in target\msix\layout, for registering it as it is with Developer Mode on
(docs/testing.md): Add-AppxPackage -Register target\msix\layout\AppxManifest.xml.

Needs the Windows SDK's makepri.exe and makeappx.exe: an installed SDK (Windows Kits\10\bin\<version>\x64, as on
GitHub's runners), or without installing anything the Microsoft.Windows.SDK.BuildTools NuGet package unpacked
into target\sdk-buildtools\<version> (docs/testing.md). Cargo's environment
(RUSTFLAGS and the like) is the caller's, so a release build made just before is reused as it is.
#>
param(
    [string] $Out,
    [switch] $Layout
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$work = Join-Path $root 'target\msix'

$build = cargo build -p busy --release --locked --message-format=json-render-diagnostics
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
$messages = $build | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object { $_.package_id -match '#busy@' }
$outDir = ($messages | Where-Object reason -eq 'build-script-executed' | Select-Object -Last 1).out_dir
$exe = ($messages | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.executable } | Select-Object -Last 1).executable
$version = ($messages | Select-Object -Last 1).package_id -replace '.*#busy@', ''
if (-not $outDir -or -not (Test-Path (Join-Path $outDir 'msix\AppxManifest.xml'))) { throw 'no package layout in the build output' }
if (-not $exe) { throw 'no busy.exe in the build output' }

$layoutDir = Join-Path $work 'layout'
if (Test-Path $layoutDir) { Remove-Item $layoutDir -Recurse -Force }
New-Item -ItemType Directory -Force $layoutDir | Out-Null
Copy-Item (Join-Path $outDir 'msix\*') $layoutDir -Recurse
Copy-Item $exe, (Join-Path $root 'LICENSE'), (Join-Path $root 'THIRD-PARTY-LICENSES.txt') $layoutDir

# The newest SDK that has both tools.
$kits = "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64", (Join-Path $root 'target\sdk-buildtools\*\bin\10.*\x64')
$sdk = Get-ChildItem $kits -Directory -ErrorAction SilentlyContinue |
    Where-Object { (Test-Path "$_\makepri.exe") -and (Test-Path "$_\makeappx.exe") } |
    Sort-Object { [version](Split-Path (Split-Path $_ -Parent) -Leaf) } | Select-Object -Last 1
if (-not $sdk) { throw 'makepri.exe and makeappx.exe not found: install the Windows SDK or unpack its BuildTools (docs/testing.md)' }

# Without packaging rules, so makepri writes one resources.pri rather than splitting it per scale.
$config = Join-Path $work 'priconfig.xml'
& "$sdk\makepri.exe" createconfig /cf $config /dq en-US /pv 10.0.0 /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'makepri createconfig failed' }
$xml = [xml](Get-Content $config -Raw)
$xml.resources.SelectNodes('packaging') | ForEach-Object { [void]$xml.resources.RemoveChild($_) }
$xml.Save($config)
& "$sdk\makepri.exe" new /pr $layoutDir /cf $config /mn (Join-Path $layoutDir 'AppxManifest.xml') /of (Join-Path $layoutDir 'resources.pri') /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'makepri new failed' }
# The strings are in resources.pri now; the .resw sources aren't needed at run time.
Remove-Item (Join-Path $layoutDir 'Strings') -Recurse -Force
if ($Layout) {
    Write-Output $layoutDir
    return
}

if (-not $Out) { $Out = Join-Path $work "busy-$version-x64.msix" }
$Out = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Out)
New-Item -ItemType Directory -Force (Split-Path $Out -Parent) | Out-Null
& "$sdk\makeappx.exe" pack /d $layoutDir /p $Out /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'makeappx pack failed' }
Write-Output $Out
