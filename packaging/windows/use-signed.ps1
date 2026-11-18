# Puts a file SignPath signed in place of the unsigned one, after checking that Windows trusts
# its signature, so a release never ships a file that lost or failed its signing.
#
#   packaging/windows/use-signed.ps1 <signed file> <file it replaces>
param(
  [Parameter(Mandatory)][string]$Signed,
  [Parameter(Mandatory)][string]$Target
)
$ErrorActionPreference = "Stop"
$signature = Get-AuthenticodeSignature -LiteralPath $Signed
if ($signature.Status -ne "Valid") {
  throw "$Signed isn't validly signed: $($signature.Status) $($signature.StatusMessage)"
}
Write-Host "$Signed is signed by $($signature.SignerCertificate.Subject)"
Move-Item -LiteralPath $Signed -Destination $Target -Force
