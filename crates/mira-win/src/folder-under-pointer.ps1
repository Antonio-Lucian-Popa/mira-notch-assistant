param([int]$Px, [int]$Py)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase
$element = [System.Windows.Automation.AutomationElement]::FromPoint([System.Windows.Point]::new($Px, $Py))
if (-not $element) { exit }
# Only Explorer/desktop file items. Never guess a folder from an arbitrary app label.
$process = Get-Process -Id $element.Current.ProcessId -ErrorAction SilentlyContinue
if (-not $process -or $process.ProcessName -ne 'explorer') { exit }
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
for ($depth = 0; $element -and $depth -lt 7; $depth++) {
    if ($element.Current.ControlType -eq [System.Windows.Automation.ControlType]::ListItem) { break }
    $element = $walker.GetParent($element)
}
if (-not $element -or $element.Current.ControlType -ne [System.Windows.Automation.ControlType]::ListItem) { exit }
$name = $element.Current.Name
if ([string]::IsNullOrWhiteSpace($name) -or $name.IndexOfAny([System.IO.Path]::GetInvalidFileNameChars()) -ge 0) { exit }
$shell = New-Object -ComObject Shell.Application
foreach ($window in $shell.Windows()) {
    try {
        $uri = [Uri]$window.LocationURL
        if (-not $uri.IsFile) { continue }
        $candidate = Join-Path $uri.LocalPath $name
        $rect = $window.Document.Folder.Self.Path
        # Explorer can have several windows: match the item under the cursor by its parent window bounds.
        if ($Px -lt $window.Left -or $Px -gt ($window.Left + $window.Width) -or $Py -lt $window.Top -or $Py -gt ($window.Top + $window.Height)) { continue }
        if (Test-Path -LiteralPath $candidate -PathType Container) { [Console]::WriteLine($candidate); exit }
    } catch { }
}
$desktop = Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) $name
if (Test-Path -LiteralPath $desktop -PathType Container) { [Console]::WriteLine($desktop) }
