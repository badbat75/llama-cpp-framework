# Run llama-server from the installed distribution

. "$PSScriptRoot\common.ps1"  # loads $cfg, activates VS Dev Shell + ROCm

$env:LLAMA_CACHE = $cfg.CacheDir

# Look for llama-server in the installed location (Program Files)
$installDir = $null
$regPath = "HKLM:\Software\llama.cpp"
if (Test-Path $regPath) {
    $installDir = (Get-ItemProperty $regPath).InstallDir
}
if (-not $installDir) { $installDir = "${env:ProgramFiles}\llama.cpp" }

$serverExe = Join-Path $installDir "bin\llama-server.exe"
if (-not (Test-Path $serverExe)) {
    throw "llama-server.exe not found at $serverExe. Install llama.cpp first (run 04-package.ps1 then install)."
}

$serverArgs = @(
    "-hf", $cfg.Model
    "--cache-type-k", $cfg.CacheTypeK
    "--cache-type-v", $cfg.CacheTypeV
    "-np", $cfg.Parallel
    "-ngl", $cfg.GpuLayers
    "--ctx-size", $cfg.CtxSize
    "--port", $cfg.Port
)

if ($cfg.FlashAttn) { $serverArgs += "-fa", "on" }
if ($cfg.Jinja)     { $serverArgs += "--jinja" }

# Sampling parameters (only passed when explicitly set in config)
if ($null -ne $cfg.Temp)            { $serverArgs += "--temp", $cfg.Temp }
if ($null -ne $cfg.TopK)            { $serverArgs += "--top-k", $cfg.TopK }
if ($null -ne $cfg.TopP)            { $serverArgs += "--top-p", $cfg.TopP }
if ($null -ne $cfg.PresencePenalty) { $serverArgs += "--presence-penalty", $cfg.PresencePenalty }
if ($null -ne $cfg.ChatTemplateKwargs) { $serverArgs += "--chat-template-kwargs", $cfg.ChatTemplateKwargs }

# CPU threads for offloaded layers: all cores -2 if >8 cores, otherwise all -1
$cpuCores = [Environment]::ProcessorCount
$threads = if ($cpuCores -gt 8) { $cpuCores - 2 } else { $cpuCores - 1 }
$serverArgs += "-t", $threads

# ── Start Open WebUI if installed ────────────────────────────────────
# Look for Open WebUI in its dedicated install directory
$webuiDir = $null
if (Test-Path $regPath) {
    $webuiDir = (Get-ItemProperty $regPath).WebUIDir
}
if (-not $webuiDir) { $webuiDir = "${env:ProgramFiles}\Open WebUI" }
$webuiExe = Join-Path $webuiDir "Scripts\open-webui.exe"
$webuiJobObj = $null
if (Test-Path $webuiExe) {
    Write-Host "Starting Open WebUI on port 3000..." -ForegroundColor Cyan
    $env:OPENAI_API_BASE_URL = "http://localhost:$($cfg.Port)/v1"

    # Use a Windows Job Object to group all child processes so we can kill them all
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class JobObject : IDisposable {
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateJobObject(IntPtr lpJobAttributes, string lpName);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool AssignProcessToJobObject(IntPtr hJob, IntPtr hProcess);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool TerminateJobObject(IntPtr hJob, uint uExitCode);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool CloseHandle(IntPtr hObject);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr hJob, int JobObjectInfoClass, IntPtr lpJobObjectInfo, int cbJobObjectInfoLength);
    private IntPtr handle;
    public JobObject() {
        handle = CreateJobObject(IntPtr.Zero, null);
        // Configure job to kill all processes when handle is closed
        var info = new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
        info.BasicLimitInformation.LimitFlags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        int size = Marshal.SizeOf(info);
        IntPtr ptr = Marshal.AllocHGlobal(size);
        Marshal.StructureToPtr(info, ptr, false);
        SetInformationJobObject(handle, 9, ptr, size); // 9 = JobObjectExtendedLimitInformation
        Marshal.FreeHGlobal(ptr);
    }
    public void AddProcess(IntPtr processHandle) { AssignProcessToJobObject(handle, processHandle); }
    public void Terminate() { if (handle != IntPtr.Zero) TerminateJobObject(handle, 1); }
    public void Dispose() { if (handle != IntPtr.Zero) { CloseHandle(handle); handle = IntPtr.Zero; } }
    [StructLayout(LayoutKind.Sequential)] struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        public long PerProcessUserTimeLimit; public long PerJobUserTimeLimit;
        public uint LimitFlags; public UIntPtr MinimumWorkingSetSize; public UIntPtr MaximumWorkingSetSize;
        public uint ActiveProcessLimit; public UIntPtr Affinity; public uint PriorityClass; public uint SchedulingClass;
    }
    [StructLayout(LayoutKind.Sequential)] struct IO_COUNTERS {
        public ulong ReadOperationCount, WriteOperationCount, OtherOperationCount;
        public ulong ReadTransferCount, WriteTransferCount, OtherTransferCount;
    }
    [StructLayout(LayoutKind.Sequential)] struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation; public IO_COUNTERS IoInfo;
        public UIntPtr ProcessMemoryLimit; public UIntPtr JobMemoryLimit; public UIntPtr PeakProcessMemoryUsed; public UIntPtr PeakJobMemoryUsed;
    }
}
"@ -ErrorAction SilentlyContinue

    $webuiJobObj = New-Object JobObject
    $webuiProc = Start-Process -FilePath $webuiExe -ArgumentList "serve","--port","3000" `
        -PassThru -WindowStyle Minimized
    $webuiJobObj.AddProcess($webuiProc.Handle)
    Write-Host "Open WebUI: http://localhost:3000" -ForegroundColor Green
}

# ── Cleanup function ────────────────────────────────────────────────
function Stop-WebUI {
    if ($webuiJobObj) {
        Write-Host "Stopping Open WebUI..." -ForegroundColor Cyan
        $webuiJobObj.Terminate()
        $webuiJobObj.Dispose()
    }
}

# Register cleanup for Ctrl+C and process exit
Register-EngineEvent PowerShell.Exiting -Action { Stop-WebUI } | Out-Null

# ── Start llama-server (foreground, blocks until exit) ──────────────
Write-Host "Starting llama-server on port $($cfg.Port)..." -ForegroundColor Cyan
try {
    & $serverExe @serverArgs
} finally {
    Stop-WebUI
}
