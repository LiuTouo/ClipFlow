# Bump ClipFlow version everywhere. Cargo.toml stays the single source of
# truth; this script propagates it and inserts a CHANGELOG skeleton.
# Idempotent: re-running only fills in the parts that are missing.
#
# Usage:  npm run bump -- 0.4.0
#    or:  powershell -ExecutionPolicy Bypass -File scripts/bump-version.ps1 0.4.0
param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Version must be semver (e.g. 0.4.0), got: $Version"
}

# PS 5.1 Get-Content/Set-Content default to ANSI for BOM-less files, which
# corrupts UTF-8 Chinese text. Use .NET IO with explicit UTF-8 (no BOM).
$utf8 = New-Object System.Text.UTF8Encoding($false)

# 1. Cargo.toml — version key is anchored at line start so dependency tables
#    (inline `version = "2"`) cannot match.
$cargoPath = "src-tauri/Cargo.toml"
$cargo = [System.IO.File]::ReadAllText($cargoPath, $utf8)
$prevCargo = [regex]::Match($cargo, '(?m)^version = "(\d+\.\d+\.\d+)"').Groups[1].Value
if (-not $prevCargo) { throw "Could not find package version in $cargoPath" }

if ($prevCargo -ne $Version) {
    $cargo = $cargo -replace '(?m)^version = "\d+\.\d+\.\d+"', "version = `"$Version`""
    [System.IO.File]::WriteAllText($cargoPath, $cargo, $utf8)
    Write-Host "Cargo.toml: $prevCargo -> $Version"

    # 2. package.json + package-lock.json (npm version syncs both).
    npm version $Version --no-git-tag-version | Out-Null
    Write-Host "package.json / package-lock.json: -> $Version"
} else {
    Write-Host "Cargo.toml / package.json already at $Version, skipping"
}

# 3. CHANGELOG.md — insert a skeleton before the first "## [" section and a
#    compare link before the first link reference at the bottom. The compare
#    base is the last RELEASED version (first section in the file), NOT the
#    Cargo version — they differ when a bump was started but never tagged.
$changelogPath = "CHANGELOG.md"
$changelog = [System.IO.File]::ReadAllText($changelogPath, $utf8)

if ($changelog -match [regex]::Escape("## [$Version]")) {
    Write-Host "CHANGELOG.md already has a $Version section, skipping"
} else {
    $prevReleased = [regex]::Match($changelog, '(?m)^## \[(\d+\.\d+\.\d+)\]').Groups[1].Value
    if (-not $prevReleased) { throw "No version section found in $changelogPath" }

    $date = Get-Date -Format "yyyy-MM-dd"
    $skeleton = @"
## [$Version] - $date

### Added

### Changed

### Fixed


"@

    $sectionIdx = $changelog.IndexOf("## [")
    $changelog = $changelog.Insert($sectionIdx, $skeleton)

    $linkMatch = [regex]::Match($changelog, '(?m)^\[\d+\.\d+\.\d+\]:')
    if ($linkMatch.Success) {
        $changelog = $changelog.Insert($linkMatch.Index, "[$Version]: https://github.com/LiuTouo/ClipFlow/compare/v$prevReleased...v$Version`n")
    }
    [System.IO.File]::WriteAllText($changelogPath, $changelog, $utf8)
    Write-Host "CHANGELOG.md: skeleton for $Version inserted (compare base v$prevReleased)"
}

Write-Host ""
Write-Host "Done. Now:"
Write-Host "  1. Fill in the CHANGELOG skeleton"
Write-Host "  2. cargo check --manifest-path src-tauri/Cargo.toml  (syncs Cargo.lock)"
Write-Host "  3. Commit, tag v$Version, push --tags"
