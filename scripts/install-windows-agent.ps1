# Per-user lifecycle only: no elevation, service, driver or unowned process changes.
[CmdletBinding()]
param(
 [ValidateSet('Install','Update','Rollback','Uninstall','Configure')][string]$Action='Install',
 [string]$AgentPath,
 [string]$ExpectedSha256,
 [string]$VideoHelperPath,
 [string]$ExpectedVideoSha256,
 [AllowEmptyString()][string]$InboxDirectory,
 [AllowEmptyString()][string]$OutboxDirectory
)
$ErrorActionPreference='Stop'
$inboxRequested=$PSBoundParameters.ContainsKey('InboxDirectory')
$outboxRequested=$PSBoundParameters.ContainsKey('OutboxDirectory')
$videoRequested=$PSBoundParameters.ContainsKey('VideoHelperPath') -or $PSBoundParameters.ContainsKey('ExpectedVideoSha256')
if ($videoRequested -and $Action -notin @('Install','Update')) {throw 'Video helper can be supplied only during Install or Update'}
if (($inboxRequested -or $outboxRequested) -and $Action -in @('Rollback','Uninstall')) {throw 'Transfer folders can be changed only during Install, Update or Configure'}
if ($Action -eq 'Configure' -and !$inboxRequested -and !$outboxRequested) {throw 'Select a transfer folder or pass an empty value to disable it'}
function ValidStoredRoot($path) {
 if ($null -eq $path) {return}
 if ($path -isnot [string] -or !$path -or [Text.Encoding]::UTF8.GetByteCount($path) -gt 2048 -or ![IO.Path]::IsPathRooted($path) -or [IO.Path]::GetFullPath($path) -ine $path) {throw 'Invalid saved transfer folder'}
 foreach ($character in $path.ToCharArray()) {if ([char]::IsControl($character)) {throw 'Invalid saved transfer folder'}}
}
function SelectedFolder([string]$path) {
 if (!$path) {return $null}
 ValidStoredRoot $path
 $resolved=Resolve-Path -LiteralPath $path
 if ($resolved.Provider.Name -ne 'FileSystem') {throw 'Select an existing filesystem folder'}
 $item=Get-Item -LiteralPath $resolved.ProviderPath -Force
 if (!$item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {throw 'Transfer folder must be an existing ordinary directory'}
 return $item.FullName
}
$selectedInbox=$null;$selectedOutbox=$null
if ($inboxRequested) {$selectedInbox=SelectedFolder $InboxDirectory}
if ($outboxRequested) {$selectedOutbox=SelectedFolder $OutboxDirectory}
function TransferSelections($previous) {
 $selection=@{inbox=$null;outbox=$null}
 if ($previous -and $previous.transfer) {$selection.inbox=$previous.transfer.inbox;$selection.outbox=$previous.transfer.outbox}
 if ($inboxRequested) {$selection.inbox=$selectedInbox}
 if ($outboxRequested) {$selection.outbox=$selectedOutbox}
 return $selection
}
function WriteManifestStage([string]$path,$value) {
 $json=$value | ConvertTo-Json -Depth 4 -Compress
 if ([Text.Encoding]::UTF8.GetByteCount($json) -gt 4096) {throw 'Transfer settings exceed the installation manifest limit'}
 [IO.File]::WriteAllText($path,$json,[Text.UTF8Encoding]::new($false))
}
$directory=Join-Path $env:LOCALAPPDATA 'FreeLane'
$destination=Join-Path $directory 'freelane-guest-agent.exe'
$manifestPath=Join-Path $directory 'agent-install.json'
$shortcut=Join-Path ([Environment]::GetFolderPath('Startup')) 'FreeLane Guest Agent.lnk'
function Hash([string]$path) {return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
function PlainFile([string]$path) {
 $item=Get-Item -LiteralPath $path -Force
 if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {throw 'Agent files must be ordinary files'}
}
function Arm64([string]$path) {
 $stream=[IO.File]::OpenRead($path)
 try {
  if ($stream.Length -lt 64) {throw 'Invalid ARM64 executable'}
  $reader=New-Object IO.BinaryReader($stream)
  if ($reader.ReadUInt16() -ne 0x5a4d) {throw 'Invalid ARM64 executable'}
  $stream.Position=0x3c;$offset=$reader.ReadUInt32()
  if ($offset -gt ($stream.Length-6)) {throw 'Invalid ARM64 executable'}
  $stream.Position=$offset
  if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0xaa64) {throw 'Supply the Windows ARM64 agent executable'}
 } finally {$stream.Dispose()}
}
function HelperPath([string]$hash) {return (Join-Path $directory ('freelane-video-helper-'+$hash+'.exe'))}
function VerifiedHelper([string]$hash) {
 if (!$hash -or $hash -notmatch '^[0-9a-f]{64}$') {throw 'Invalid helper checksum'}
 $path=HelperPath $hash
 PlainFile $path;Arm64 $path
 if ((Hash $path) -ne $hash) {throw 'Installed helper changed; preserve and inspect it before continuing'}
 return $path
}
function ReadManifest {
 PlainFile $manifestPath
 if ((Get-Item -LiteralPath $manifestPath).Length -gt 4096) {throw 'Invalid installation manifest'}
 $value=[IO.File]::ReadAllText($manifestPath) | ConvertFrom-Json
 if ($value.schema -ne 1 -or $value.sha256 -notmatch '^[0-9a-f]{64}$') {throw 'Invalid installation manifest'}
 if ($value.backup -and ($value.backup -notmatch '^agent-backup-[0-9a-f]{32}\.exe$' -or $value.backup_sha256 -notmatch '^[0-9a-f]{64}$')) {throw 'Invalid backup metadata'}
 foreach ($field in @('video_sha256','backup_video_sha256')) {
  if ($null -ne $value.$field -and ($value.$field -isnot [string] -or $value.$field -notmatch '^[0-9a-f]{64}$')) {throw 'Invalid helper metadata'}
 }
 if ($value.PSObject.Properties.Name -contains 'transfer') {
  if (!$value.transfer -or $value.transfer -isnot [pscustomobject]) {throw 'Invalid saved transfer settings'}
  foreach ($property in $value.transfer.PSObject.Properties) {
   if ($property.Name -notin @('inbox','outbox')) {throw 'Invalid saved transfer settings'}
   ValidStoredRoot $property.Value
  }
 }
 return $value
}
function ManagedInstallation {
 $value=ReadManifest
 if ($value.video_sha256) {$null=VerifiedHelper $value.video_sha256}
 PlainFile $destination
 if ((Hash $destination) -ne $value.sha256) {throw 'Installed agent changed; preserve and inspect it before updating or uninstalling'}
 PlainFile $shortcut
 $link=(New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
 if ($link.TargetPath -ine $destination -or $link.Arguments) {throw 'Startup shortcut changed; preserve and inspect it'}
 # An active executable can be locked by Windows. Never terminate a process by name.
 foreach ($process in @(Get-Process -Name 'freelane-guest-agent' -ErrorAction SilentlyContinue)) {
  try {$path=$process.Path} catch {throw 'Cannot inspect running agent; stop it before continuing'}
  if (!$path -or $path -ieq $destination) {throw 'Stop the installed agent in your user session before continuing'}
 }
 foreach ($process in @(Get-Process -Name 'freelane-video-helper*' -ErrorAction SilentlyContinue)) {
  try {$path=$process.Path} catch {throw 'Cannot inspect running helper; stop it before continuing'}
  if (!$path -or ($value.video_sha256 -and $path -ieq (HelperPath $value.video_sha256))) {throw 'Stop the installed video helper in your user session before continuing'}
 }
 return $value
}
if ($Action -in @('Install','Update')) {
 if (!$AgentPath -or $ExpectedSha256 -notmatch '^[0-9a-fA-F]{64}$') {throw 'Install/update requires AgentPath and the trusted package ExpectedSha256'}
 $source=(Resolve-Path -LiteralPath $AgentPath).Path
 PlainFile $source
 if ([IO.Path]::GetExtension($source) -ine '.exe') {throw 'Supply the ARM64 guest-agent .exe'}
 Arm64 $source
 if ((Hash $source) -ne $ExpectedSha256.ToLowerInvariant()) {throw 'Agent checksum verification failed; installation refused'}
 if ($videoRequested) {
  if (!$VideoHelperPath -or $ExpectedVideoSha256 -notmatch '^[0-9a-fA-F]{64}$') {throw 'Supply VideoHelperPath and trusted ExpectedVideoSha256 together'}
  $helperSource=(Resolve-Path -LiteralPath $VideoHelperPath).Path
  PlainFile $helperSource;Arm64 $helperSource
  $ExpectedVideoSha256=$ExpectedVideoSha256.ToLowerInvariant()
  if ((Hash $helperSource) -ne $ExpectedVideoSha256) {throw 'Video helper checksum verification failed; installation refused'}
 }

}
if (!(Test-Path -LiteralPath $directory)) {
 if ($Action -ne 'Install') {throw 'No managed agent installation found'}
 New-Item -ItemType Directory -Path $directory | Out-Null
}
$item=Get-Item -LiteralPath $directory -Force
if (!$item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {throw 'Installation directory must be an ordinary directory'}
$lockPath=Join-Path $directory 'agent-install.lock'
if (Test-Path -LiteralPath $lockPath) {PlainFile $lockPath}
$lock=[IO.File]::Open($lockPath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$helperStage=$null;$createdHelper=$null;$helperCommitted=$false
$stage=$null;$stageManifest=$null;$stageShortcut=$null;$backup=$null;$manifestBackup=$null;$published=$false;$oldManifest=$null;$installedShortcut=$false
try {
 if ($Action -eq 'Install') {
  foreach ($path in @($destination,$shortcut,$manifestPath)) {if (Test-Path -LiteralPath $path) {throw 'Existing installation or shortcut found; no files replaced'}}
 } else {$oldManifest=ManagedInstallation}
 if ($Action -eq 'Configure') {
  # One atomic manifest replacement: executable, binary backups and Startup link stay intact.
  $configured=@{}
  foreach ($property in $oldManifest.PSObject.Properties) {$configured[$property.Name]=$property.Value}
  $configured.transfer=TransferSelections $oldManifest
  $stageManifest=Join-Path $directory ('manifest-stage-'+[Guid]::NewGuid().ToString('N')+'.json')
  WriteManifestStage $stageManifest $configured
  # Refuse changed installations before publication; process shutdown remains explicit.
  $checked=ManagedInstallation
  if ($checked.sha256 -ne $oldManifest.sha256) {throw 'Installation changed during configuration'}
  $manifestBackup=Join-Path $directory ('manifest-config-backup-'+[Guid]::NewGuid().ToString('N')+'.json')
  [IO.File]::Replace($stageManifest,$manifestPath,$manifestBackup)
  Write-Output 'Transfer folder selections saved. Restart the agent in your user session, or sign in again. No folders were created.'
  return
 }
 if ($Action -eq 'Uninstall') {
  # Rename on each original filesystem before committing; a locked file refuses safely.
  $id=[Guid]::NewGuid().ToString('N')
  $removedAgent=Join-Path $directory ('agent-removed-'+$id+'.exe')
  $removedManifest=Join-Path $directory ('manifest-removed-'+$id+'.json')
  $removedShortcut=Join-Path ([IO.Path]::GetDirectoryName($shortcut)) ('FreeLane-removed-'+$id+'.lnk')
  $removedHelper=$null;$currentHelper=$null;$movedHelper=$false
  if ($oldManifest.video_sha256) {$currentHelper=VerifiedHelper $oldManifest.video_sha256;$removedHelper=Join-Path $directory ('helper-removed-'+$id+'.exe')}
  $movedAgent=$false;$movedShortcut=$false
  try {
   [IO.File]::Move($destination,$removedAgent);$movedAgent=$true
   if ($currentHelper) {[IO.File]::Move($currentHelper,$removedHelper);$movedHelper=$true}
   [IO.File]::Move($shortcut,$removedShortcut);$movedShortcut=$true
   [IO.File]::Move($manifestPath,$removedManifest)
  } catch {
   $failure=$_
   # Move refuses an existing replacement; never overwrite another user's new file.
   if ($movedShortcut) {[IO.File]::Move($removedShortcut,$shortcut)}
   if ($movedHelper) {[IO.File]::Move($removedHelper,$currentHelper)}
   if ($movedAgent) {[IO.File]::Move($removedAgent,$destination)}
   throw $failure
  }
  foreach ($removed in @($removedAgent,$removedShortcut,$removedManifest,$removedHelper)) {
   if (!$removed) {continue}
   try {Remove-Item -LiteralPath $removed} catch {Write-Output 'Uninstalled; a quarantined file is retained because Windows refused cleanup.'}
  }
  Write-Output 'Uninstalled managed agent and Startup shortcut. Backups and other user files are preserved.'
  return
 }
 if ($Action -eq 'Rollback') {
  if (!$oldManifest.backup) {throw 'No verified backup available'}
  $source=Join-Path $directory $oldManifest.backup
  PlainFile $source
  $ExpectedSha256=$oldManifest.backup_sha256
  if ((Hash $source) -ne $ExpectedSha256) {throw 'Backup checksum verification failed; rollback refused'}
  Arm64 $source
 }
 $expected=$ExpectedSha256.ToLowerInvariant()
 $id=[Guid]::NewGuid().ToString('N')
 $selectedVideoHash=$null
 if ($Action -eq 'Rollback') {$selectedVideoHash=$oldManifest.backup_video_sha256}
 elseif ($videoRequested) {$selectedVideoHash=$ExpectedVideoSha256}
 elseif ($oldManifest) {$selectedVideoHash=$oldManifest.video_sha256}
 if ($selectedVideoHash) {
  $helperDestination=HelperPath $selectedVideoHash
  if (Test-Path -LiteralPath $helperDestination) {
   if (!$oldManifest -or ($selectedVideoHash -ne $oldManifest.video_sha256 -and $selectedVideoHash -ne $oldManifest.backup_video_sha256)) {throw 'Existing unowned helper preserved; inspect before continuing'}
   $null=VerifiedHelper $selectedVideoHash
  } else {
   if (!$videoRequested -or $Action -eq 'Rollback') {throw 'Verified paired helper is missing'}
   $helperStage=Join-Path $directory ('helper-stage-'+$id+'.exe')
   [IO.File]::Copy($helperSource,$helperStage,$false);PlainFile $helperStage;Arm64 $helperStage
   if ((Hash $helperStage) -ne $selectedVideoHash) {throw 'Staged helper checksum verification failed'}
   [IO.File]::Move($helperStage,$helperDestination);$createdHelper=$helperDestination
  }
 }
 $stage=Join-Path $directory ('agent-stage-'+$id+'.exe')
 [IO.File]::Copy($source,$stage,$false)
 PlainFile $stage;Arm64 $stage
 if ((Hash $stage) -ne $expected) {throw 'Staged checksum verification failed; installation refused'}
 $newManifest=@{schema=1;sha256=$expected;backup=$null;backup_sha256=$null;video_sha256=$selectedVideoHash;backup_video_sha256=$null;transfer=(TransferSelections $oldManifest)}
 if ($oldManifest) {
  # Recheck immediately before replacement; File.Replace retains the actual old bytes.
  $checked=ManagedInstallation
  if ($checked.sha256 -ne $oldManifest.sha256) {throw 'Installation changed during update'}
  $backupName='agent-backup-'+$id+'.exe';$backup=Join-Path $directory $backupName
  [IO.File]::Replace($stage,$destination,$backup)
  $published=$true
  if ((Hash $backup) -ne $oldManifest.sha256) {throw 'Installation changed during replacement; restoring preserved bytes'}
  $newManifest.backup=$backupName;$newManifest.backup_sha256=$oldManifest.sha256;$newManifest.backup_video_sha256=$oldManifest.video_sha256
 } else {
  $stageShortcut=Join-Path ([IO.Path]::GetDirectoryName($shortcut)) ('FreeLane-stage-'+$id+'.lnk')
  $link=(New-Object -ComObject WScript.Shell).CreateShortcut($stageShortcut)
  $link.TargetPath=$destination;$link.WorkingDirectory=$directory;$link.WindowStyle=7
  $link.Description='FreeLane interactive user guest agent';$link.Save()
  [IO.File]::Move($stage,$destination);$published=$true
  [IO.File]::Move($stageShortcut,$shortcut);$installedShortcut=$true
 }
 $stageManifest=Join-Path $directory ('manifest-stage-'+$id+'.json')
 WriteManifestStage $stageManifest $newManifest
 if ($oldManifest) {
  $manifestBackup=Join-Path $directory ('manifest-backup-'+$id+'.json')
  [IO.File]::Replace($stageManifest,$manifestPath,$manifestBackup)
 } else {[IO.File]::Move($stageManifest,$manifestPath)}
 $published=$false;$helperCommitted=$true
 Write-Output "Agent $Action complete. Sign in again to start the per-user agent. No service or elevation was used."
} catch {
 $failure=$_
 if ($published) {
  if ((Test-Path -LiteralPath $destination) -and (Hash $destination) -eq $expected) {
   if ($backup -and (Test-Path -LiteralPath $backup)) {
    $replaced=Join-Path $directory ('agent-recovery-'+[Guid]::NewGuid().ToString('N')+'.exe')
    [IO.File]::Replace($backup,$destination,$replaced)
    if ((Hash $replaced) -eq $expected) {Remove-Item -LiteralPath $replaced}
   } else {Remove-Item -LiteralPath $destination}
   if ($installedShortcut) {Remove-Item -LiteralPath $shortcut}
  } else {throw 'Agent destination changed during failure recovery; files retained for inspection'}
 }
 throw $failure
} finally {
 if ($createdHelper -and !$helperCommitted -and (Test-Path -LiteralPath $createdHelper)) {
  PlainFile $createdHelper
  if ((Hash $createdHelper) -eq $selectedVideoHash) {Remove-Item -LiteralPath $createdHelper}
 }
 foreach ($path in @($stage,$stageManifest,$stageShortcut,$manifestBackup,$helperStage)) {if ($path -and (Test-Path -LiteralPath $path)) {Remove-Item -LiteralPath $path}}
 $lock.Dispose()
}
