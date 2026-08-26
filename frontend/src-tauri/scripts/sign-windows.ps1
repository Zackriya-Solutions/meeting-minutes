param(
    [Parameter(Mandatory=$true)]
    [string]$FilePath
)

# Fail hard if signing environment is not configured.
# Signing is mandatory when signCommand is set in tauri.conf.json — a silent skip
# would allow an unsigned binary to be distributed and blocked by Windows Defender SmartScreen.
if (-not $env:DIGICERT_KEYPAIR_ALIAS) {
    Write-Warning "DIGICERT_KEYPAIR_ALIAS is not set — skipping signing for local build."
    Write-Warning "CI builds set this variable via the DigiCert KeyLocker workflow step."
    Write-Warning "Distributing this unsigned binary will trigger Windows Defender SmartScreen."
    exit 0
}

Write-Host "Signing: $FilePath"
Write-Host "Using keypair alias: $env:DIGICERT_KEYPAIR_ALIAS"

# Sign with an RFC3161 timestamp so the signature remains valid after the certificate expires.
$signOutput = smctl sign --keypair-alias $env:DIGICERT_KEYPAIR_ALIAS --input $FilePath --timestamp http://timestamp.digicert.com --verbose 2>&1
$signExitCode = $LASTEXITCODE

Write-Host "Sign output: $signOutput"
Write-Host "Sign exit code: $signExitCode"

if ($signExitCode -ne 0) {
    Write-Error "Signing failed with exit code: $signExitCode"
    Write-Error "Output: $signOutput"
    exit $signExitCode
}

# Verify the Authenticode signature was applied correctly.
$sig = Get-AuthenticodeSignature -FilePath $FilePath
if ($sig.Status -ne 'Valid') {
    Write-Error "Signature verification failed after signing"
    Write-Error "Status: $($sig.Status)"
    Write-Error "Message: $($sig.StatusMessage)"
    exit 1
}

# Verify the timestamp is present — without it the signature expires with the cert.
if (-not $sig.TimeStamperCertificate) {
    Write-Error "Timestamp certificate is missing after signing."
    Write-Error "Signatures without timestamps become invalid when the signing certificate expires."
    exit 1
}

Write-Host "Successfully signed: $FilePath"
Write-Host "Signature status: $($sig.Status)"
Write-Host "Timestamp issuer: $($sig.TimeStamperCertificate.Subject)"
