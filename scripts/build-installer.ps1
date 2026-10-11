param(
    [string]$Version = "0.1.0",
    [string]$Makensis = "C:\Program Files (x86)\NSIS\makensis.exe"
)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")

if (!(Test-Path $Makensis)) {
    throw "makensis.exe not found at '$Makensis'. Pass -Makensis with the NSIS path."
}

Push-Location $repo
try {
    cargo build --release
    New-Item -ItemType Directory -Force -Path "dist" | Out-Null
    & $Makensis "/DVERSION=$Version" "/DSOURCE_ROOT=$repo" "packaging\adb-remote.nsi"
    if ($LASTEXITCODE -ne 0) {
        throw "makensis failed with exit code $LASTEXITCODE"
    }
    Write-Host "Installer written to dist\adb-remote-$Version-windows-x64-setup.exe"
}
finally {
    Pop-Location
}
