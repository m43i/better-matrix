# Packages a Windows release build (target\release\better-matrix.exe) into dist\out:
# better-matrix-windows-x86_64-setup.exe, the installer people download, and
# better-matrix-windows-x86_64.zip with the same files, which the self-update downloads.
# Needs Inno Setup 6 (GitHub's Windows runners have it): ISCC on PATH or in its default folder.
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path dist\zip, dist\out | Out-Null
Copy-Item target\release\better-matrix.exe, packaging\icons\better-matrix.ico, LICENSE -Destination dist\zip
Compress-Archive -Path dist\zip\* -DestinationPath dist\out\better-matrix-windows-x86_64.zip -Force

$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (!$iscc) { $iscc = Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe" }
if (!(Test-Path -LiteralPath $iscc)) { throw "Inno Setup 6 is missing: install it from https://jrsoftware.org/isdl.php" }
& $iscc /Q "/DVersion=$Version" packaging\windows\better-matrix.iss
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
