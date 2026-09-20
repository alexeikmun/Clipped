$ErrorActionPreference = "Stop"
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
# Fail before touching release artifacts when the installer tool is unavailable.
$makensis = (Get-ChildItem "C:\Program Files*\NSIS\makensis.exe", "$env:LOCALAPPDATA\Programs\NSIS\makensis.exe" -ErrorAction SilentlyContinue | Select-Object -First 1).FullName
if (-not $makensis) {
    throw "makensis.exe not found. Install NSIS before packaging."
}

# Build release binary
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "Release build failed ($LASTEXITCODE)." }

& $makensis "installer.nsi"
if ($LASTEXITCODE -ne 0) { throw "Installer build failed ($LASTEXITCODE)." }

# Ensure output directory exists
New-Item -ItemType Directory -Force -Path "releases\download" | Out-Null

# Copy standalone binaries
Copy-Item -Path "target\release\clipped.exe" -Destination "releases\download\clipped.exe" -Force
Copy-Item -Path "target\release\clipped.exe" -Destination "releases\download\clipped_0.5.0_x64.exe" -Force

# Create zip bundle
Compress-Archive -Path "target\release\clipped.exe", "README.md" -DestinationPath "releases\download\clipped-v0.5.0-windows-x64.zip" -Force

# Update SHA256 hashes
$hashes = Get-ChildItem "releases\download" -Exclude "SHA256SUMS.txt" | Get-FileHash -Algorithm SHA256 | ForEach-Object { "$($_.Hash)  $($_.Path | Split-Path -Leaf)" }
$hashes | Out-File -Encoding utf8 "releases\download\SHA256SUMS.txt"

Write-Host "Build and packaging complete."
} finally {
    Pop-Location
}
