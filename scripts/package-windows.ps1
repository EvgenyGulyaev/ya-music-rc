$ErrorActionPreference = "Stop"

$version = $env:VERSION
if ([string]::IsNullOrWhiteSpace($version)) {
    $versionLine = Get-Content Cargo.toml | Where-Object { $_ -match '^version = "(.+)"' } | Select-Object -First 1
    if ($versionLine -match '^version = "(.+)"') {
        $version = $Matches[1]
    }
}

if ([string]::IsNullOrWhiteSpace($version)) {
    throw "Cannot read package version from Cargo.toml"
}

$packageName = "ya-player-windows-x86_64-$version"
$packageDir = "target/release/package/$packageName"
$archivePath = "target/release/Ya-Player-windows-x86_64-$version.zip"
$binaryPath = "target/release/ya-player.exe"

if (!(Test-Path $binaryPath)) {
    throw "Missing $binaryPath. Run cargo build --release first."
}

Remove-Item -Recurse -Force $packageDir, $archivePath -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $packageDir | Out-Null

Copy-Item $binaryPath "$packageDir/Ya Player.exe"
Copy-Item README.md "$packageDir/README.md"

Compress-Archive -Path "$packageDir/*" -DestinationPath $archivePath -Force
Write-Output $archivePath
