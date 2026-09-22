param([switch]$Test, [string]$Rom, [string]$Manifest)
if ($Rom) { $env:HOENN_ROM_PATH = (Resolve-Path -LiteralPath $Rom).Path }
if ($Manifest) { $env:HOENN_MANIFEST_PATH = (Resolve-Path -LiteralPath $Manifest).Path }
$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = 'C:/Program Files/Android/Android Studio/jbr' }
if (-not (Test-Path "$env:JAVA_HOME/bin/javac.exe")) { throw 'Set JAVA_HOME to a JDK 21+ installation.' }
if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = "$env:LOCALAPPDATA/Android/Sdk" }
$sdkManager = "$env:ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager.bat"
if (-not (Test-Path $sdkManager)) { throw 'Install Android SDK command-line tools into ANDROID_HOME/cmdline-tools/latest.' }
& $sdkManager "--sdk_root=$env:ANDROID_HOME" 'platforms;android-37.0' 'build-tools;36.0.0' 'ndk;27.2.12479018' 'cmake;3.22.1' 'platform-tools'
if ($LASTEXITCODE) { throw 'SDK setup failed' }
$nativeSource = Join-Path $repository '.local/mgba'
if (-not (Test-Path "$nativeSource/.git")) {
    & git clone --depth 1 --branch 0.10.5 https://github.com/mgba-emu/mgba.git $nativeSource
    if ($LASTEXITCODE) { throw 'mGBA checkout failed' }
}
$nativeCommit = & git -C $nativeSource rev-parse HEAD
if ($nativeCommit -ne '26b7884bc25a5933960f3cdcd98bac1ae14d42e2') { throw 'Unexpected mGBA source identity' }
if (& git -C $nativeSource status --porcelain) { throw 'mGBA source has local modifications; review before building' }
Set-Content -LiteralPath "$PSScriptRoot/local.properties" -Value ('sdk.dir=' + $env:ANDROID_HOME.Replace('\','/').Replace(':','\:')) -Encoding utf8
$buildTasks = @('assembleDebug')
if ($Test) { $buildTasks += 'testDebugUnitTest' }
& "$PSScriptRoot/gradlew.bat" -p $PSScriptRoot @buildTasks --no-daemon --console=plain
if ($LASTEXITCODE) { throw 'APK build or tests failed' }
Write-Output "APK: $PSScriptRoot/app/build/outputs/apk/debug/app-debug.apk"
