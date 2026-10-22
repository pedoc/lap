param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')]
    [string]$Target,
    [string]$VcRedistDir = $env:VCToolsRedistDir,
    [ValidatePattern('^(?:[A-Za-z0-9][A-Za-z0-9.-]*)?$')]
    [string]$Suffix = ''
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$projectDir = Join-Path $repoRoot 'src-tauri'
$releaseDir = Join-Path $projectDir "target/$Target/release"
$arch = if ($Target -eq 'x86_64-pc-windows-msvc') { 'x64' } else { 'arm64' }
$expectedMachine = if ($arch -eq 'x64') { 0x8664 } else { 0xAA64 }
$config = Get-Content -LiteralPath (Join-Path $projectDir 'tauri.conf.json') -Raw | ConvertFrom-Json -AsHashtable
$windowsConfig = Get-Content -LiteralPath (Join-Path $projectDir 'tauri.windows.conf.json') -Raw | ConvertFrom-Json -AsHashtable
$version = $config.version
if ($version -notmatch '^[0-9A-Za-z.+-]+$') { throw 'Invalid application version for the portable filename.' }
if (-not $VcRedistDir) { throw 'VCToolsRedistDir is missing. Run this script in the MSVC build environment.' }

function Get-PeMachine([string]$Path) {
    $reader = [System.IO.BinaryReader]::new([System.IO.File]::OpenRead($Path))
    try {
        if ($reader.BaseStream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5A4D) { throw "Invalid Windows binary: $Path" }
        $reader.BaseStream.Position = 0x3C
        $header = $reader.ReadInt32()
        if ($header -lt 64 -or $header -gt $reader.BaseStream.Length - 6) { throw "Invalid PE header: $Path" }
        $reader.BaseStream.Position = $header
        if ($reader.ReadUInt32() -ne 0x4550) { throw "Invalid PE signature: $Path" }
        return $reader.ReadUInt16()
    } finally { $reader.Dispose() }
}

function Assert-PeArchitecture([string]$Path) {
    $machine = Get-PeMachine $Path
    if ($machine -ne $expectedMachine) {
        throw ('Windows binary does not match {0} (PE 0x{1:X4}, expected 0x{2:X4}): {3}' -f $arch, $machine, $expectedMachine, $Path)
    }
}

# The build directory also contains intermediate files; only ship the executable and runtime DLLs.
$executable = Join-Path $releaseDir 'Lap.exe'
Assert-PeArchitecture $executable
# ONNX Runtime is statically linked in the current build; DirectML remains an external DLL.
Assert-PeArchitecture (Join-Path $releaseDir 'DirectML.dll')
$runtimeDlls = @(Get-ChildItem -LiteralPath $releaseDir -Filter '*.dll' -File)
$crtDirs = @(Get-ChildItem -LiteralPath (Join-Path $VcRedistDir $arch) -Filter 'Microsoft.VC*.CRT' -Directory)
if ($crtDirs.Count -ne 1) { throw "Expected one redistributable CRT directory for $arch under $VcRedistDir." }
foreach ($required in @('msvcp140.dll', 'vcruntime140.dll')) {
    Assert-PeArchitecture (Join-Path $crtDirs[0].FullName $required)
}
foreach ($dll in $runtimeDlls) { Assert-PeArchitecture $dll.FullName }
$crtDlls = @(
    foreach ($dll in Get-ChildItem -LiteralPath $crtDirs[0].FullName -Filter '*.dll' -File) {
        $machine = Get-PeMachine $dll.FullName
        if ($machine -eq $expectedMachine) {
            $dll
        } elseif ($arch -eq 'arm64' -and $machine -eq 0x8664) {
            # Microsoft's ARM64 CRT directory also contains x64-only helpers, such as vcruntime140_1.dll.
            Write-Host "Skipping x64-only VC runtime companion in ARM64 package: $($dll.Name)"
        } else {
            Assert-PeArchitecture $dll.FullName
        }
    }
)
$resources = @{} + $config.bundle.resources + $windowsConfig.bundle.resources

$name = "Lap_${version}_${arch}_portable"
if ($Suffix) { $name += "-$Suffix" }
$outputDir = [System.IO.Path]::GetFullPath((Join-Path $releaseDir 'bundle/portable'))
$staging = Join-Path $outputDir ".staging-$([guid]::NewGuid().ToString('N'))"
$packageDir = Join-Path $staging $name
$temporaryZip = Join-Path $outputDir ".$name-$([guid]::NewGuid().ToString('N')).zip"
$archivePath = Join-Path $outputDir "$name.zip"
try {
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    Copy-Item -LiteralPath $executable -Destination $packageDir
    foreach ($dll in @($runtimeDlls) + @($crtDlls)) { Copy-Item -LiteralPath $dll.FullName -Destination $packageDir }
    foreach ($resource in $resources.GetEnumerator()) {
        $source = [System.IO.Path]::GetFullPath((Join-Path $projectDir $resource.Key))
        $destination = [System.IO.Path]::GetFullPath((Join-Path $packageDir $resource.Value))
        if (-not $destination.StartsWith("$packageDir$([System.IO.Path]::DirectorySeparatorChar)", [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Resource destination escapes the portable package: $($resource.Value)"
        }
        if ($source.EndsWith('.dll', [System.StringComparison]::OrdinalIgnoreCase)) { Assert-PeArchitecture $source }
        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
        Copy-Item -LiteralPath $source -Destination $destination
    }
    @"
Lap $version ($arch) - portable

Extract the entire ZIP and run Lap.exe. Keep the DLLs, resources and licenses together.
Microsoft Visual C++ runtime DLLs are included. Microsoft Edge WebView2 Runtime must be installed.
Settings, libraries, caches and downloaded models use Lap's normal per-user data directories.
AI models, FFmpeg and the optional CUDA runtime are downloaded separately inside Lap.

To update this portable copy, quit Lap and extract the new ZIP into a new folder.
The built-in Windows updater uses the MSI installer; use ZIP downloads to stay installation-free.
"@ | Set-Content -LiteralPath (Join-Path $packageDir 'README-portable.txt') -Encoding utf8
    [System.IO.Compression.ZipFile]::CreateFromDirectory($packageDir, $temporaryZip, [System.IO.Compression.CompressionLevel]::Optimal, $true)
    Move-Item -LiteralPath $temporaryZip -Destination $archivePath -Force
    Write-Host "Created portable package: $archivePath"
} finally {
    # Delete only this invocation's staging directory inside the explicitly selected output directory.
    if ($staging.StartsWith("$outputDir$([System.IO.Path]::DirectorySeparatorChar)", [System.StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $staging)) {
        Remove-Item -LiteralPath $staging -Recurse -Force
    }
    if (Test-Path -LiteralPath $temporaryZip) { Remove-Item -LiteralPath $temporaryZip -Force }
}
