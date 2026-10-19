param([Parameter(Mandatory=$true)][string]$AgentPath,[Parameter(Mandatory=$true)][string]$NativeAgentPath)
$ErrorActionPreference='Stop'
$phase='initial checksum'
$repo=Split-Path $PSScriptRoot
$script=Join-Path $repo 'scripts/install-windows-agent.ps1'
# Refuse to touch an existing real installation; only this test's fresh directory is cleaned.
$directory=Join-Path $env:LOCALAPPDATA 'FreeLane'
$shortcut=Join-Path ([Environment]::GetFolderPath('Startup')) 'FreeLane Guest Agent.lnk'
if ((Test-Path -LiteralPath $directory) -or (Test-Path -LiteralPath $shortcut)) {throw 'Acceptance requires a fresh runner without an agent installation'}
$root=Join-Path ([IO.Path]::GetTempPath()) ('freelane-lifecycle-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$first=Join-Path $root 'first.exe';Copy-Item -LiteralPath $AgentPath -Destination $first
$hash=(Get-FileHash -LiteralPath $first -Algorithm SHA256).Hash
# Real ARM64 PE fixtures with distinct hashes; install tests never execute these helper fixtures.
$helperFirst=Join-Path $root 'helper-first.exe';[IO.File]::WriteAllBytes($helperFirst,([IO.File]::ReadAllBytes($first)+[byte[]](7,8)))
$helperHash=(Get-FileHash -LiteralPath $helperFirst -Algorithm SHA256).Hash.ToLowerInvariant()
$helperSecond=Join-Path $root 'helper-second.exe';[IO.File]::WriteAllBytes($helperSecond,([IO.File]::ReadAllBytes($first)+[byte[]](7,8,9)))
$helperSecondHash=(Get-FileHash -LiteralPath $helperSecond -Algorithm SHA256).Hash.ToLowerInvariant()
function HelperFile($hash) {Join-Path $directory ('freelane-video-helper-'+$hash+'.exe')}
function Assert($condition,$name) {if (!$condition) {throw "FAILED: $name"}}
function Refused([scriptblock]$call,$name) {$failed=$false;try {& $call | Out-Null} catch {$failed=$true};Assert $failed $name}
function ProbeAgent($settingsPath) {
 $probeDirectory=Join-Path $root 'native-probe'
 if (!(Test-Path -LiteralPath $probeDirectory)) {New-Item -ItemType Directory -Path $probeDirectory | Out-Null}
 $probe=Join-Path $probeDirectory 'freelane-guest-agent.exe'
 Copy-Item -LiteralPath $NativeAgentPath -Destination $probe -Force
 Copy-Item -LiteralPath $settingsPath -Destination (Join-Path $probeDirectory 'agent-install.json') -Force
 $info=New-Object System.Diagnostics.ProcessStartInfo
 $info.FileName=$probe;$info.Arguments='--stdio';$info.UseShellExecute=$false;$info.CreateNoWindow=$true
 $info.RedirectStandardInput=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 $process=New-Object System.Diagnostics.Process
 $process.StartInfo=$info
 try {
  Assert ($process.Start()) 'native settings probe started'
  $process.StandardInput.WriteLine('{"version":3,"id":91,"action":{"type":"ping"}}');$process.StandardInput.Close()
  if (!$process.WaitForExit(5000)) {$process.Kill();throw 'Native settings probe exceeded deadline'}
  Assert ($process.ExitCode -eq 0) 'native agent accepted saved settings'
  $reply=$process.StandardOutput.ReadToEnd() | ConvertFrom-Json
  Assert ($reply.version -eq 3 -and $reply.id -eq 91 -and $reply.outcome.status -eq 'agent_info') 'native discovery reply'
  return $reply.outcome.info.features.name
 } finally {$process.Dispose()}
}
try {
 Refused {& $script -Action Install -AgentPath $first -ExpectedSha256 ('0'*64)} 'tampered checksum refused'
 Assert (!(Test-Path -LiteralPath (Join-Path $directory 'freelane-guest-agent.exe'))) 'invalid source never published'
 $phase='persisted outbox install'
 $outbox=Join-Path $root 'Tài liệu xuất';New-Item -ItemType Directory -Path $outbox | Out-Null
 & $script -Action Install -AgentPath $first -ExpectedSha256 $hash -OutboxDirectory $outbox -VideoHelperPath $helperFirst -ExpectedVideoSha256 $helperHash
 $settings=Get-Content -LiteralPath (Join-Path $directory 'agent-install.json') -Raw | ConvertFrom-Json
 Assert ($settings.transfer.outbox -eq $outbox) 'outbox persisted'
 Assert ($settings.video_sha256 -eq $helperHash -and (Get-FileHash -LiteralPath (HelperFile $helperHash)).Hash.ToLowerInvariant() -eq $helperHash) 'verified paired helper installed'
 Assert (!$settings.transfer.inbox) 'inbox remains disabled'
 $features=ProbeAgent (Join-Path $directory 'agent-install.json')
 Assert ($features -contains 'file_export' -and $features -notcontains 'file_transfer') 'native startup enables only selected outbox'
 $dest=Join-Path $directory 'freelane-guest-agent.exe'
 Assert ((Get-FileHash -LiteralPath $dest).Hash -eq $hash) 'verified install'
 Assert (Test-Path -LiteralPath $shortcut) 'startup shortcut'
 Refused {& $script -Action Install -AgentPath $first -ExpectedSha256 $hash} 'duplicate preserved'
 $second=Join-Path $root 'second.exe';[IO.File]::WriteAllBytes($second,([IO.File]::ReadAllBytes($first)+[byte[]](1,2,3)))
 $secondHash=(Get-FileHash -LiteralPath $second).Hash
 $phase='verified update'
 & $script -Action Update -AgentPath $second -ExpectedSha256 $secondHash -VideoHelperPath $helperSecond -ExpectedVideoSha256 $helperSecondHash
 Assert ((Get-FileHash -LiteralPath $dest).Hash -eq $secondHash) 'update published'
 $settings=Get-Content -LiteralPath (Join-Path $directory 'agent-install.json') -Raw | ConvertFrom-Json
 Assert ($settings.transfer.outbox -eq $outbox) 'update preserves outbox'
 Assert ($settings.video_sha256 -eq $helperSecondHash -and $settings.backup_video_sha256 -eq $helperHash) 'update retains verified paired rollback helper'
 $phase='configure explicit sharing selections'
 $inbox=Join-Path $root 'Inbox';New-Item -ItemType Directory -Path $inbox | Out-Null
 & $script -Action Configure -InboxDirectory $inbox -OutboxDirectory ''
 $settings=Get-Content -LiteralPath (Join-Path $directory 'agent-install.json') -Raw | ConvertFrom-Json
 Assert ($settings.transfer.inbox -eq $inbox -and !$settings.transfer.outbox) 'configure enables inbox and disables outbox'
 Assert ((Get-FileHash -LiteralPath $dest).Hash -eq $secondHash) 'configure preserves executable'
 Assert ($settings.backup_sha256 -eq $hash.ToLowerInvariant()) 'configure preserves rollback binary'
 $features=ProbeAgent (Join-Path $directory 'agent-install.json')
 Assert ($features -contains 'file_transfer' -and $features -notcontains 'file_export') 'native restart applies revoked outbox and selected inbox'
 $configuration=[IO.File]::ReadAllText((Join-Path $directory 'agent-install.json'))
 Refused {& $script -Action Configure -OutboxDirectory (Join-Path $root 'missing')} 'missing folder refused'
 Assert ([IO.File]::ReadAllText((Join-Path $directory 'agent-install.json')) -eq $configuration) 'invalid configuration leaves manifest intact'
 Assert (!(Test-Path -LiteralPath (Join-Path $root 'missing'))) 'missing folder never created'
 $phase='unicode control folder refusal'
 $controlFolder=Join-Path $root ('control-'+[char]0x85);New-Item -ItemType Directory -Path $controlFolder | Out-Null
 Refused {& $script -Action Configure -OutboxDirectory $controlFolder} 'unicode control folder refused before saving'
 Assert ([IO.File]::ReadAllText((Join-Path $directory 'agent-install.json')) -eq $configuration) 'refused control folder preserves working configuration'
 $phase='verified rollback'
 & $script -Action Rollback
 Assert ((Get-FileHash -LiteralPath $dest).Hash -eq $hash) 'rollback restored verified old binary'
 $settings=Get-Content -LiteralPath (Join-Path $directory 'agent-install.json') -Raw | ConvertFrom-Json
 Assert ($settings.transfer.inbox -eq $inbox -and !$settings.transfer.outbox) 'rollback does not resurrect revoked outbox access'
 Assert ($settings.video_sha256 -eq $helperHash) 'rollback restores paired helper'
 $manifest=Join-Path $directory 'agent-install.json';$saved=[IO.File]::ReadAllText($manifest)
 $phase='automatic update failure recovery'
 $held=[IO.File]::Open($manifest,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try {Refused {& $script -Action Configure -OutboxDirectory $outbox} 'locked manifest configure refused';Refused {& $script -Action Update -AgentPath $second -ExpectedSha256 $secondHash} 'locked manifest update refused'} finally {$held.Dispose()}
 Assert ((Get-FileHash -LiteralPath $dest).Hash -eq $hash) 'failed update restored original executable'
 Assert ([IO.File]::ReadAllText($manifest) -eq $saved) 'failed update preserved manifest'
 Assert (Test-Path -LiteralPath $shortcut) 'failed update preserved startup'
 $phase='paired helper corruption refusal'
 [IO.File]::WriteAllBytes((HelperFile $helperHash),[byte[]](1,2,3))
 Refused {& $script -Action Uninstall} 'modified helper preserved on uninstall'
 Refused {& $script -Action Update -AgentPath $second -ExpectedSha256 $secondHash} 'modified helper preserved on update'
 [IO.File]::WriteAllBytes((HelperFile $helperHash),[IO.File]::ReadAllBytes($helperFirst))
 $phase='uninstall lock preservation' 
 $held=[IO.File]::Open($dest,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try {Refused {& $script -Action Uninstall} 'locked executable uninstall refused'} finally {$held.Dispose()}
 Assert (Test-Path -LiteralPath $shortcut) 'failed uninstall preserved shortcut'
 Assert ((Get-FileHash -LiteralPath $dest).Hash -eq $hash) 'failed uninstall preserved executable'

 $bad=$saved | ConvertFrom-Json;$bad.backup='../outside.exe';$bad | ConvertTo-Json | Set-Content -LiteralPath $manifest
 Refused {& $script -Action Rollback} 'backup traversal refused'
 [IO.File]::WriteAllText($manifest,$saved)
 [IO.File]::WriteAllBytes($dest,([IO.File]::ReadAllBytes($first)+[byte[]](9)))
 Refused {& $script -Action Update -AgentPath $second -ExpectedSha256 $secondHash} 'changed installation preserved on update'
 Refused {& $script -Action Uninstall} 'changed installation preserved on uninstall'
 [IO.File]::WriteAllBytes($dest,[IO.File]::ReadAllBytes($first))
 $unrelated=Join-Path $directory 'user-file.txt';[IO.File]::WriteAllText($unrelated,'preserve')
 $phase='verified uninstall'
 & $script -Action Uninstall
 Assert (!(Test-Path -LiteralPath $dest)) 'uninstalled executable'
 Assert (!(Test-Path -LiteralPath (HelperFile $helperHash))) 'uninstalled current managed helper'
 Assert (Test-Path -LiteralPath (HelperFile $helperSecondHash)) 'retained backup helper'
 Assert (!(Test-Path -LiteralPath $shortcut)) 'uninstalled shortcut'
 Assert ([IO.File]::ReadAllText($unrelated) -eq 'preserve') 'unrelated file preserved'
 Assert ((Test-Path -LiteralPath $outbox) -and (Test-Path -LiteralPath $inbox)) 'selected transfer folders preserved'
 Write-Output 'PASS: verified install/update/rollback/uninstall and preservation checks'
} catch {
 # Public diagnostic contains only this controlled test's phase and bounded script/engine failure.
 $message=$_.Exception.Message.Replace('%','%25').Replace("`r",'%0D').Replace("`n",'%0A')
 Write-Output "::error title=Agent lifecycle $phase::$message"
 throw
} finally {
 # Runner fixture only; production installation directories are never recursively removed by the helper.
 if (Test-Path -LiteralPath $shortcut) {Remove-Item -LiteralPath $shortcut}
 if (Test-Path -LiteralPath $directory) {Remove-Item -LiteralPath $directory -Recurse}
 Remove-Item -LiteralPath $root -Recurse
}
