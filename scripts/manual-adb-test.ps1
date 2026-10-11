param(
    [string]$ServerUrl = "https://localhost:8443",
    [string]$Domain = "localhost",
    [string]$Ca = "server.pem",
    [string]$Db = "adb-remote-server.sqlite3",
    [string]$Cert = "server.pem",
    [string]$Key = "server.key",
    [int]$Port = 8443,
    [int]$LocalPort = 5555
)

$ErrorActionPreference = "Stop"

function Step($Message) {
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

Step "Build binaries"
cargo build

Step "Check local adb devices"
adb devices -l

$serial = Read-Host "Enter the adb serial to share"
$displayName = Read-Host "Enter display name for this test device"

Step "Initialize server database"
cargo run -p adb-remote-server -- init --db $Db

Write-Host ""
Write-Host "Start the server in another PowerShell window:"
Write-Host "cargo run -p adb-remote-server -- serve --listen 0.0.0.0:$Port --db $Db --cert $Cert --key $Key"
Read-Host "Press Enter after the server is running"

Step "Register device as passwordless"
cargo run -p adb-remote-client -- add-device $serial --name $displayName --no-password

Write-Host ""
Write-Host "Start the device client in another PowerShell window:"
Write-Host "cargo run -p adb-remote-client -- serve --server $ServerUrl --ca $Ca --domain $Domain"
Read-Host "Press Enter after the device client is running"

Step "List remote devices"
cargo run -p adb-remote -- devices --server $ServerUrl --ca $Ca --domain $Domain

Write-Host ""
Write-Host "Start the user tunnel in another PowerShell window:"
Write-Host "cargo run -p adb-remote -- connect $displayName --server $ServerUrl --ca $Ca --domain $Domain --local-port $LocalPort"
Read-Host "Press Enter after the tunnel says it is leased"

Step "Connect adb to local tunnel"
adb connect "127.0.0.1:$LocalPort"
adb devices

Step "Run adb shell smoke test"
adb -s "127.0.0.1:$LocalPort" shell getprop ro.product.model

Write-Host ""
Write-Host "Manual smoke test complete. Press Ctrl+C in the tunnel window to release the lease."
