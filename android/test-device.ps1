param(
    [Parameter(Mandatory)][string]$CredentialFile,
    [Security.SecureString]$Invitation,
    [string]$Serial = 'emulator-5554'
)
$ErrorActionPreference = 'Stop'
if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = "$env:LOCALAPPDATA/Android/Sdk" }
$adb = "$env:ANDROID_HOME/platform-tools/adb.exe"
$credential = Import-Clixml -LiteralPath $CredentialFile
if ($credential -isnot [PSCredential]) { throw 'Expected a Windows DPAPI PSCredential exported with Export-Clixml.' }
$config = @{ username = $credential.UserName; password = $credential.GetNetworkCredential().Password }
if ($Invitation) { $config.invitation_code = [PSCredential]::new('invitation',$Invitation).GetNetworkCredential().Password }
& $adb -s $Serial shell run-as io.hoenn.sessions mkdir -p files
if ($LASTEXITCODE) { throw 'Install the debug APK first.' }
$info = [Diagnostics.ProcessStartInfo]::new($adb)
foreach ($arg in @('-s',$Serial,'shell',"run-as io.hoenn.sessions sh -c 'umask 077; cat > files/device-smoke.json'")) { $info.ArgumentList.Add($arg) }
$info.UseShellExecute = $false
$info.RedirectStandardInput = $true
$info.CreateNoWindow = $true
$process = [Diagnostics.Process]::Start($info)
try { $process.StandardInput.Write(($config | ConvertTo-Json -Compress)) }
finally { $process.StandardInput.Close(); $config.Clear() }
$process.WaitForExit()
if ($process.ExitCode) { throw 'Private test input transfer failed.' }
$report = & $adb -s $Serial shell am instrument -w io.hoenn.sessions/.DeviceSmoke
$report
if ($LASTEXITCODE -or ($report -match 'failure=') -or -not ($report -match 'logout=PASS')) { throw 'Device smoke failed. Check the reported stage/status before retrying a one-use invitation.' }
