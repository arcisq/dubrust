<#
.SYNOPSIS
    Builds every DubRust release artifact on a Windows machine.

.DESCRIPTION
    Produces, in ./dist:
      * dubrust-<version>-windows-x64-portable.zip - unpack and run, keeps all
        of its data next to the executable (portable.txt marker)
      * DubRust-<version>-windows-x64-setup.exe    - Inno Setup installer with
        shortcuts, uninstaller and bundled ffmpeg
      * SHA256SUMS.txt                             - checksums for both files

    A static ffmpeg/ffprobe build is downloaded once and cached in dist/cache,
    so the shipped app never depends on ffmpeg being present in PATH.

.EXAMPLE
    pwsh -File scripts/package.ps1 -Version 0.1.0
#>
[CmdletBinding()]
param(
    [string]$Version = "0.1.0",
    [switch]$SkipInstaller
)

$ErrorActionPreference = "Stop"
$FfmpegZipUrl = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"

$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root "dist"
$payload = Join-Path $dist "payload"
$cache = Join-Path $dist "cache"

function Step([string]$text) {
    Write-Host "==> $text" -ForegroundColor Cyan
}

Step "Preparing dist folders"
Remove-Item $payload -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $payload, $cache | Out-Null

Step "Building release binary (static CRT)"
Push-Location $root
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
finally {
    Pop-Location
}

Step "Fetching static ffmpeg build (cached)"
$zip = Join-Path $cache "ffmpeg-release-essentials.zip"
if (-not (Test-Path $zip)) {
    Invoke-WebRequest -Uri $FfmpegZipUrl -OutFile $zip -UseBasicParsing
}
$unpacked = Join-Path $cache "ffmpeg"
if (-not (Test-Path $unpacked)) {
    Expand-Archive -Path $zip -DestinationPath $unpacked -Force
}

$ffmpegExe = Get-ChildItem -Path $unpacked -Recurse -Filter "ffmpeg.exe" | Select-Object -First 1
$ffprobeExe = Get-ChildItem -Path $unpacked -Recurse -Filter "ffprobe.exe" | Select-Object -First 1
if (-not $ffmpegExe -or -not $ffprobeExe) { throw "ffmpeg.exe/ffprobe.exe not found in the downloaded archive" }

Step "Staging payload"
Copy-Item (Join-Path $root "target\release\dubrust.exe") $payload
Copy-Item $ffmpegExe.FullName $payload
Copy-Item $ffprobeExe.FullName $payload

$ffLicense = Get-ChildItem -Path $unpacked -Recurse -Include "LICENSE", "LICENSE.txt" -File | Select-Object -First 1
$ffNotice = @(
    "ffmpeg and ffprobe shipped next to dubrust.exe are unmodified binaries from",
    "the FFmpeg project (https://ffmpeg.org), redistributed under the GNU GPL v3.",
    "DubRust only launches them as separate processes via the command line.",
    "Source code for these builds: https://www.gyan.dev/ffmpeg/builds/",
    ""
)
if ($ffLicense) { $ffNotice += (Get-Content $ffLicense.FullName) }
$ffNotice | Set-Content -Path (Join-Path $payload "LICENSE-ffmpeg.txt") -Encoding UTF8

Step "Building portable archive"
$portableName = "dubrust-$Version-windows-x64-portable"
$portableDir = Join-Path $dist $portableName
Remove-Item $portableDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $portableDir | Out-Null
Copy-Item (Join-Path $payload "*") $portableDir
foreach ($doc in @("README.md", "CHANGELOG.md", "LICENSE", "THIRD-PARTY-LICENSES.md")) {
    Copy-Item (Join-Path $root $doc) $portableDir
}
@(
    "This marker file switches DubRust into portable mode.",
    "Model weights, onnxruntime.dll and settings are kept in the ./data folder",
    "next to dubrust.exe instead of %APPDATA%. Nothing is written to the registry.",
    "Delete this file if you want the usual per-user data location."
) | Set-Content -Path (Join-Path $portableDir "portable.txt") -Encoding UTF8

$portableZip = Join-Path $dist "$portableName.zip"
Remove-Item $portableZip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $portableDir "*") -DestinationPath $portableZip

if (-not $SkipInstaller) {
    Step "Building Windows installer (Inno Setup)"
    $isccCandidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    )
    $iscc = $isccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $iscc) {
        Write-Warning "ISCC.exe not found. Install Inno Setup 6 (winget install -e --id JRSoftware.InnoSetup) or pass -SkipInstaller."
    }
    else {
        & $iscc "/DMyAppVersion=$Version" (Join-Path $root "installer\dubrust.iss")
        if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed with exit code $LASTEXITCODE" }
    }
}

Step "Writing SHA256SUMS.txt"
$artifacts = Get-ChildItem -Path $dist -File | Where-Object { $_.Extension -in ".zip", ".exe" }
$lines = foreach ($artifact in $artifacts) {
    $hash = (Get-FileHash -Path $artifact.FullName -Algorithm SHA256).Hash.ToLower()
    "$hash  $($artifact.Name)"
}
$lines | Set-Content -Path (Join-Path $dist "SHA256SUMS.txt") -Encoding ASCII

Step "Done"
$artifacts | ForEach-Object {
    "{0,-52} {1,8:N1} MB" -f $_.Name, ($_.Length / 1MB)
}
