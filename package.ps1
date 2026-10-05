param([switch]$IncludeTomTomKey)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

& (Join-Path $PSScriptRoot 'build.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }

$built = Join-Path $PSScriptRoot 'dist\Streetwalk'
$key = Join-Path $built 'data\tomtom_key.txt'
if ($IncludeTomTomKey -and -not (Test-Path -LiteralPath $key -PathType Leaf)) {
    throw 'TomTom key file is missing from dist\Streetwalk\data\tomtom_key.txt'
}
$controller = Join-Path $built 'nvdaControllerClient.dll'
if (-not (Test-Path -LiteralPath $controller -PathType Leaf)) {
    throw 'NVDA controller DLL is missing from dist\Streetwalk'
}

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$destination = Join-Path $PSScriptRoot "dist\Streetwalk-portable-$stamp"
if (Test-Path -LiteralPath $destination) { throw "Package already exists: $destination" }
New-Item -ItemType Directory -Path $destination | Out-Null
New-Item -ItemType Directory -Path (Join-Path $destination 'data') | Out-Null

foreach ($file in @('README.md', 'SERVICES.md', 'EV-AUDIO-MODEL.md', 'LICENSE', 'THIRD_PARTY.md', 'Cargo.toml', 'Cargo.lock', 'build.ps1', 'package.ps1', '.gitignore')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination $destination
}
foreach ($folder in @('src', 'assets', 'tests')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $folder) -Destination $destination -Recurse
}
$researchDestination = Join-Path $destination 'research'
New-Item -ItemType Directory -Path $researchDestination | Out-Null
Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'research') -File |
    Copy-Item -Destination $researchDestination
$leafDestination = Join-Path $researchDestination 'leaf2017'
New-Item -ItemType Directory -Path $leafDestination | Out-Null
Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'research\leaf2017') -File |
    Where-Object Extension -In @('.py', '.md', '.csv', '.json') |
    Copy-Item -Destination $leafDestination
foreach ($file in @('nvdaControllerClient.dll', 'NVDA-controller-license.txt', 'NVDA-controller-readme.md', 'SOUNDSCAPE-LICENSE.txt', 'HRTF-DATA-NOTICE.txt', 'EV-TEXTURE-NOTICE.txt', 'THIRD-PARTY-NOTICES.txt')) {
    Copy-Item -LiteralPath (Join-Path $built $file) -Destination $destination
}
$mingwNotice = Join-Path $built 'COPYING.MinGW-w64-runtime.txt'
if (Test-Path -LiteralPath $mingwNotice) {
    Copy-Item -LiteralPath $mingwNotice -Destination $destination
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'target\release\streetwalk.exe') -Destination $destination
if ($IncludeTomTomKey) {
    Copy-Item -LiteralPath $key -Destination (Join-Path $destination 'tomtom_key.txt')
}

$zip = "$destination.zip"
Compress-Archive -LiteralPath $destination -DestinationPath $zip
Write-Host "Portable folder: $destination"
Write-Host "Portable ZIP: $zip"
