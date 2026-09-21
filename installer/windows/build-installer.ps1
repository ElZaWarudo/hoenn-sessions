[CmdletBinding()]
param(
    [ValidateSet('Prepare', 'SignExecutables', 'Package', 'SignMsi', 'Finalize', 'All')]
    [string] $Phase = 'All',
    [switch] $UnsignedDevelopment,
    [switch] $UnsignedPrivatePilot,
    [string] $OnboardingExe,
    [string] $NoticesFile,
    [string] $OutputRoot = (Join-Path $PSScriptRoot 'out'),
    [string] $ReleaseTrustKeyId,
    [string] $ReleaseTrustPublicKeyHex,
    [string] $ManifestTrustKeyId,
    [string] $ManifestTrustPublicKeyHex,
    [string] $BootstrapperExe,
    [string] $SourceRepository,
    [string] $SourceRevision,
    [string] $CiRunId,
    [string] $CiRunAttempt,
    [string] $AuthenticodeCertificateThumbprint,
    [string] $AuthenticodeTimestampUrl
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$manifestPath = Join-Path $PSScriptRoot 'tool-manifest.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$output = [IO.Path]::GetFullPath($OutputRoot)
$stage = Join-Path $output 'staged'
$wixProject = Join-Path $PSScriptRoot 'HoennSessions.wixproj'

if ($UnsignedDevelopment -and $UnsignedPrivatePilot) {
    throw 'UnsignedDevelopment and UnsignedPrivatePilot are mutually exclusive'
}

function Test-UnsignedBuild {
    return $UnsignedDevelopment -or $UnsignedPrivatePilot
}

$msiName = if ($UnsignedPrivatePilot) {
    'HoennSessions-UNSIGNED-PRIVATE-PILOT.msi'
} elseif ($UnsignedDevelopment) {
    'HoennSessions-UNSIGNED-DEVELOPMENT.msi'
} else {
    'HoennSessions.msi'
}
$msiOutput = Join-Path $output $msiName

function Assert-TrustConfig {
    if ($ReleaseTrustKeyId -notmatch '^[A-Za-z0-9_.-]{1,64}$' -or
        [string]::IsNullOrWhiteSpace($ReleaseTrustPublicKeyHex) -or
        $ReleaseTrustPublicKeyHex -notmatch '^[0-9a-fA-F]{64}$' -or
        $ManifestTrustKeyId -notmatch '^[A-Za-z0-9_.-]{1,64}$' -or
        [string]::IsNullOrWhiteSpace($ManifestTrustPublicKeyHex) -or
        $ManifestTrustPublicKeyHex -notmatch '^[0-9a-fA-F]{64}$') {
        throw 'Release and manifest trust key ids/public keys are required build inputs'
    }
}
function Assert-CommandVersion {
    param(
        [Parameter(Mandatory = $true)] [string] $Name,
        [Parameter(Mandatory = $true)] [string] $Expected,
        [Parameter(Mandatory = $true)] [string[]] $Arguments
    )
    $command = Get-Command $Name -ErrorAction Stop
    $actual = (& $command.Source @Arguments).Trim()
    if ($LASTEXITCODE -ne 0 -or $actual -notmatch [Regex]::Escape($Expected)) {
        throw "$Name version is not pinned to $Expected (actual: $actual)"
    }
    return $command.Source
}

function Assert-InstallerInputs {
    if ([string]::IsNullOrWhiteSpace($OnboardingExe) -or
        -not (Test-Path -LiteralPath $OnboardingExe -PathType Leaf)) {
        throw 'An MSI-owned onboarding executable must be supplied explicitly'
    }
    if ([string]::IsNullOrWhiteSpace($NoticesFile) -or
        -not (Test-Path -LiteralPath $NoticesFile -PathType Leaf)) {
        throw 'A complete third-party notices file must be supplied explicitly'
    }
}

function Assert-Stage {
    $allowed = @(
        'hoenn-sessions-bootstrapper.exe',
        'hoenn-sessions-onboarding.exe',
        'bootstrap-config.private-pilot.json',
        'THIRD_PARTY_NOTICES.txt'
    )
    if (-not (Test-Path -LiteralPath $stage -PathType Container)) {
        throw "Installer staging directory does not exist: $stage"
    }
    $actual = @(Get-ChildItem -LiteralPath $stage -File | ForEach-Object { $_.Name })
    if (@($actual | Where-Object { $_ -notin $allowed }).Count -ne 0 -or
        @($allowed | Where-Object { $_ -notin $actual }).Count -ne 0) {
        throw 'Installer staging directory contains a file outside the explicit allowlist'
    }
}

function Prepare-Installer {
    Assert-TrustConfig
    Assert-InstallerInputs
    $dotnet = Assert-CommandVersion -Name 'dotnet' -Expected $manifest.dotnet_sdk -Arguments @('--version')
    [void](Assert-CommandVersion -Name 'rustc' -Expected $manifest.rust_toolchain -Arguments @('--version'))
    $cargo = Get-Command cargo -ErrorAction Stop

    New-Item -ItemType Directory -Force -Path $output | Out-Null
    if (Test-Path -LiteralPath $stage) {
        Get-ChildItem -LiteralPath $stage -Force | Remove-Item -Recurse -Force
    } else {
        New-Item -ItemType Directory -Force -Path $stage | Out-Null
    }

    $cargoTarget = Join-Path $repoRoot 'target\x86_64-pc-windows-msvc\release\coop-bootstrapper.exe'
    if (-not [string]::IsNullOrWhiteSpace($BootstrapperExe)) {
        $cargoTarget = (Resolve-Path -LiteralPath $BootstrapperExe).Path
    } else {
        $env:HOENN_RELEASE_TRUST_KEY_ID = $ReleaseTrustKeyId
        $env:HOENN_RELEASE_TRUST_KEY_HEX = $ReleaseTrustPublicKeyHex
        $env:HOENN_MANIFEST_TRUST_KEY_ID = $ManifestTrustKeyId
        $env:HOENN_MANIFEST_TRUST_KEY_HEX = $ManifestTrustPublicKeyHex
        Push-Location $repoRoot
        try {
            & $cargo.Source build --locked --release --package coop-bootstrapper --target $manifest.target
            if ($LASTEXITCODE -ne 0) { throw 'Rust bootstrap build failed' }
        } finally {
            Pop-Location
        }
    }
    if (-not (Test-Path -LiteralPath $cargoTarget -PathType Leaf)) {
        throw "Bootstrap output was not produced: $cargoTarget"
    }

    Copy-Item -LiteralPath $cargoTarget -Destination (Join-Path $stage 'hoenn-sessions-bootstrapper.exe') -Force
    Copy-Item -LiteralPath $OnboardingExe -Destination (Join-Path $stage 'hoenn-sessions-onboarding.exe') -Force
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'bootstrap-config.private-pilot.json') -Destination (Join-Path $stage 'bootstrap-config.private-pilot.json') -Force
    Copy-Item -LiteralPath $NoticesFile -Destination (Join-Path $stage 'THIRD_PARTY_NOTICES.txt') -Force
    Assert-Stage

    & $dotnet restore $wixProject --force-evaluate
    if ($LASTEXITCODE -ne 0) { throw 'WiX restore failed' }
}

