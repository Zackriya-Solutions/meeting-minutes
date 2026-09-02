<#
.SYNOPSIS
    Post-build Windows installer branding gate for PulseTalq.

.DESCRIPTION
    Inspects built NSIS setup executables and MSI packages and fails (exit 1)
    if any user-visible metadata still says Meetily or Zackriya, or if the
    product name is not PulseTalq.

    Checks performed:
      * PE version resources (ProductName, CompanyName, FileDescription,
        LegalCopyright) via Get-Item ...VersionInfo on .exe and .msi files.
      * MSI Property table (ProductName, Manufacturer, ProductCode,
        UpgradeCode, ProductVersion) via the WindowsInstaller.Installer COM API.
      * Optional: with -CheckInstalled, the HKCU/HKLM Uninstall registry
        entries (DisplayName, DisplayIcon, Publisher) and the Start menu
        shortcut for an already-installed PulseTalq.

    This script never runs an installer. Install manually first if you want
    the -CheckInstalled pass.

.PARAMETER Path
    One or more paths to .exe (NSIS) or .msi files. When omitted the script
    searches <repo>/target/**/bundle/{nsis,msi} and frontend/src-tauri/target/**/bundle/{nsis,msi}.

.PARAMETER CheckInstalled
    Also verify registry uninstall entries and the Start menu shortcut.

.PARAMETER ExpectedProductName
    Defaults to PulseTalq.

.EXAMPLE
    pwsh scripts/verify-installer-branding.ps1
    pwsh scripts/verify-installer-branding.ps1 -Path .\PulseTalq_0.4.0_x64-setup.exe -CheckInstalled
