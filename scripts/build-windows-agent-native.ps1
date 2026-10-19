# Native ARM64 release package, including the Windows SDK capture helper.
[CmdletBinding()]
param([ValidatePattern('^[a-zA-Z0-9._-]+$')][string]$Version='development')
$ErrorActionPreference='Stop'
if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne [Runtime.InteropServices.Architecture]::Arm64) {throw 'Build this package on native Windows ARM64'}
$repo=Split-Path $PSScriptRoot
$stage=Join-Path $repo ('target/package-stage-'+[Guid]::NewGuid().ToString('N'))
$previousFlags=$env:CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_RUSTFLAGS
Push-Location $repo
try {
 $env:CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_RUSTFLAGS=($previousFlags+' -C target-feature=+crt-static').Trim()
 cargo build --locked -p freelane-guest-agent --release --target aarch64-pc-windows-msvc
 if ($LASTEXITCODE -ne 0) {throw 'ARM64 guest agent build failed'}
 $null=New-Item -ItemType Directory -Path (Join-Path $stage 'notices') -Force
 Copy-Item -LiteralPath 'target/aarch64-pc-windows-msvc/release/freelane-guest-agent.exe' -Destination $stage
 & (Join-Path $PSScriptRoot 'build-windows-video-helper.ps1') -OutputPath (Join-Path $stage 'freelane-video-helper.exe')
 # Compiler intermediates never enter the distributable.
 Remove-Item -LiteralPath (Join-Path $stage 'freelane-video-helper.exe.obj') -ErrorAction SilentlyContinue
 Copy-Item -LiteralPath 'scripts/install-windows-agent.ps1','scripts/setup-windows-agent.ps1' -Destination $stage
 Copy-Item -LiteralPath 'LICENSE' -Destination (Join-Path $stage 'notices/FreeLane-MIT.txt')
 $metadata=cargo metadata --locked --format-version 1 --filter-platform aarch64-pc-windows-msvc
 if ($LASTEXITCODE -ne 0) {throw 'Cargo metadata failed'}
 $metadataPath=Join-Path $stage 'cargo-dependencies.json'
 [IO.File]::WriteAllText($metadataPath,($metadata -join "`n"),[Text.UTF8Encoding]::new($false))
 python scripts/package-license-notices.py $metadataPath (Join-Path $stage 'notices/rust')
 if ($LASTEXITCODE -ne 0) {throw 'Rust license notices failed'}
 $agent=Join-Path $stage 'freelane-guest-agent.exe'
 $hash=(Get-FileHash -LiteralPath $agent -Algorithm SHA256).Hash.ToLowerInvariant()
 [IO.File]::WriteAllText(($agent+'.sha256'),($hash+"  freelane-guest-agent.exe`n"),[Text.UTF8Encoding]::new($false))
 $packages=Join-Path $repo 'target/packages';$null=New-Item -ItemType Directory -Path $packages -Force
 $name='FreeLane-'+$Version+'-windows-arm64-agent.zip'
 $archive=Join-Path $packages $name
 Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -Force
 $checksum=(Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
 [IO.File]::WriteAllText(($archive+'.sha256'),($checksum+'  '+$name+"`n"),[Text.UTF8Encoding]::new($false))
 Write-Output ('Built '+$name)
} finally {
 $env:CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_RUSTFLAGS=$previousFlags
 Pop-Location
 if (Test-Path -LiteralPath $stage) {Remove-Item -LiteralPath $stage -Recurse}
}
