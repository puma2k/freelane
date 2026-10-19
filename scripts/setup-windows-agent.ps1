# Run from the verified release ZIP after extraction in the Windows user session.
[CmdletBinding()]
param([ValidateSet('Auto','Install','Update')][string]$Action='Auto')
$ErrorActionPreference='Stop'
function PackageHash([string]$name) {
 $path=Join-Path $PSScriptRoot ($name+'.sha256')
 if ((Get-Item -LiteralPath $path).Length -gt 256) {throw 'Invalid package checksum record'}
 $record=[IO.File]::ReadAllText($path).Trim()
 if ($record -notmatch ('^([0-9a-f]{64})  '+[regex]::Escape($name)+'$')) {throw 'Invalid package checksum record'}
 return $Matches[1]
}
if ($Action -eq 'Auto') {
 $manifest=Join-Path $env:LOCALAPPDATA 'FreeLane/agent-install.json'
 $Action=if (Test-Path -LiteralPath $manifest) {'Update'} else {'Install'}
}
& (Join-Path $PSScriptRoot 'install-windows-agent.ps1') -Action $Action `
 -AgentPath (Join-Path $PSScriptRoot 'freelane-guest-agent.exe') -ExpectedSha256 (PackageHash 'freelane-guest-agent.exe') `
 -VideoHelperPath (Join-Path $PSScriptRoot 'freelane-video-helper.exe') -ExpectedVideoSha256 (PackageHash 'freelane-video-helper.exe')
