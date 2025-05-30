$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'package_policy.ps1')
$projectDirectory = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectDirectory
try {
    $manifest = Get-Content -LiteralPath (Join-Path $projectDirectory 'Cargo.toml') -Raw
    $versionMatch = [regex]::Match($manifest, '(?m)^version = "([^"]+)"')
    if (-not $versionMatch.Success) { throw 'Package version was not found' }
    $version = $versionMatch.Groups[1].Value
    $targetTriple = 'x86_64-pc-windows-msvc'
    $env:CARGO_BUILD_JOBS = '2'
    $env:CARGO_TARGET_DIR = Join-Path $projectDirectory 'target'
    cargo build --release --locked --target $targetTriple --jobs 2
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $distribution = Join-Path $projectDirectory 'dist'
    New-Item -ItemType Directory -Path $distribution -Force | Out-Null
    $assetName = "FileOrganizer-v$version.exe"
    $asset = Join-Path $distribution $assetName
    $builtAsset = Join-Path $projectDirectory "target\$targetTriple\release\file-organizer.exe"
    python (Join-Path $PSScriptRoot 'inspect_executable.py') $builtAsset --expect-version $version --icon (Join-Path $projectDirectory 'assets/FileOrganizer.ico') | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Executable icon/version validation failed' }
    Assert-PackageVersion -Version $version -SourcePath $builtAsset -AssetPath $asset
    Copy-Item -LiteralPath $builtAsset -Destination $asset
    $digest = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $distribution 'SHA256SUMS.txt'), "$digest  $assetName`n", [Text.UTF8Encoding]::new($false))
    $licenseDirectory = Join-Path $distribution 'licenses'
    New-Item -ItemType Directory -Path $licenseDirectory -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectDirectory '../rust-auto-updator/LICENSE') -Destination (Join-Path $licenseDirectory 'RustAutoUpdator-MIT.txt') -Force
    Write-Output "Packaged $asset"
    Write-Output "SHA256: $digest"
} finally {
    Pop-Location
}
