[CmdletBinding()]
param(
    [string] $MsiPath,
    [string] $HashesPath,
    [string] $ProvenancePath,
    [ValidateSet('authenticode', 'unsigned-private-pilot')]
    [string] $ExpectedSigningMode,
    [string] $ExpectedSourceRepository,
    [string] $ExpectedSourceRevision,
    [string] $ExpectedCiRunId,
    [string] $ExpectedCiRunAttempt,
    [switch] $StaticOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$windowsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$packagePath = Join-Path $windowsRoot 'Package.wxs'
$projectPath = Join-Path $windowsRoot 'HoennSessions.wixproj'
$buildPath = Join-Path $windowsRoot 'build-installer.ps1'
$configPath = Join-Path $windowsRoot 'bootstrap-config.private-pilot.json'
$manifestPath = Join-Path $windowsRoot 'tool-manifest.json'
$workflowPath = (Resolve-Path (Join-Path $windowsRoot '..\..\.github\workflows\deploy.yml')).Path
$package = Get-Content -LiteralPath $packagePath -Raw
$project = Get-Content -LiteralPath $projectPath -Raw
$build = Get-Content -LiteralPath $buildPath -Raw
$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$workflow = Get-Content -LiteralPath $workflowPath -Raw

if ($package -notmatch 'Scope="perUser"') {
    throw 'WiX package must be per-user'
}
if ($package -notmatch 'UpgradeCode="123F488C-39CB-454D-B12A-A7A4D591C88C"') {
    throw 'WiX UpgradeCode changed from the stable product identity'
}
foreach ($required in @(
        'StandardDirectory Id="LocalAppDataFolder"',
        'Name="Programs"',
        'Name="Hoenn Sessions"',
        'Id="StableBootstrap"',
        'Id="OnboardingFallback"',
        'Id="PrivatePilotConfig"',
        'Id="ThirdPartyNotices"',
        'Id="StartMenuShortcut"',
        'hoenn-sessions-bootstrapper.exe',
        'hoenn-sessions-onboarding.exe',
        'bootstrap-config.private-pilot.json',
        'THIRD_PARTY_NOTICES.txt')) {
    if ($package -notmatch [Regex]::Escape($required)) {
        throw "WiX package is missing required invariant: $required"
    }
}
if ($package -match '(?i)(RemoveFile|RemoveFolder[^>]*(runtime|state|recovery)|Credential Manager|CredentialManager)') {
    throw 'WiX package must not remove mutable runtime, state, recovery, or credential data'
}
if ($project -notmatch 'WixToolset\.Sdk/6\.0\.2') {
    throw 'WiX SDK is not pinned to 6.0.2'
}
if ($build -notmatch 'dotnet restore' -or $build -notmatch '--no-restore') {
    throw 'Installer phases must restore during prepare and package with --no-restore'
}
foreach ($required in @(
        'HOENN_RELEASE_TRUST_KEY_ID',
        'HOENN_RELEASE_TRUST_KEY_HEX',
        'HOENN_MANIFEST_TRUST_KEY_ID',
        'HOENN_MANIFEST_TRUST_KEY_HEX',
        'AuthenticodeCertificateThumbprint',
        'Cert:\CurrentUser\My',
        "'/sha1'")) {
    if ($build -notmatch [Regex]::Escape($required)) {
        throw "Installer build is missing secure configuration invariant: $required"
    }
}
foreach ($required in @(
        '[switch] $UnsignedPrivatePilot',
        'unsigned-private-pilot',
        'HoennSessions-UNSIGNED-PRIVATE-PILOT.msi',
        'private-pilot-only',
        'CiRunAttempt',
        'Assert-FinalSigningState',
        "channel = 'private-pilot'")) {
    if ($build -notmatch [Regex]::Escape($required)) {
        throw "Installer build is missing unsigned private-pilot invariant: $required"
    }
}
if ($build -match '(?i)HOENN_SIGNING_PASSWORD|AuthenticodeCertificatePath|\.pfx' -or $build -match '(?<![A-Za-z])/p(?![A-Za-z])') {
    throw 'Installer build must not pass signing passwords or PFX paths to signtool'
}
if ($build -match 'PersistKeySet') {
    throw 'Installer signing must not use PersistKeySet'
}
foreach ($phase in @('Prepare', 'SignExecutables', 'Package', 'SignMsi', 'Finalize')) {
    if ($workflow -notmatch [Regex]::Escape("-Phase $phase") -and
        $workflow -notmatch [Regex]::Escape("Phase = '$phase'")) {
        throw "Workflow is missing installer phase: $phase"
    }
}
$secretStepPattern = '(?s)- name: Sign staged installer executables.*?(?=\r?\n      - name:)|- name: Sign packaged MSI.*?(?=\r?\n      - name:)'
$secretSteps = [Regex]::Matches($workflow, $secretStepPattern)
if ($secretSteps.Count -ne 2) {
    throw 'Workflow must have exactly two narrow secret-only signing steps'
}
foreach ($match in $secretSteps) {
    if ($match.Value -notmatch [Regex]::Escape("if: env.WINDOWS_INSTALLER_SIGNING_MODE == 'authenticode'")) {
        throw 'Each Authenticode secret step must be conditional on exact authenticode mode'
    }
    if ($match.Value -match '(?i)cargo|dotnet|msbuild|wix|restore|uses:|upload-artifact|checkout|rust-toolchain|setup-dotnet') {
        throw 'Signing steps must not run build tools, restore, checkout, actions, or artifact upload'
    }
}
$workflowWithoutSigningSteps = $workflow
foreach ($match in $secretSteps) {
    $workflowWithoutSigningSteps = $workflowWithoutSigningSteps.Replace($match.Value, '')
}
if ($workflowWithoutSigningSteps -match 'secrets\.AUTHENTICODE_(CERT_B64|PASSWORD)') {
    throw 'Authenticode secrets may appear only in the two conditional signing steps'
}
if ([Regex]::Matches($workflow, 'needs: \[validate, release-key-gate\]').Count -ne 2) {
    throw 'Installer publication and runtime release must both require the release-key gate'
}
foreach ($required in @(
        'WINDOWS_INSTALLER_SIGNING_MODE: unsigned-private-pilot',
        'Validate installer signing policy',
        '$arguments.UnsignedPrivatePilot = $true',
        'Verify finalized installer before upload',
        '-HashesPath installer/windows/out/hashes.json',
        '-ExpectedSourceRevision $env:HOENN_SOURCE_REVISION',
        'HoennSessions-UNSIGNED-PRIVATE-PILOT.msi',
        'HOENN_CI_RUN_ATTEMPT',
        'hoenn-private-pilot-installer-${{ env.WINDOWS_INSTALLER_SIGNING_MODE }}')) {
    if ($workflow -notmatch [Regex]::Escape($required)) {
        throw "Workflow is missing explicit unsigned private-pilot policy: $required"
    }
}
if ($workflow -match 'PersistKeySet') {
    throw 'Workflow must not persist Authenticode private keys'
}
if ($manifest.wix_sdk -ne '6.0.2') {
    throw 'Tool manifest WiX pin changed'
}
if ($config.channel -ne 'private-pilot' -or
    $config.bootstrap_mode -ne 'msi-owned' -or
    -not $config.mutable_runtime_is_outside_install_root) {
    throw 'Private-pilot bootstrap configuration invariants are not explicit'
}

if ($StaticOnly) {
    Write-Output (ConvertTo-Json -Compress ([ordered]@{
        static = $true
        per_user = $true
        upgrade_code = '123F488C-39CB-454D-B12A-A7A4D591C88C'
        preserves_mutable_data = $true
    }))
    exit 0
}

if ([string]::IsNullOrWhiteSpace($MsiPath)) {
    throw 'MsiPath is required unless -StaticOnly is supplied'
}
if ([string]::IsNullOrWhiteSpace($ProvenancePath)) {
    throw 'ProvenancePath is required unless -StaticOnly is supplied'
}
if ([string]::IsNullOrWhiteSpace($HashesPath)) {
    throw 'HashesPath is required unless -StaticOnly is supplied'
}
if ([string]::IsNullOrWhiteSpace($ExpectedSigningMode)) {
    throw 'ExpectedSigningMode is required unless -StaticOnly is supplied'
}
if ([string]::IsNullOrWhiteSpace($ExpectedSourceRepository) -or
    [string]::IsNullOrWhiteSpace($ExpectedSourceRevision) -or
    [string]::IsNullOrWhiteSpace($ExpectedCiRunId) -or
    [string]::IsNullOrWhiteSpace($ExpectedCiRunAttempt)) {
    throw 'Expected source repository, revision, CI run id, and CI run attempt are required'
}
if (-not (Test-Path -LiteralPath $MsiPath -PathType Leaf)) {
    throw "MSI does not exist: $MsiPath"
}
if (-not (Test-Path -LiteralPath $ProvenancePath -PathType Leaf)) {
    throw "Provenance does not exist: $ProvenancePath"
}
if (-not (Test-Path -LiteralPath $HashesPath -PathType Leaf)) {
    throw "Hashes file does not exist: $HashesPath"
}

$hash = Get-FileHash -LiteralPath $MsiPath -Algorithm SHA256
if ($hash.Hash.Length -ne 64) {
    throw "MSI hash was not produced"
}
$provenance = Get-Content -LiteralPath $ProvenancePath -Raw | ConvertFrom-Json
$hashRecords = @(Get-Content -LiteralPath $HashesPath -Raw | ConvertFrom-Json)
$expectedProvenanceSigning = if ($ExpectedSigningMode -eq 'authenticode') {
    'authenticode-signed'
} else {
    'unsigned-private-pilot'
}
$expectedSignatureStatus = if ($ExpectedSigningMode -eq 'authenticode') { 'Valid' } else { 'NotSigned' }
$actualSignatureStatus = (Get-AuthenticodeSignature -LiteralPath $MsiPath).Status.ToString()
$msiName = Split-Path -Leaf $MsiPath
$msiRecord = @($provenance.artifacts | Where-Object { $_.name -eq $msiName })
$msiHashRecord = @($hashRecords | Where-Object { $_.name -eq $msiName })
if ($provenance.signing -ne $expectedProvenanceSigning -or
    $provenance.channel -ne 'private-pilot' -or
    $actualSignatureStatus -ne $expectedSignatureStatus -or
    $msiRecord.Count -ne 1 -or
    $msiHashRecord.Count -ne 1 -or
    $msiHashRecord[0].sha256 -ne $hash.Hash.ToLowerInvariant() -or
    $msiRecord[0].sha256 -ne $hash.Hash.ToLowerInvariant()) {
    throw 'Finalized MSI signing mode, filename, or checksum does not match provenance'
}
if ($hashRecords.Count -ne @($provenance.artifacts).Count) {
    throw 'hashes.json and provenance.json contain different artifact inventories'
}
foreach ($hashRecord in $hashRecords) {
    $provenanceRecord = @($provenance.artifacts | Where-Object { $_.name -eq $hashRecord.name })
    if ($provenanceRecord.Count -ne 1 -or
        $provenanceRecord[0].sha256 -ne $hashRecord.sha256 -or
        $provenanceRecord[0].size -ne $hashRecord.size) {
        throw "hashes.json and provenance.json disagree for $($hashRecord.name)"
    }
}
if ($ExpectedSigningMode -eq 'unsigned-private-pilot' -and
    ($msiName -ne 'HoennSessions-UNSIGNED-PRIVATE-PILOT.msi' -or
     $provenance.distribution_restriction -ne 'private-pilot-only' -or
     [string]::IsNullOrWhiteSpace($provenance.unsigned_warning))) {
    throw 'Unsigned private-pilot MSI must retain its conspicuous filename and distribution warning'
}
if ($provenance.source.repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' -or
    $provenance.source.revision -notmatch '^[0-9a-f]{40}$' -or
    "$($provenance.source.ci_run_id)" -notmatch '^[1-9][0-9]*$' -or
    "$($provenance.source.ci_run_attempt)" -notmatch '^[1-9][0-9]*$') {
    throw 'Provenance must identify the exact source repository, revision, run, and attempt'
}
if ($provenance.source.repository -cne $ExpectedSourceRepository -or
    $provenance.source.revision -cne $ExpectedSourceRevision.ToLowerInvariant() -or
    "$($provenance.source.ci_run_id)" -cne $ExpectedCiRunId -or
    "$($provenance.source.ci_run_attempt)" -cne $ExpectedCiRunAttempt) {
    throw 'Provenance source identity does not equal the current GitHub Actions run'
}

$allowedMutableRoots = @(
    [IO.Path]::Combine($env:LOCALAPPDATA, 'Hoenn Sessions', 'runtime'),
    [IO.Path]::Combine($env:LOCALAPPDATA, 'Hoenn Sessions', 'state'),
    [IO.Path]::Combine($env:LOCALAPPDATA, 'Hoenn Sessions', 'recovery')
)
foreach ($root in $allowedMutableRoots) {
    if ([IO.Path]::GetFullPath($root) -eq [IO.Path]::GetFullPath($env:LOCALAPPDATA)) {
        throw 'Mutable data root escaped LocalAppData'
    }
}

Write-Output (ConvertTo-Json -Compress ([ordered]@{
    msi = [IO.Path]::GetFullPath($MsiPath)
    sha256 = $hash.Hash.ToLowerInvariant()
    signing = $expectedProvenanceSigning
    source_revision = $provenance.source.revision
    preserves_mutable_data = $true
    static = $true
}))