function Assert-SigningInputs {
    if ([string]::IsNullOrWhiteSpace($AuthenticodeCertificateThumbprint) -or
        $AuthenticodeCertificateThumbprint -notmatch '^[0-9a-fA-F]{40}$' -or
        [string]::IsNullOrWhiteSpace($AuthenticodeTimestampUrl)) {
        throw 'Signing requires a certificate-store thumbprint and timestamp URL'
    }
}

function Sign-Authenticode {
    param([Parameter(Mandatory = $true)] [string] $Path)
    $signtool = Get-Command signtool.exe -ErrorAction Stop
    $certificate = Get-ChildItem -LiteralPath "Cert:\CurrentUser\My\$AuthenticodeCertificateThumbprint" -ErrorAction Stop
    if (-not $certificate.HasPrivateKey) { throw 'Authenticode certificate has no private key' }
    $signArgs = @('sign', '/fd', 'SHA256', '/tr', $AuthenticodeTimestampUrl, '/td', 'SHA256', '/sha1', $AuthenticodeCertificateThumbprint, $Path)
    & $signtool.Source @signArgs
    if ($LASTEXITCODE -ne 0) { throw "Authenticode signing failed: $Path" }
    & $signtool.Source verify '/pa' '/all' $Path
    if ($LASTEXITCODE -ne 0) { throw "Authenticode verification failed: $Path" }
}

function Sign-StagedExecutables {
    Assert-SigningInputs
    Assert-Stage
    Sign-Authenticode -Path (Join-Path $stage 'hoenn-sessions-bootstrapper.exe')
    Sign-Authenticode -Path (Join-Path $stage 'hoenn-sessions-onboarding.exe')
}

