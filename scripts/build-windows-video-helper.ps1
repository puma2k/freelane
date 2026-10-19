# Native Windows SDK build. No elevation, downloads or third-party codec runtime.
[CmdletBinding()]
param(
 [ValidateSet('amd64','arm64')][string]$Architecture='arm64',
 [string]$OutputPath='target/guest-agent-arm64/freelane-video-helper.exe'
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot
$destination=[IO.Path]::GetFullPath($OutputPath)
$folder=[IO.Path]::GetDirectoryName($destination)
$null=New-Item -ItemType Directory -Force -Path $folder
$vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs=& $vswhere -latest -products '*' -property installationPath
if (!$vs) {throw 'Visual Studio C++ tools and Windows SDK are required'}
$hostArchitecture=if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq [Runtime.InteropServices.Architecture]::Arm64) {'arm64'} else {'amd64'}
$batch=Join-Path ([IO.Path]::GetTempPath()) ('freelane-helper-'+[Guid]::NewGuid().ToString('N')+'.cmd')
$source=Join-Path $repo 'scripts/video/worker-windows.cpp'
try {
 @('@echo off',"call `"$vs\Common7\Tools\VsDevCmd.bat`" -no_logo -host_arch=$hostArchitecture -arch=$Architecture",'if errorlevel 1 exit /b 1',"cl /nologo /std:c++20 /EHsc /W4 /MT /DUNICODE /D_UNICODE /Fo:`"$destination.obj`" /Fe:`"$destination`" `"$source`" mfplat.lib mfuuid.lib ole32.lib oleaut32.lib d3d11.lib dxgi.lib windowsapp.lib user32.lib gdi32.lib dwmapi.lib",'exit /b %errorlevel%') | Set-Content -LiteralPath $batch -Encoding ascii
 & cmd /d /c $batch
 if ($LASTEXITCODE -ne 0) {throw 'Windows video helper compilation failed'}
 $hash=(Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
 [IO.File]::WriteAllText(($destination+'.sha256'),($hash+'  '+[IO.Path]::GetFileName($destination)+"`n"),[Text.UTF8Encoding]::new($false))
} finally {Remove-Item -LiteralPath $batch -ErrorAction SilentlyContinue}
