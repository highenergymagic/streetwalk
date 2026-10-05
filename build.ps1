$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$toolchainBin = Join-Path $PSScriptRoot 'tools\w64devkit\bin'
if (Test-Path $toolchainBin) {
    $env:PATH = "$toolchainBin;$env:PATH"
    # w64devkit keeps the SEH unwinder in libgcc; Rust requests libgcc_eh by name.
    $unwindAlias = Join-Path $PSScriptRoot 'tools\w64devkit\lib\libgcc_eh.a'
    if (-not (Test-Path $unwindAlias)) {
        Set-Content -LiteralPath $unwindAlias -Value 'INPUT(-lgcc)' -Encoding ASCII
    }
}
cargo fmt --check
if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed' }
cargo test --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
$destination = Join-Path $PSScriptRoot 'dist\Streetwalk'
New-Item -ItemType Directory -Force $destination | Out-Null
$executable = Join-Path $destination 'streetwalk.exe'
try {
    Copy-Item -LiteralPath 'target\release\streetwalk.exe' -Destination $executable -ErrorAction Stop
} catch [System.IO.IOException] {
    $executable = Join-Path $destination 'streetwalk-updated.exe'
    Copy-Item -LiteralPath 'target\release\streetwalk.exe' -Destination $executable -ErrorAction Stop
    Write-Warning 'Streetwalk is running. Close it before replacing streetwalk.exe; this build was saved as streetwalk-updated.exe.'
}
Copy-Item -LiteralPath 'README.md' -Destination $destination
Copy-Item -LiteralPath 'USERGUIDE.md' -Destination $destination
Copy-Item -LiteralPath 'SERVICES.md' -Destination $destination
Copy-Item -LiteralPath 'EV-AUDIO-MODEL.md' -Destination $destination
Copy-Item -LiteralPath 'LICENSE' -Destination $destination
Copy-Item -LiteralPath 'THIRD_PARTY.md' -Destination $destination
Copy-Item -LiteralPath 'assets\hrtf\README.txt' -Destination (Join-Path $destination 'HRTF-DATA-NOTICE.txt')
Copy-Item -LiteralPath 'assets\ev\README.txt' -Destination (Join-Path $destination 'EV-TEXTURE-NOTICE.txt')
Copy-Item -LiteralPath 'assets\soundscape\LICENSE.txt' -Destination (Join-Path $destination 'SOUNDSCAPE-LICENSE.txt')
$metadata = cargo metadata --locked --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Could not collect dependency notices' }
$notices = [System.Text.StringBuilder]::new()
foreach ($package in $metadata.packages | Where-Object name -NE 'streetwalk' | Sort-Object name) {
    [void]$notices.AppendLine("$($package.name) $($package.version) - $($package.license)")
    [void]$notices.AppendLine($package.repository)
    $packageDirectory = Split-Path $package.manifest_path
    foreach ($licenseFile in Get-ChildItem -LiteralPath $packageDirectory -File | Where-Object Name -Match '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)') {
        [void]$notices.AppendLine([System.IO.File]::ReadAllText($licenseFile.FullName))
    }
    [void]$notices.AppendLine("`r`n")
}
[System.IO.File]::WriteAllText((Join-Path $destination 'THIRD-PARTY-NOTICES.txt'), $notices.ToString())
if (Test-Path 'tools\w64devkit\COPYING.MinGW-w64-runtime.txt') {
    Copy-Item -LiteralPath 'tools\w64devkit\COPYING.MinGW-w64-runtime.txt' -Destination $destination
}
$controller = Join-Path $PSScriptRoot 'dist\nvda-controller'
if (Test-Path "$controller\x64\nvdaControllerClient.dll") {
    Copy-Item -LiteralPath "$controller\x64\nvdaControllerClient.dll" -Destination $destination
    Copy-Item -LiteralPath "$controller\license.txt" -Destination "$destination\NVDA-controller-license.txt"
    Copy-Item -LiteralPath "$controller\readme.md" -Destination "$destination\NVDA-controller-readme.md"
} else {
    Write-Warning 'Place the official x64 nvdaControllerClient.dll beside streetwalk.exe for speech. See USERGUIDE.md.'
}
Write-Host "Ready: $executable"
