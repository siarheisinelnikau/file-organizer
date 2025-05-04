$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'package_policy.ps1')
$fixtureParent = Join-Path (Split-Path -Parent $PSScriptRoot) 'target/test-fixtures'
$fixtureDirectory = Join-Path $fixtureParent ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $fixtureDirectory -Force | Out-Null
try {
    $source = Join-Path $fixtureDirectory 'built.exe'
    $asset = Join-Path $fixtureDirectory 'FileOrganizer-v0.2.0.exe'
    [IO.File]::WriteAllText($source, 'synthetic original bytes')
    Assert-PackageVersion -Version '0.2.0' -SourcePath $source -AssetPath $asset
    Copy-Item -LiteralPath $source -Destination $asset
    Assert-PackageVersion -Version '0.2.0' -SourcePath $source -AssetPath $asset
    [IO.File]::WriteAllText($source, 'synthetic changed bytes')
    $rejected = $false
    try { Assert-PackageVersion -Version '0.2.0' -SourcePath $source -AssetPath $asset } catch { $rejected = $true }
    if (-not $rejected) { throw 'A changed executable reused an existing version' }
    if ([IO.File]::ReadAllText($asset) -ne 'synthetic original bytes') { throw 'The existing package was modified' }
    Assert-PackageVersion -Version '0.2.1' -SourcePath $source -AssetPath (Join-Path $fixtureDirectory 'FileOrganizer-v0.2.1.exe')
    $rejected = $false
    try { Assert-PackageVersion -Version '0.1.9' -SourcePath $source -AssetPath (Join-Path $fixtureDirectory 'FileOrganizer-v0.1.9.exe') } catch { $rejected = $true }
    if (-not $rejected) { throw 'An older version was accepted' }
    Write-Output 'Package policy: identical rebuild accepted; changed same-version bytes and version regression rejected; new patch accepted.'
} finally {
    # Delete only the exact three fixture files created in this invocation, then the empty directory.
    foreach ($file in 'built.exe', 'FileOrganizer-v0.2.0.exe') {
        $fixtureFile = Join-Path $fixtureDirectory $file
        if (Test-Path -LiteralPath $fixtureFile) { Remove-Item -LiteralPath $fixtureFile }
    }
    Remove-Item -LiteralPath $fixtureDirectory
}
