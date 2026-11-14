# Custom action of llama.cpp's MSI (llama.cpp.wxs, RemoveInstanceServices):
# on an uninstall that is not the old half of an upgrade, stop and delete the
# instance services llama.cpp_<id>, which pai-manager creates at run time, so
# Windows Installer does not know them. A script, not the framework's manager:
# the engine does not require the framework, so its manager may be absent.
# xtask package embeds this file as a -EncodedCommand, so it runs under the
# target machine's Windows PowerShell 5.1: keep it 5.1-safe and ASCII-only. It
# takes no arguments.
#
# Deferred, as LocalSystem (deleting a service takes it). Only the services
# named llama.cpp_*; the framework's own services are its MSI's. Never fails
# the uninstall: every error is swallowed, and the exit code is 0.

$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'   # no CLIXML progress records in the MSI log
try {
    $services = @(Get-Service -Name 'llama.cpp_*' -ErrorAction SilentlyContinue)
    foreach ($s in $services) {
        try { Stop-Service -Name $s.Name -Force -ErrorAction SilentlyContinue } catch { }
    }
    foreach ($s in $services) {
        try { & sc.exe delete $s.Name | Out-Null } catch { }
    }
} catch { }
exit 0
