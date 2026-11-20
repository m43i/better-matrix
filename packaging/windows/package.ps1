# Packages a Windows release build (target\release\better-matrix.exe) into dist\out:
# better-matrix-windows-x86_64-setup.exe, the installer people download, and
# better-matrix-windows-x86_64.zip with the same files, which the self-update downloads.
# Needs Inno Setup 6 (GitHub's Windows runners have it): ISCC on PATH or in its default folder.
#
# With WINDOWS_CERTIFICATE (a base64 .pfx holding a code signing certificate and its key) and
# WINDOWS_CERTIFICATE_PASSWORD, signtool (Windows SDK) signs the executable, the installer and its
# uninstaller, timestamped by WINDOWS_TIMESTAMP_URL (DigiCert's by default). Without them nothing is
# signed here; a release may sign through SignPath instead (ADR 0001).
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path dist\zip, dist\out | Out-Null

$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (!$iscc) { $iscc = Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe" }
if (!(Test-Path -LiteralPath $iscc)) { throw "Inno Setup 6 is missing: install it from https://jrsoftware.org/isdl.php" }
$isccArgs = @("/Q", "/DVersion=$Version")

$added = @()
try {
  if ($env:WINDOWS_CERTIFICATE) {
    $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
      Sort-Object { [version]$_.Directory.Parent.Name } | Select-Object -Last 1
    if (!$signtool) { throw "signtool is missing: install the Windows SDK" }
    # The certificate (and any chain in the .pfx) goes into the user's stores for the run, so no
    # password reaches a command line; what this run added is removed again at the end.
    $certificates = [Security.Cryptography.X509Certificates.X509Certificate2Collection]::new()
    $certificates.Import([Convert]::FromBase64String($env:WINDOWS_CERTIFICATE), "$env:WINDOWS_CERTIFICATE_PASSWORD",
      [Security.Cryptography.X509Certificates.X509KeyStorageFlags]"PersistKeySet, UserKeySet")
    $certificate = $certificates | Where-Object HasPrivateKey | Select-Object -First 1
    if (!$certificate) { throw "WINDOWS_CERTIFICATE holds no certificate with its private key" }
    foreach ($item in $certificates) {
      $name = if ($item.HasPrivateKey) { "My" } else { "CA" }
      if (!(Test-Path "Cert:\CurrentUser\$name\$($item.Thumbprint)")) {
        $store = [Security.Cryptography.X509Certificates.X509Store]::new($name, "CurrentUser")
        $store.Open("ReadWrite")
        $store.Add($item)
        $store.Close()
        $added += "Cert:\CurrentUser\$name\$($item.Thumbprint)"
      }
    }
    $timestamp = if ($env:WINDOWS_TIMESTAMP_URL) { $env:WINDOWS_TIMESTAMP_URL } else { "http://timestamp.digicert.com" }
    $sign = @("sign", "/sha1", $certificate.Thumbprint, "/fd", "SHA256", "/tr", $timestamp, "/td", "SHA256")
    & $signtool.FullName @sign target\release\better-matrix.exe
    if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE" }
    # ISCC signs the installer and the uninstaller inside it with the same command: $q is a quote,
    # $f the file (better-matrix.iss, SignTool).
    $isccArgs += "/DSign", ("/Ssigntool=`$q" + $signtool.FullName + "`$q " + ($sign -join " ") + " `$f")
  }

  Copy-Item target\release\better-matrix.exe, packaging\icons\better-matrix.ico, LICENSE -Destination dist\zip
  Compress-Archive -Path dist\zip\* -DestinationPath dist\out\better-matrix-windows-x86_64.zip -Force

  & $iscc @isccArgs packaging\windows\better-matrix.iss
  if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
} finally {
  foreach ($path in $added) { Remove-Item -LiteralPath $path -DeleteKey:($path -like "*\My\*") }
}
