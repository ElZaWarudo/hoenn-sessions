param([string]$Avd = 'Hoenn_API_36')
$ErrorActionPreference = 'Stop'
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = 'C:/Program Files/Android/Android Studio/jbr' }
if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = "$env:LOCALAPPDATA/Android/Sdk" }
$adb = "$env:ANDROID_HOME/platform-tools/adb.exe"
$emulator = "$env:ANDROID_HOME/emulator/emulator.exe"
$existing = & $emulator -list-avds
if ($existing -notcontains $Avd) {
    & "$env:ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager.bat" "--sdk_root=$env:ANDROID_HOME" 'emulator' 'system-images;android-36;google_apis;x86_64'
    if ($LASTEXITCODE) { throw 'System image installation failed' }
    'no' | & "$env:ANDROID_HOME/cmdline-tools/latest/bin/avdmanager.bat" create avd -n $Avd -k 'system-images;android-36;google_apis;x86_64' -d pixel_6
    if ($LASTEXITCODE) { throw 'AVD creation failed' }
}
$serial = $null
foreach ($line in (& $adb devices)) {
    if ($line -match '^(emulator-\d+)\s+device$') {
        $candidate = $Matches[1]
        $name = & $adb -s $candidate emu avd name
        if ($name -contains $Avd) { $serial = $candidate; break }
    }
}
if (-not $serial) {
    $serial = 'emulator-5554'
    if ((& $adb devices) -match '^emulator-5554\s') { throw 'Port 5554 belongs to another/offline emulator. Wait or close it before retrying.' }
    Start-Process -FilePath $emulator -ArgumentList '-avd',$Avd,'-port','5554','-gpu','software','-no-snapshot','-no-boot-anim','-memory','3072','-cores','4','-dns-server','1.1.1.1,8.8.8.8' -WindowStyle Normal
}
$deadline = (Get-Date).AddMinutes(8)
do {
    $boot = & $adb -s $serial shell getprop sys.boot_completed 2>$null
    if ($boot -eq '1') { break }
    if ((Get-Date) -gt $deadline) { throw 'Android did not finish booting within 8 minutes.' }
    Start-Sleep -Seconds 2
} while ($true)
# This emulator's Wi-Fi DNS failed for application UIDs. Use its validated
# cellular network, retaining normal DNS/TLS verification in Android.
& $adb -s $serial shell svc wifi disable
# Boot completion can precede telephony registration on a cold API 36 AVD.
$phoneDeadline = (Get-Date).AddSeconds(45)
while ((& $adb -s $serial shell service check phone) -notmatch ': found$') {
    if ((Get-Date) -gt $phoneDeadline) { throw 'Android telephony service did not become ready.' }
    Start-Sleep -Seconds 2
}
& $adb -s $serial shell svc data enable
& $adb -s $serial install -r "$PSScriptRoot/app/build/outputs/apk/debug/app-debug.apk"
if ($LASTEXITCODE) { throw 'APK installation failed' }
& $adb -s $serial shell am start -W -n io.hoenn.sessions/.MainActivity
if ($LASTEXITCODE) { throw 'Application launch failed' }
