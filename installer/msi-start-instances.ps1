# Custom action of llama.cpp's MSI (llama.cpp.wxs, StartInstances): once
# llama.cpp's files are in place (a first install, a repair, the new half of
# an upgrade), start again the instance services that msi-stop-instances.ps1
# recorded as running, then delete the record. Nothing recorded, nothing
# started. xtask package embeds this file as a -EncodedCommand, so it runs
# under the target machine's Windows PowerShell 5.1: keep it 5.1-safe and
# ASCII-only. It takes no arguments.
#
# Deferred, as LocalSystem. Only names starting with llama.cpp_ are started,
# whatever else the file holds. Never fails the installation: every error is
# swallowed, and the exit code is 0.

$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'   # no CLIXML progress records in the MSI log
try {
    $record = Join-Path $env:ProgramData 'llama.cpp\instances.running'
    if (Test-Path -LiteralPath $record) {
        $names = @(Get-Content -LiteralPath $record -ErrorAction SilentlyContinue |
            ForEach-Object { $_.Trim() } |
            Where-Object { $_.StartsWith('llama.cpp_') })
        foreach ($n in $names) {
            try { Start-Service -Name $n -ErrorAction SilentlyContinue } catch { }
        }
        Remove-Item -LiteralPath $record -Force -ErrorAction SilentlyContinue
    }
} catch { }
exit 0
