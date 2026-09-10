# Build release binary
cargo build --release

# Ensure output directory exists
New-Item -ItemType Directory -Force -Path "releases\download" | Out-Null

# Copy standalone binaries
Copy-Item -Path "target\release\clipped.exe" -Destination "releases\download\clipped.exe" -Force
Copy-Item -Path "target\release\clipped.exe" -Destination "releases\download\clipped_0.5.0_x64.exe" -Force

# Create zip bundle
Compress-Archive -Path "target\release\clipped.exe", "README.md" -DestinationPath "releases\download\clipped-v0.5.0-windows-x64.zip" -Force

# Compile NSIS installer
$makensis = (Get-ChildItem "C:\Program Files*\NSIS\makensis.exe", "$env:LOCALAPPDATA\Programs\NSIS\makensis.exe" -ErrorAction SilentlyContinue | Select-Object -First 1).FullName
if ($makensis) {
    & $makensis "installer.nsi"
} else {
    Write-Warning "makensis.exe not found. Install NSIS via 'winget install NSIS.NSIS'"
}

# Update SHA256 hashes
$hashes = Get-ChildItem "releases\download" -Exclude "SHA256SUMS.txt" | Get-FileHash -Algorithm SHA256 | ForEach-Object { "$($_.Hash)  $($_.Path | Split-Path -Leaf)" }
$hashes | Out-File -Encoding utf8 "releases\download\SHA256SUMS.txt"

Write-Host "Build and packaging complete."
