# Run llama-server from the installed distribution (dev launcher).
# By default starts llama-server only; pass -WithWebUI to also launch Open WebUI.
[CmdletBinding()]
param(
    [switch]$WithWebUI
)

. "$PSScriptRoot\common.ps1"  # activates VS Dev Shell + ROCm; loads dev $cfg from repo

$resourcesDir = Join-Path $PSScriptRoot "resources"
$configDir    = Join-Path $env:LOCALAPPDATA "llama.cpp\config"
$serverPath   = Join-Path $configDir "server.psd1"
$modelsRoot   = Join-Path $configDir "models"
$webuiPath    = Join-Path $configDir "webui.psd1"

# ── server.psd1 + active model ──────────────────────────────────────
if (-not (Test-Path $serverPath)) {
    & (Join-Path $resourcesDir "config-server.ps1")
    if (-not (Test-Path $serverPath)) { throw "server.psd1 was not created. Aborting." }
}
$srv = Import-PowerShellDataFile -Path $serverPath

if (-not $srv.ActiveModel) {
    & (Join-Path $resourcesDir "config-model.ps1")
    $srv = Import-PowerShellDataFile -Path $serverPath
    if (-not $srv.ActiveModel) { throw "No active model configured. Aborting." }
}
$modelCfgPath = Join-Path $modelsRoot "$($srv.ActiveModel).psd1"
if (-not (Test-Path $modelCfgPath)) {
    & (Join-Path $resourcesDir "config-model.ps1")
    $srv = Import-PowerShellDataFile -Path $serverPath
    $modelCfgPath = Join-Path $modelsRoot "$($srv.ActiveModel).psd1"
    if (-not (Test-Path $modelCfgPath)) { throw "Model config still missing. Aborting." }
}
$mdl = Import-PowerShellDataFile -Path $modelCfgPath

if ($srv.ModelsDir) { $env:LLAMA_CACHE = $srv.ModelsDir }

# ── Locate llama-server (installed distribution preferred) ──────────
$installDir = $null
$regPath = "HKLM:\Software\llama.cpp"
if (Test-Path $regPath) {
    $installDir = (Get-ItemProperty $regPath).InstallDir
}
if (-not $installDir) { $installDir = "${env:ProgramFiles}\llama.cpp" }

$serverExe = Join-Path $installDir "bin\llama-server.exe"
if (-not (Test-Path $serverExe)) {
    # Fall back to the local build dir (dev mode pre-install)
    $serverExe = Join-Path $PSScriptRoot "build\bin\llama-server.exe"
    if (-not (Test-Path $serverExe)) {
        throw "llama-server.exe not found. Build with 02-build.ps1 or install via 03-package.ps1."
    }
}

# ── Build server arguments ──────────────────────────────────────────
$modelArgs = if (Test-Path -LiteralPath $mdl.Model) {
    @("-m", $mdl.Model)
} else {
    @("-hf", $mdl.Model)
}

$hostname = if ($null -ne $srv.Hostname) { $srv.Hostname } else { "localhost" }

$serverArgs = $modelArgs + @(
    "--cache-type-k", $mdl.CacheTypeK
    "--cache-type-v", $mdl.CacheTypeV
    "-np", $mdl.Parallel
    "-ngl", $mdl.GpuLayers
    "--ctx-size", $mdl.CtxSize
    "--port", $srv.Port
    "--host", $hostname
)

if ($mdl.FlashAttn) { $serverArgs += "-fa", "on" }
if ($mdl.Jinja)     { $serverArgs += "--jinja" }
if ($srv.Mlock)     { $serverArgs += "--mlock" }

if ($null -ne $mdl.NCpuMoe) { $serverArgs += "--n-cpu-moe", $mdl.NCpuMoe }

if ($null -ne $mdl.Temp)               { $serverArgs += "--temp", $mdl.Temp }
if ($null -ne $mdl.TopK)               { $serverArgs += "--top-k", $mdl.TopK }
if ($null -ne $mdl.TopP)               { $serverArgs += "--top-p", $mdl.TopP }
if ($null -ne $mdl.RepeatPenalty)      { $serverArgs += "--repeat-penalty", $mdl.RepeatPenalty }
if ($null -ne $mdl.PresencePenalty)    { $serverArgs += "--presence-penalty", $mdl.PresencePenalty }
if ($null -ne $mdl.ChatTemplateKwargs) { $serverArgs += "--chat-template-kwargs", $mdl.ChatTemplateKwargs }

$threads = if ($null -ne $srv.Threads) {
    $srv.Threads
} else {
    $cpuCores = [Environment]::ProcessorCount
    if ($cpuCores -gt 8) { $cpuCores - 2 } else { $cpuCores - 1 }
}
$serverArgs += "-t", $threads

# ── Start Open WebUI if requested and installed ──────────────────────
$webuiJobObj = $null
if ($WithWebUI) {
    $webuiDir = $null
    if (Test-Path $regPath) {
        $webuiDir = (Get-ItemProperty $regPath).WebUIDir
    }
    if (-not $webuiDir) { $webuiDir = "${env:ProgramFiles}\Open WebUI" }
    $webuiExe = Join-Path $webuiDir "Scripts\open-webui.exe"
    if (-not (Test-Path $webuiExe)) {
        Write-Host "Open WebUI not found at $webuiExe — skipping (install via 03-package.ps1)" -ForegroundColor Yellow
    } else {
        if (-not (Test-Path $webuiPath)) {
            & (Join-Path $resourcesDir "config-webui.ps1")
        }
        $wui = if (Test-Path $webuiPath) { Import-PowerShellDataFile -Path $webuiPath } else { @{} }
        $wuiHost = if ($null -ne $wui.Hostname) { $wui.Hostname } else { 'localhost' }
        $wuiPort = if ($null -ne $wui.Port)     { $wui.Port }     else { 3000 }

        Write-Host "Starting Open WebUI on ${wuiHost}:${wuiPort}..." -ForegroundColor Cyan
        # First-run preconfig — seeds the DB; UI edits in Admin Settings persist
        # there and override these on subsequent runs.
        $env:OPENAI_API_BASE_URL = "http://localhost:$($srv.Port)/v1"
        $env:OPENAI_API_KEY      = "none"
        $env:HOST                = $wuiHost
        $env:PYTHONUNBUFFERED    = '1'
        $env:PYTHONIOENCODING    = 'utf-8'
        # Force WebUI data (webui.db, vector_db/, uploads/) to a writable
        # per-user dir. Otherwise Open WebUI falls back inside Program Files
        # and ChromaDB crashes on the read-only install location.
        $webuiDataDir = Join-Path $env:LOCALAPPDATA "llama.cpp\data"
        New-Item -ItemType Directory -Path $webuiDataDir -Force | Out-Null
        $env:DATA_DIR = $webuiDataDir

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
        $webuiProc = Start-Process -FilePath $webuiExe -ArgumentList "serve","--port",$wuiPort `
            -PassThru -WindowStyle Minimized
        $webuiJobObj.AddProcess($webuiProc.Handle)
        $displayHost = if ($wuiHost -eq '0.0.0.0') { 'localhost' } else { $wuiHost }
        Write-Host "Open WebUI: http://${displayHost}:${wuiPort}" -ForegroundColor Green
    }
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
Write-Host "Active model: $($srv.ActiveModel)" -ForegroundColor DarkGray
Write-Host "Starting llama-server on ${hostname}:$($srv.Port)..." -ForegroundColor Cyan
try {
    & $serverExe @serverArgs
} finally {
    Stop-WebUI
}
