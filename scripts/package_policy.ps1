function Assert-PackageVersion {
    param(
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][string]$SourcePath,
        [Parameter(Mandatory)][string]$AssetPath
    )
    if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Packaging requires a stable major.minor.patch version' }
    $distribution = Split-Path -Parent $AssetPath
    if (Test-Path -LiteralPath $distribution) {
        foreach ($existing in Get-ChildItem -LiteralPath $distribution -Filter 'FileOrganizer-v*.exe' -File) {
            if ($existing.Name -match '^FileOrganizer-v(\d+\.\d+\.\d+)\.exe$' -and [version]$Matches[1] -gt [version]$Version) {
                throw "Version $Version is older than $($existing.Name). Advance Cargo.toml before packaging."
            }
        }
    }
    if (Test-Path -LiteralPath $AssetPath) {
        $existingHash = (Get-FileHash -LiteralPath $AssetPath -Algorithm SHA256).Hash
        $sourceHash = (Get-FileHash -LiteralPath $SourcePath -Algorithm SHA256).Hash
        if ($existingHash -ne $sourceHash) {
            throw "A different $Version executable already exists. Run python scripts/bump_version.py patch (or minor for a feature), update release notes, and package again."
        }
    }
}
