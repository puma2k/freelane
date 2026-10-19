# Windows guest-agent install and recovery

Download the Windows ARM64 agent ZIP and its SHA256 file from the same trusted FreeLane release. Verify the ZIP checksum before extracting it; the package includes `freelane-guest-agent.exe.sha256` and the lifecycle helper. Run PowerShell as the ordinary interactive Windows user. Driver installation is separate.

For the complete matching agent/video package, use the single setup flow:

```powershell
powershell.exe -NoProfile -File .\setup-windows-agent.ps1
```

It validates both packaged checksums and installs the native ARM64 helper beside the agent using a content-hash filename recorded in `agent-install.json`. Updating preserves the previous agent/helper pair for rollback. Stop the installed agent first; sign in again afterward. Uninstall removes the current managed helper and preserves backups/unrelated files. Modified installed bytes are refused rather than overwritten. Native ARM64 CI builds/extracts this same ZIP, runs helper capture and verifies setup/uninstall.

The advanced commands below can install an agent alone (snapshot fallback), or preserve an existing verified helper during update. To supply a new helper explicitly, add `-VideoHelperPath .\freelane-video-helper.exe -ExpectedVideoSha256 <trusted-helper-sha256>` to Install/Update.

```powershell
$hash = (Get-Content -LiteralPath '.\freelane-guest-agent.exe.sha256').Split()[0]
.\install-windows-agent.ps1 -Action Install -AgentPath '.\freelane-guest-agent.exe' -ExpectedSha256 $hash
# Stop the installed agent in your own user session before updating:
.\install-windows-agent.ps1 -Action Update -AgentPath '.\freelane-guest-agent.exe' -ExpectedSha256 $hash
.\install-windows-agent.ps1 -Action Rollback
.\install-windows-agent.ps1 -Action Uninstall
```

Install/update verifies the ARM64 PE header and SHA256 of a staged copy before publishing it. Installation is per-user in LocalAppData with a Startup shortcut. Sign in again to start it. The helper never elevates, creates a service or kills a process by name. Active agents must be stopped first. It leases a lock, verifies the ownership manifest/current executable/shortcut and refuses changed or unrecognized installations. Legacy install-only copies have no manifest; inspect/back up their files before replacing them through a fresh installation.

Updates retain the old verified binary. An update failure restores it; explicit Rollback restores the recorded backup and retains the replaced version. Uninstall first stages same-filesystem renames and restores managed paths on failure. It preserves backups and unrelated files. A quarantined file that Windows refuses to delete remains for inspection; do not recursively delete a FreeLane folder containing your other files.

Native Windows CI uses a real ARM64 MSVC artifact to check install/update/rollback/uninstall, bad hashes, changed files, backup traversal, locked-manifest update recovery, locked-executable uninstall and unrelated-file preservation. CI is not proof of startup in a normal Windows guest account; that acceptance remains pending. [Evidence](evidence/p2-lifecycle-clipboard.json).

## Remember inbox and outbox selections

The matching updated Windows agent reads only the bounded `agent-install.json` beside its executable at startup. Install without folder parameters leaves both directions disabled. Select existing ordinary directories explicitly:

```powershell
.\install-windows-agent.ps1 -Action Install -AgentPath '.\freelane-guest-agent.exe' -ExpectedSha256 $hash -InboxDirectory 'C:\Users\me\FreeLane Inbox' -OutboxDirectory 'C:\Users\me\FreeLane Outbox'
# Stop the installed agent first. No new download is needed to change selections:
.\install-windows-agent.ps1 -Action Configure -OutboxDirectory 'D:\Guest Exports'
# Explicit empty values revoke a selection; omitted values preserve it:
.\install-windows-agent.ps1 -Action Configure -OutboxDirectory ''
```

`Configure` atomically replaces only the settings manifest, preserving the executable, Startup shortcut and verified rollback binary. Update and Rollback preserve the current selections; rollback never resurrects revoked folder access. Configure and Update still require stopping the installed agent first, followed by restarting it in the user session or signing in again. This is persistent configuration, not automatic process restart or hot reload. Uninstall preserves selected folders and their contents.

Missing folders are never created. Invalid JSON, oversized/nonregular/reparse settings files or malformed stored paths refuse startup rather than enabling a fallback. With the matching preview.21 agent, a selected folder that cannot open at startup disables only that transfer direction; the agent and its other features stay available. Preview.19/20 instead refuse transfer-agent startup. Use the explicit folder check/reconnect flow below after restoring a missing folder; no replacement host/guest folder is inferred. CLI `--transfer-root` / `--export-root` still override their corresponding saved selection for that process. Linux keeps its explicit CLI setup. Older binaries can ignore stored settings, so use the matching updated Windows agent and verify its advertised capabilities. Physical removable drives, Windows ARM guest login and normal-user Startup acceptance remain separate hardware/guest checks.

[Persistent-settings verification](evidence/p2-persisted-settings.json) records the native Windows lifecycle and actual native x64 startup/capability checks for remembered folder choices. Use the matching [preview.19 release](https://github.com/puma2k/freelane/releases/tag/v0.1.0-preview.19) or a newer verified package.

## Recover a startup-unavailable folder

With matching protocol v13 host/agent, click **Check guest folders** in Display. Each direction is **Ready**, **Not selected** or **Unavailable**. After restoring the configured directory, click **Reconnect guest folders**; this retries only roots that failed to open at startup. It never creates a folder, accepts a replacement path or swaps an already opened directory handle. Configuration changes still require agent restart.

CLI: `freelane file-folders vm.toml` inspects state; `freelane file-reconnect vm.toml` explicitly retries. Reconnect refuses while an upload/export session is active; finish/cancel it or allow its idle expiry, then check again. Checks and reconnects never replay a file transfer. Older agents ask for an update and retain their existing copy features.

Ready means a directory handle was opened, not proof of drive identity, current physical availability or successful future file I/O. Live unmount/hotplug and invalidation of already opened handles are not handled by this startup recovery; restarting the agent may be necessary. Native Mac/Windows ARM and physical removable-drive acceptance remain pending.