#>
[CmdletBinding()]
param(
    [string[]] $Path,
    [switch]   $CheckInstalled,
    [string]   $ExpectedProductName = 'PulseTalq',
    [string]   $ExpectedPublisher = 'PolyphronAI',
    [string[]] $ForbiddenTerms = @('Meetily', 'meetily', 'Zackriya', 'zackriya', 'meeting-minutes')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:Rows = New-Object System.Collections.Generic.List[object]

function Add-Result {
    param([string] $Status, [string] $Check, [string] $Detail)
    $script:Rows.Add([pscustomobject]@{ Status = $Status; Check = $Check; Detail = $Detail })
}
function Pass { param($c, $d) Add-Result 'PASS' $c $d }
function Warn { param($c, $d) Add-Result 'WARN' $c $d }
function Fail { param($c, $d) Add-Result 'FAIL' $c $d }

function Test-Forbidden {
    param([string] $Value)
    if ([string]::IsNullOrEmpty($Value)) { return $null }
    foreach ($term in $ForbiddenTerms) {
        if ($Value.IndexOf($term, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) { return $term }
    }
    return $null
}

function Resolve-RepoRoot {
    $scriptDir = Split-Path -Parent $PSCommandPath
    return (Resolve-Path (Join-Path $scriptDir '..')).Path
}

function Find-Artifacts {
    $root = Resolve-RepoRoot
    $targets = @((Join-Path $root 'target'), (Join-Path $root 'frontend/src-tauri/target')) | Where-Object { Test-Path $_ }
    if (-not $targets) { return @() }
    $found = @()
    Get-ChildItem -Path $targets -Recurse -Directory -Filter 'bundle' -ErrorAction SilentlyContinue | ForEach-Object {
        foreach ($sub in 'nsis', 'msi') {
            $dir = Join-Path $_.FullName $sub
            if (Test-Path $dir) {
                $found += Get-ChildItem -Path $dir -File -Include '*.exe', '*.msi' -Recurse -ErrorAction SilentlyContinue
            }
        }
    }
    return $found | Select-Object -ExpandProperty FullName -Unique
}

function Test-VersionInfo {
    param([string] $File)
    $name = Split-Path -Leaf $File
    if ($File -like '*.msi') {
        Warn "$name VersionInfo" 'MSI files carry no PE version resource; see Property table checks'
        return
    }
    $vi = (Get-Item -LiteralPath $File).VersionInfo
    $fields = [ordered]@{
        ProductName     = $vi.ProductName
        CompanyName     = $vi.CompanyName
        FileDescription = $vi.FileDescription
        LegalCopyright  = $vi.LegalCopyright
        OriginalFilename = $vi.OriginalFilename
        InternalName    = $vi.InternalName
    }
    $anyPresent = $false
    foreach ($k in $fields.Keys) {
        $v = $fields[$k]
        if (-not [string]::IsNullOrEmpty($v)) { $anyPresent = $true }
        $hit = Test-Forbidden $v
        $isAttribution = ($k -eq 'LegalCopyright' -and $v -like "*$ExpectedPublisher*" -and $v -notlike '*eetily*' -and $v -like '*Includes work*')
        if ($hit -and $isAttribution) {
            Pass "$name VersionInfo.$k" "upstream MIT attribution kept alongside $ExpectedPublisher"
        } elseif ($hit) {
            Fail "$name VersionInfo.$k" "contains '$hit': $v"
        } elseif ($k -eq 'CompanyName' -and -not [string]::IsNullOrEmpty($v) -and $v -ne $ExpectedPublisher) {
            Fail "$name VersionInfo.CompanyName" "'$v', expected '$ExpectedPublisher'"
        } elseif ($k -eq 'ProductName') {
            if ([string]::IsNullOrEmpty($v)) {
                Fail "$name VersionInfo.ProductName" 'empty'
            } elseif ($v -ne $ExpectedProductName) {
                Fail "$name VersionInfo.ProductName" "'$v', expected '$ExpectedProductName'"
            } else {
                Pass "$name VersionInfo.ProductName" $v
            }
        } elseif (-not [string]::IsNullOrEmpty($v)) {
            Pass "$name VersionInfo.$k" $v
        }
    }
    if (-not $anyPresent) {
        if ($File -like '*.msi') {
            Warn "$name VersionInfo" 'MSI files carry no PE version resource; see Property table checks'
        } else {
            Fail "$name VersionInfo" 'no version resource found'
        }
    }
}

function Test-MsiProperties {
    param([string] $File)
    $name = Split-Path -Leaf $File
    try {
        $installer = New-Object -ComObject WindowsInstaller.Installer
    } catch {
        Fail "$name MSI" "cannot create WindowsInstaller.Installer COM object: $($_.Exception.Message)"
        return
    }
    try {
        # OpenDatabase(path, msiOpenDatabaseModeReadOnly = 0)
        $db = $installer.GetType().InvokeMember('OpenDatabase', 'InvokeMethod', $null, $installer, @($File, 0))
        $view = $db.GetType().InvokeMember('OpenView', 'InvokeMethod', $null, $db, @('SELECT `Property`, `Value` FROM `Property`'))
        $view.GetType().InvokeMember('Execute', 'InvokeMethod', $null, $view, $null) | Out-Null
        $props = @{}
        while ($true) {
            $rec = $view.GetType().InvokeMember('Fetch', 'InvokeMethod', $null, $view, $null)
            if ($null -eq $rec) { break }
            $k = $rec.GetType().InvokeMember('StringData', 'GetProperty', $null, $rec, @(1))
            $v = $rec.GetType().InvokeMember('StringData', 'GetProperty', $null, $rec, @(2))
            $props[$k] = $v
        }
        $view.GetType().InvokeMember('Close', 'InvokeMethod', $null, $view, $null) | Out-Null
    } catch {
        Fail "$name MSI Property table" "read failed: $($_.Exception.Message)"
        return
    } finally {
        if ($installer) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($installer) }
    }

    foreach ($k in 'ProductName', 'Manufacturer', 'ProductCode', 'UpgradeCode', 'ProductVersion', 'ARPHELPLINK', 'ARPURLINFOABOUT') {
        $v = $props[$k]
        if (-not $props.ContainsKey($k)) {
            if ($k -in 'ProductName', 'Manufacturer', 'ProductCode', 'UpgradeCode') { Fail "$name MSI.$k" 'missing' }
            continue
        }
        $hit = Test-Forbidden $v
        if ($hit) { Fail "$name MSI.$k" "contains '$hit': $v"; continue }
        if ($k -eq 'ProductName' -and $v -ne $ExpectedProductName) {
            Fail "$name MSI.ProductName" "'$v', expected '$ExpectedProductName'"
        } elseif ($k -eq 'Manufacturer' -and $v -ne $ExpectedPublisher) {
            Fail "$name MSI.Manufacturer" "'$v', expected '$ExpectedPublisher'"
        } else {
            Pass "$name MSI.$k" $v
        }
    }
    # Guard against accidentally reusing the legacy Meetily upgrade family. Tauri derives
    # UpgradeCode from the bundle identifier, so a changed identifier must change this GUID.
    # Fill in the legacy GUID here once it is read from an old Meetily MSI; until then this is informational.
    $legacyUpgradeCodes = @()
    if ($props['UpgradeCode'] -and ($legacyUpgradeCodes -contains $props['UpgradeCode'])) {
        Fail "$name MSI.UpgradeCode" 'matches legacy Meetily upgrade code; the MSI would upgrade Meetily in place'
    }
}

function Test-Installed {
    $displayFound = $false
    $roots = @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
    )
    foreach ($root in $roots) {
        if (-not (Test-Path $root)) { continue }
        Get-ChildItem $root -ErrorAction SilentlyContinue | ForEach-Object {
            $p = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
            if ($null -eq $p) { return }
            $dn = $p.PSObject.Properties['DisplayName']
            if ($null -eq $dn) { return }
            $dnValue = [string]$dn.Value
            if ($dnValue -eq $ExpectedProductName) {
                $displayFound = $true
                Pass "Uninstall entry ($root)" "DisplayName '$dnValue' at $($_.PSChildName)"
                foreach ($k in 'Publisher', 'DisplayIcon', 'InstallLocation', 'UninstallString') {
                    $prop = $p.PSObject.Properties[$k]
                    if ($null -eq $prop) { continue }
                    $v = [string]$prop.Value
                    $hit = Test-Forbidden $v
                    if ($hit) { Fail "Uninstall.$k" "contains '$hit': $v" } else { Pass "Uninstall.$k" $v }
                }
                $iconProp = $p.PSObject.Properties['DisplayIcon']
                if ($null -ne $iconProp) {
                    $iconPath = ([string]$iconProp.Value).Split(',')[0].Trim('"')
                    if (Test-Path -LiteralPath $iconPath) { Pass 'Uninstall.DisplayIcon path' "exists: $iconPath" }
                    else { Fail 'Uninstall.DisplayIcon path' "missing: $iconPath" }
                } else {
                    Warn 'Uninstall.DisplayIcon' 'not set'
                }
            } elseif (Test-Forbidden $dnValue) {
                Warn "Legacy uninstall entry ($root)" "'$dnValue' still installed (expected: old Meetily is left in place, see docs/installer-verification.md)"
            }
        }
    }
    if (-not $displayFound) { Fail 'Uninstall entry' "no DisplayName '$ExpectedProductName' found in HKCU/HKLM" }

    $shortcuts = @(
        (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$ExpectedProductName.lnk"),
        (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\$ExpectedProductName.lnk"),
        (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$ExpectedProductName\$ExpectedProductName.lnk"),
        (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\$ExpectedProductName\$ExpectedProductName.lnk")
    )
    $hit = $shortcuts | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if ($hit) {
        Pass 'Start menu shortcut' $hit
        try {
            $shell = New-Object -ComObject WScript.Shell
            $lnk = $shell.CreateShortcut($hit)
            if (Test-Path -LiteralPath $lnk.TargetPath) { Pass 'Shortcut target' $lnk.TargetPath } else { Fail 'Shortcut target' "missing: $($lnk.TargetPath)" }
            $t = Test-Forbidden ($lnk.TargetPath + ' ' + $lnk.Description)
            if ($t) { Fail 'Shortcut text' "contains '$t'" }
        } catch { Warn 'Shortcut inspection' $_.Exception.Message }
    } else {
        Fail 'Start menu shortcut' "none of: $($shortcuts -join '; ')"
    }
}

# ---------------------------------------------------------------------------

if (-not $Path -or $Path.Count -eq 0) {
    $Path = @(Find-Artifacts)
    if ($Path.Count -eq 0 -and -not $CheckInstalled) {
        Write-Host 'No installer artifacts found under target/**/bundle/{nsis,msi}. Build first or pass -Path.'
        exit 1
    }
}

foreach ($p in $Path) {
    if (-not (Test-Path -LiteralPath $p)) { Fail (Split-Path -Leaf $p) "file not found: $p"; continue }
    $full = (Resolve-Path -LiteralPath $p).Path
    Test-VersionInfo -File $full
    if ($full -like '*.msi') { Test-MsiProperties -File $full }
}

if ($CheckInstalled) { Test-Installed }

Write-Host ''
Write-Host "PulseTalq installer branding gate ($($Path.Count) artifact(s))"
$script:Rows | Format-Table -AutoSize -Property Status, Check, Detail | Out-String -Width 4096 | Write-Host
$fails = @($script:Rows | Where-Object Status -eq 'FAIL').Count
$warns = @($script:Rows | Where-Object Status -eq 'WARN').Count
Write-Host "$($script:Rows.Count) checks, $fails failed, $warns warnings"
if ($fails -gt 0) { Write-Host 'RESULT: FAIL'; exit 1 }
Write-Host 'RESULT: PASS'
exit 0
