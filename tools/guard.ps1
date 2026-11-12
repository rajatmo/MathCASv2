# guard.ps1 (tools/guard.ps1): run one program under guards, output to a file. Every heavy run (benchmarks, full test
# suites, measurements) goes through it, with progress where the caller controls the loop.
#   -MemMB      hard memory cap for the process (Windows job object; allocations past it fail)
#   -TimeoutSec wall-clock limit; on expiry the whole process tree is killed
# Prints one status line: "ok <secs>s peak <MB>MB", "exit <code> ...", "timeout ..." or "memory ..." .
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $OutFile,
    [Parameter(Mandatory)] [string] $WorkDir,
    [int] $MemMB = 4096,
    [int] $TimeoutSec = 900,
    # program arguments, comma-separated (an argument may contain spaces, not commas)
    [string] $ArgCsv = ""
)
$ArgList = @($ArgCsv -split ',' | Where-Object { $_ -ne '' })
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class Job {
    [StructLayout(LayoutKind.Sequential)] struct BASIC { public long a, b; public uint LimitFlags; public UIntPtr c, d; public uint e; public UIntPtr f; public uint g, h; }
    [StructLayout(LayoutKind.Sequential)] struct IO { public ulong a, b, c, d, e, f; }
    [StructLayout(LayoutKind.Sequential)] struct EXT { public BASIC Basic; public IO Io; public UIntPtr ProcessMemoryLimit, JobMemoryLimit, PeakProcessMemoryUsed, PeakJobMemoryUsed; }
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateJobObject(IntPtr a, string n);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr j, int c, ref EXT i, uint l);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool QueryInformationJobObject(IntPtr j, int c, ref EXT i, uint l, IntPtr r);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool AssignProcessToJobObject(IntPtr j, IntPtr p);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool TerminateJobObject(IntPtr j, uint code);
    public static void Kill(IntPtr j) { TerminateJobObject(j, 124); }
    public static IntPtr Create(ulong bytes) {
        IntPtr j = CreateJobObject(IntPtr.Zero, null);
        EXT e = new EXT();
        e.Basic.LimitFlags = 0x100 | 0x2000; // process memory limit, kill on job close
        e.ProcessMemoryLimit = new UIntPtr(bytes);
        if (!SetInformationJobObject(j, 9, ref e, (uint)Marshal.SizeOf(e))) throw new Exception("SetInformationJobObject failed");
        return j;
    }
    public static void Assign(IntPtr j, IntPtr p) { if (!AssignProcessToJobObject(j, p)) throw new Exception("AssignProcessToJobObject failed"); }
    public static ulong Peak(IntPtr j) { EXT e = new EXT(); QueryInformationJobObject(j, 9, ref e, (uint)Marshal.SizeOf(e), IntPtr.Zero); return e.PeakProcessMemoryUsed.ToUInt64(); }
}
"@
$job = [Job]::Create([uint64]$MemMB * 1MB)
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Exe
$psi.WorkingDirectory = $WorkDir
$psi.Arguments = ($ArgList | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' '
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$out = [System.IO.StreamWriter]::new($OutFile, $false, [System.Text.UTF8Encoding]::new($false))
$p = New-Object System.Diagnostics.Process
$p.StartInfo = $psi
$handler = { if ($EventArgs.Data -ne $null) { $Event.MessageData.WriteLine($EventArgs.Data); $Event.MessageData.Flush() } }
$e1 = Register-ObjectEvent -InputObject $p -EventName OutputDataReceived -Action $handler -MessageData $out
$e2 = Register-ObjectEvent -InputObject $p -EventName ErrorDataReceived -Action $handler -MessageData $out
$t0 = Get-Date
[void]$p.Start()
[Job]::Assign($job, $p.Handle)
$p.BeginOutputReadLine(); $p.BeginErrorReadLine()
$done = $p.WaitForExit($TimeoutSec * 1000)
if (-not $done) { [Job]::Kill($job); [void]$p.WaitForExit(10000) } else { $p.WaitForExit() }
Unregister-Event -SourceIdentifier $e1.Name; Unregister-Event -SourceIdentifier $e2.Name
$out.Close()
$secs = [int]((Get-Date) - $t0).TotalSeconds
$peak = [int]([Job]::Peak($job) / 1MB)
if (-not $done) { "timeout after ${secs}s peak ${peak}MB"; exit 124 }
$code = $p.ExitCode
if ($code -eq 0) { "ok ${secs}s peak ${peak}MB"; exit 0 }
if ($peak -ge $MemMB - 16) { "memory cap ${MemMB}MB hit (exit $code) after ${secs}s"; exit 125 }
"exit $code after ${secs}s peak ${peak}MB"; exit $code