function Package-Installer {
    Assert-TrustConfig
    Assert-Stage
    $dotnet = Assert-CommandVersion -Name 'dotnet' -Expected $manifest.dotnet_sdk -Arguments @('--version')
    $wixOutput = Join-Path $output 'wix'
    if (Test-Path -LiteralPath $wixOutput) {
        Remove-Item -LiteralPath $wixOutput -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $wixOutput | Out-Null
    $buildArgs = @(
        $wixProject, '--configuration', 'Release', '--no-restore',
        "-property:BootstrapExe=$(Join-Path $stage 'hoenn-sessions-bootstrapper.exe')",
        "-property:OnboardingExe=$(Join-Path $stage 'hoenn-sessions-onboarding.exe')",
        "-property:ConfigFile=$(Join-Path $stage 'bootstrap-config.private-pilot.json')",
        "-property:NoticesFile=$(Join-Path $stage 'THIRD_PARTY_NOTICES.txt')",
        "-property:OutputPath=$wixOutput\"
    )
    & $dotnet build @buildArgs
    if ($LASTEXITCODE -ne 0) { throw 'WiX MSI build failed' }

    $builtMsi = Get-ChildItem -LiteralPath $wixOutput -Filter '*.msi' -File -Recurse |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($null -eq $builtMsi) { throw 'WiX did not emit an MSI' }
    Get-ChildItem -LiteralPath $output -Filter 'HoennSessions*.msi' -File -ErrorAction SilentlyContinue |
        Remove-Item -Force
    Copy-Item -LiteralPath $builtMsi.FullName -Destination $msiOutput -Force
}

function Sign-Msi {
    Assert-SigningInputs
    if (-not (Test-Path -LiteralPath $msiOutput -PathType Leaf)) {
        throw "MSI does not exist: $msiOutput"
    }
    Sign-Authenticode -Path $msiOutput
}

function Assert-ProvenanceInputs {
    if ($UnsignedDevelopment) { return }
    if ($SourceRepository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' -or
        $SourceRevision -notmatch '^[0-9a-fA-F]{40}$' -or
        $CiRunId -notmatch '^[1-9][0-9]*$' -or
        $CiRunAttempt -notmatch '^[1-9][0-9]*$') {
        throw 'Release provenance requires repository, 40-character revision, CI run id, and CI run attempt'
    }
}

function Assert-FinalSigningState {
    $artifacts = @(
        (Join-Path $stage 'hoenn-sessions-bootstrapper.exe'),
        (Join-Path $stage 'hoenn-sessions-onboarding.exe'),
        $msiOutput
    )
    $expected = if (Test-UnsignedBuild) { 'NotSigned' } else { 'Valid' }
    foreach ($artifact in $artifacts) {
        $status = (Get-AuthenticodeSignature -LiteralPath $artifact).Status.ToString()
        if ($status -ne $expected) {
            throw "Unexpected Authenticode status for $artifact (expected $expected, actual $status)"
        }
    }
}

function Finalize-Installer {
    Assert-TrustConfig
    Assert-ProvenanceInputs
    Assert-Stage
    if (-not (Test-Path -LiteralPath $msiOutput -PathType Leaf)) {
        throw "MSI does not exist: $msiOutput"
    }
    Assert-FinalSigningState
    $hashRecords = @(Get-ChildItem -LiteralPath $stage -File | Sort-Object Name | ForEach-Object {
        [ordered]@{
            name = $_.Name
            size = $_.Length
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    })
    $hashRecords += [ordered]@{
        name = $msiName
        size = (Get-Item -LiteralPath $msiOutput).Length
        sha256 = (Get-FileHash -LiteralPath $msiOutput -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $provenance = [ordered]@{
        product = 'hoenn-sessions'
        target = $manifest.target
        toolchain = [ordered]@{
            dotnet = $manifest.dotnet_sdk
            rustc = $manifest.rust_toolchain
            wix_sdk = $manifest.wix_sdk
        }
        channel = 'private-pilot'
        signing = if ($UnsignedPrivatePilot) {
            'unsigned-private-pilot'
        } elseif ($UnsignedDevelopment) {
            'unsigned-development'
        } else {
            'authenticode-signed'
        }
        distribution_restriction = if ($UnsignedPrivatePilot) { 'private-pilot-only' } else { 'standard-release' }
        unsigned_warning = if ($UnsignedPrivatePilot) {
            'Unknown publisher: verify this MSI checksum against hashes.json from the same authenticated GitHub Actions run before choosing Run anyway.'
        } else { $null }
        source = [ordered]@{
            repository = $SourceRepository
            revision = if ([string]::IsNullOrWhiteSpace($SourceRevision)) { $null } else { $SourceRevision.ToLowerInvariant() }
            ci_run_id = $CiRunId
            ci_run_attempt = $CiRunAttempt
        }
        release_trust_key_id = $ReleaseTrustKeyId
        artifacts = $hashRecords
    }
    $hashRecords | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'hashes.json') -Encoding utf8
    $provenance | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $output 'provenance.json') -Encoding utf8
    Write-Output (ConvertTo-Json -Compress $provenance)
}

switch ($Phase) {
    'Prepare' { Prepare-Installer }
    'SignExecutables' { Sign-StagedExecutables }
    'Package' { Package-Installer }
    'SignMsi' { Sign-Msi }
    'Finalize' { Finalize-Installer }
    'All' {
        Prepare-Installer
        if (-not (Test-UnsignedBuild)) { Sign-StagedExecutables }
        Package-Installer
        if (-not (Test-UnsignedBuild)) { Sign-Msi }
        Finalize-Installer
    }
}
