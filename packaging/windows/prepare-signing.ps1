# Inno's documented external-signing flow: the first compilation emits the
# unsigned uninstaller template and stops because a signature is required.
# That failed preparation is never a release artifact. The final compile must
# succeed with the verified, signed cache file restored at the exact same path.
param([Parameter(Mandatory=$true)][string]$Version)
$ErrorActionPreference = 'Stop'
$compiler = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (!(Test-Path -LiteralPath $compiler)) { throw 'Inno Setup 6 is required.' }
$cache = Join-Path $PSScriptRoot 'Output\uninstaller-cache'
if ((Test-Path $cache) -and @(Get-ChildItem $cache -File).Count -ne 0) { throw 'Preparation requires an empty uninstaller cache.' }
$log = Join-Path $env:RUNNER_TEMP 'mira-inno-prepare.log'
$errlog = Join-Path $env:RUNNER_TEMP 'mira-inno-prepare-errors.log'
$script = Join-Path $PSScriptRoot 'mira.iss'
$process = Start-Process -FilePath $compiler -ArgumentList @("/DMiraVersion=$Version", '/DSignPath', "`"$script`"") -NoNewWindow -PassThru -RedirectStandardOutput $log -RedirectStandardError $errlog
if (!$process.WaitForExit(60000)) { $process.Kill(); throw 'Uninstaller preparation timed out.' }
$process.Refresh()
$output = (Get-Content $log -Raw) + (Get-Content $errlog -Raw)
Write-Host $output
$templates = @(Get-ChildItem $cache -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'uninst-*' -and $_.Extension -in '.e32','.e64' })
if ($process.ExitCode -eq 0 -or $templates.Count -ne 1 -or $output -notmatch '(?i)signature') {
    throw 'Expected Inno to stop at its signature-required check and emit exactly one uninstaller template.'
}
$stage = Join-Path $PSScriptRoot 'Output\unsigned-components'
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item (Join-Path $PSScriptRoot '..\..\target\release\mira-win.exe') (Join-Path $stage 'mira-win.exe')
Copy-Item $templates[0].FullName (Join-Path $stage 'mira-uninstaller.exe')
@{ uninstallerCachePath = $templates[0].FullName } | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot 'Output\uninstaller-manifest.json')
