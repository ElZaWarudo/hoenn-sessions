param([string]$Sdk = "$env:LOCALAPPDATA/Android/Sdk")
$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
$mgba = Join-Path $repository '.local/mgba'
$mgbaHead = & git -C $mgba rev-parse HEAD
if ($LASTEXITCODE -or $mgbaHead -ne '26b7884bc25a5933960f3cdcd98bac1ae14d42e2') { throw 'Unexpected mGBA source identity.' }
$mgbaChanges = & git -C $mgba status --porcelain
if ($LASTEXITCODE -or $mgbaChanges) { throw 'mGBA sources must match the pinned clean checkout.' }
$cargo = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
$rustup = Join-Path $env:USERPROFILE '.cargo/bin/rustup.exe'
if (-not (Test-Path $cargo)) { throw 'Install Rust with rustup before building Android.' }
$llvm = Join-Path $Sdk 'ndk/27.2.12479018/toolchains/llvm/prebuilt/windows-x86_64/bin'
if (-not (Test-Path "$llvm/clang.exe")) { throw 'Android NDK 27.2.12479018 is required.' }
$msys = if ($env:MSYS2_ROOT) { $env:MSYS2_ROOT } else { "$env:USERPROFILE/.hoenn-tools/msys64" }
$env:PATH = "$msys/ucrt64/bin;$env:USERPROFILE/.cargo/bin;$env:PATH"
$targets = @(
    @{ Triple='x86_64-linux-android'; Abi='x86_64' },
    @{ Triple='aarch64-linux-android'; Abi='arm64-v8a' }
)
Push-Location $repository
try {
    foreach ($target in $targets) {
        $triple = $target.Triple
        & $rustup target add $triple
        if ($LASTEXITCODE) { throw "Rust target installation failed: $triple" }
        $lower = $triple.Replace('-','_')
        $upper = $lower.ToUpperInvariant()
        Set-Item "env:CARGO_TARGET_${upper}_LINKER" "$llvm/clang.exe"
        Set-Item "env:CARGO_TARGET_${upper}_RUSTFLAGS" "-C link-arg=--target=${triple}28 -C link-arg=-Wl,-z,max-page-size=16384"
        Set-Item "env:CC_$lower" "$llvm/clang.exe"
        Set-Item "env:CFLAGS_$lower" "--target=${triple}28"
        Set-Item "env:AR_$lower" "$llvm/llvm-ar.exe"
        & $cargo build -p coop-android --target $triple --release --locked
        if ($LASTEXITCODE) { throw "Rust Android compilation failed: $triple" }
        $output = Join-Path $PSScriptRoot "app/build/generated/rustJniLibs/$($target.Abi)"
        New-Item -ItemType Directory -Force $output | Out-Null
        Copy-Item -LiteralPath "target/$triple/release/libcoop_android.so" -Destination $output
    }
} finally { Pop-Location }
