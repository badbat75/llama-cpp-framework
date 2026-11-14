# Custom action of llama.cpp's MSI (llama.cpp.wxs, StopInstances): before a
# repair, the old half of an upgrade or the removal a downgrade makes
# (KEEPINSTANCES=1) replaces or removes llama.cpp's files, record the instance
# services llama.cpp_<id> that are running, then stop them, so that
# msi-start-instances.ps1 starts the same ones once the files are in place.
# The services themselves stay: only a real uninstall deletes them
# (msi-remove-instances.ps1). xtask package embeds this file as a
# -EncodedCommand, so it runs under the target machine's Windows PowerShell
# 5.1: keep it 5.1-safe and ASCII-only. It takes no arguments.
#
# Deferred, as LocalSystem (stopping a service takes it). Only the services
# named llama.cpp_*; the framework's own services are its MSI's. The record
# is written only when an instance runs, so a record an earlier run left (an
# install that failed after it) is not lost. Never fails the installation:
# every error is swallowed, and the exit code is 0.

$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'   # no CLIXML progress records in the MSI log
try {
    $record = Join-Path $env:ProgramData 'llama.cpp\instances.running'
    $running = @(Get-Service -Name 'llama.cpp_*' -ErrorAction SilentlyContinue |
        Where-Object { $_.Status -eq 'Running' })
    if ($running.Count -gt 0) {
        Set-Content -Path $record -Value @($running | ForEach-Object { $_.Name }) -Encoding ASCII
    }
    foreach ($s in $running) {
        try { Stop-Service -Name $s.Name -Force -ErrorAction SilentlyContinue } catch { }
    }
} catch { }
exit 0
