param(
    [Parameter(Mandatory=$true)][string[]]$Paths,
    [Parameter(Mandatory=$true)][string]$ExpectedSubject
)
$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($ExpectedSubject)) { throw 'Expected publisher subject must be configured.' }
foreach ($path in $Paths) {
    if (!(Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing signed file: $path" }
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    if ($signature.Status -ne 'Valid') { throw "Invalid Authenticode signature: $path ($($signature.Status))" }
    if ($signature.SignerCertificate.Subject -ne $ExpectedSubject) { throw "Unexpected publisher on $path" }
    if ($null -eq $signature.TimeStamperCertificate) { throw "Missing trusted timestamp on $path" }
    Write-Host "Verified signed and timestamped file: $path"
}
