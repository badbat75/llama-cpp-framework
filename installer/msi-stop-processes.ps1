# Custom action of the MSI (llama-cpp-framework.wxs, StopProcesses): stop the
# configurator and llama-server before Windows Installer replaces or removes
# their files. xtask package embeds this file as a -EncodedCommand, so it runs
# under the target machine's Windows PowerShell 5.1: keep it 5.1-safe and
# ASCII-only. It takes no arguments and reads what it needs from the registry.
#
# Immediate, scheduled before InstallValidate: Windows Installer's own
# files-in-use check (the Restart Manager dialog) runs there, so a process
# stopped here never reaches it. Impersonated, which is what it needs: the
# configurator and llama-server run as the desktop user, who can stop them.
#
# The order matters. `llama-cpp-config control stop-and-close` is the GRACEFUL
# leg: it writes the conversation snapshot first (Server tab, Conversation
# snapshots), then stops llama-server, then signals the GUI to close. A plain
# kill would lose that snapshot. Whatever is still alive afterwards is killed.
#
# What is stopped: processes named llama-server / llama-cpp-config whose image
# lies under the installed bin\, plus those whose image path cannot be read
# (another user's, from a non-elevated context). Not by name alone: a process
# of the same name started from a build tree
# (build\windows\llama.cpp-cmake\bin) is not ours to stop.
#
# Never fails the install: every error is swallowed, and the exit code is 0.
# What could not be stopped is left to Windows Installer's files-in-use
# handling.

$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'   # no CLIXML progress records in the MSI log
try {
    $dir = (Get-ItemProperty -LiteralPath 'HKLM:\Software\llama.cpp').InstallDir
    if (-not $dir) { exit 0 }
    $bin = (Join-Path $dir 'bin').TrimEnd('\') + '\'
    $exe = Join-Path $bin 'llama-cpp-config.exe'

    function Get-Ours {
        Get-Process -Name 'llama-server', 'llama-cpp-config' -ErrorAction SilentlyContinue |
            Where-Object { -not $_.Path -or $_.Path.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) }
    }

    if (-not @(Get-Ours).Count) { exit 0 }

    if (Test-Path -LiteralPath $exe) {
        # The snapshot of a long conversation is several GiB of sequential IO:
        # give it minutes, not seconds, before falling back to the kill.
        $p = Start-Process -FilePath $exe -ArgumentList 'control', 'stop-and-close' -WindowStyle Hidden -PassThru
        if ($p -and -not $p.WaitForExit(300000)) { $p | Stop-Process -Force }
        # The GUI exits on the close signal, a moment after the command returns.
        $deadline = (Get-Date).AddSeconds(10)
        while (@(Get-Ours).Count -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 250 }
    }

    Get-Ours | Stop-Process -Force
} catch { }
exit 0
