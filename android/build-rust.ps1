param([string]$Sdk = "$env:LOCALAPPDATA/Android/Sdk")
$ErrorActionPreference = 'Stop'
& python "$PSScriptRoot/build-rust.py" --sdk $Sdk
if ($LASTEXITCODE) { throw 'Rust Android build failed' }
