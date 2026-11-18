# Paths are arguments to -File, never interpolated into PowerShell source.
param([string]$Target, [string]$Staged, [int]$ParentId, [string]$Work, [string]$ErrorFile, [string]$Version)
$ErrorActionPreference = 'Stop'
$backup = Join-Path $Work 'previous.exe'
$replacing = $false
try {
    if (!(Test-Path -LiteralPath $Target -PathType Leaf) -or !(Test-Path -LiteralPath $Staged -PathType Leaf)) {
        throw 'The update executable is missing'
    }
    $parent = Get-Process -Id $ParentId -ErrorAction SilentlyContinue
    New-Item -ItemType File -Path (Join-Path $Work 'ready') | Out-Null
    if ($parent -and !$parent.WaitForExit(120000)) {
        # The app stayed open: leave its install alone.
        try { Set-Content -LiteralPath $ErrorFile -Value 'timeout' } catch {}
        Remove-Item -LiteralPath $Work -Recurse -Force
        exit 1
    }
    $replacing = $true
    # Antivirus scanners can hold the executable briefly after process exit.
    for ($attempt = 0; ; $attempt++) {
        try { Move-Item -LiteralPath $Target -Destination $backup; break }
        catch { if ($attempt -ge 20) { throw }; Start-Sleep -Milliseconds 500 }
    }
    Move-Item -LiteralPath $Staged -Destination $Target
    Start-Process -FilePath $Target -WorkingDirectory (Split-Path -Parent $Target)
    # An install from the installer lists its version in Installed apps: keep it current. The ID
    # is packaging\windows\better-matrix.iss's AppId. A failure here leaves the update in place.
    try {
        $entry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{DB776B2C-C930-4C78-B82D-3BACD4B141C9}_is1'
        if ($Version -and (Test-Path -LiteralPath $entry)) {
            Set-ItemProperty -LiteralPath $entry -Name DisplayVersion -Value $Version
        }
    } catch {}
} catch {
    # Before the parent exits, the running app reports the helper-start failure itself.
    if (!$replacing) { exit 1 }
    # Technical details are logged on next startup, never inserted into a translated notice.
    $detail = $_.Exception.Message
    try { Set-Content -LiteralPath $ErrorFile -Value $detail } catch {}
    if (Test-Path -LiteralPath $backup) {
        Remove-Item -LiteralPath $Target -Force -ErrorAction SilentlyContinue
        # If restoring fails, the backup is retained for recovery.
        Move-Item -LiteralPath $backup -Destination $Target
    }
    Start-Process -FilePath $Target -WorkingDirectory (Split-Path -Parent $Target)
    Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}

Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue
