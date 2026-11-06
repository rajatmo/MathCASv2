# nb-storm.ps1 (tools/nb-storm.ps1): an input storm against the notebook window on Windows.
#
# Starts mc-notebook on a copy of a notebook, in a settings folder of its own, inside a job object (memory cap; the
# window and its kernels are killed together at the end), and posts real window messages to it: characters, keys
# with modifiers, mouse moves, clicks, double clicks, drags, wheel turns, window resizes, minimize and restore. The
# messages go through the window's own message loop as typed input does, without moving the user's pointer or
# taking the keyboard. Every second it samples CPU, memory, handles and threads and checks that the window answers
# (a hang of more than -HangSec seconds stops the run). Shortcuts that open system file dialogs (Ctrl+O, Ctrl+S,
# Ctrl+Shift+S, Ctrl+P), that quit (Alt+F4, Ctrl+Q) or touch the user's clipboard (Ctrl+C, Ctrl+X, Ctrl+V) are not
# sent.
#
#   -Exe        the notebook program
#   -Notebook   a notebook to copy and open (optional; its sidecar folder of pictures, `<name>.files`, is copied too)
#   -Out        a folder for the copy, settings, samples.csv and the log
#   -Seconds    storm length;  -Rate events per second;  -Seed random seed
#   -Mix        type | mouse | window | eval | all
#   -MemMB      memory cap of the job (window + kernels);  -Check: only check that Ctrl+N gets through
# Prints progress every 5 s and a final line "ok ..." or "FAIL <reason> ...".
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Out,
    [string] $Notebook = "",
    [int] $Seconds = 60,
    [int] $Rate = 400,
    [int] $Seed = 1,
    [ValidateSet('type', 'mouse', 'window', 'eval', 'all')] [string] $Mix = 'all',
    [int] $MemMB = 3072,
    [int] $HangSec = 5,
    # only check that a modifier shortcut gets through (Ctrl+N opens an Untitled tab), then stop
    [switch] $Check
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $Out | Out-Null
Add-Type -TypeDefinition @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class Storm {
    [StructLayout(LayoutKind.Sequential)] struct BASIC { public long a, b; public uint LimitFlags; public UIntPtr c, d; public uint e; public UIntPtr f; public uint g, h; }
    [StructLayout(LayoutKind.Sequential)] struct IO { public ulong a, b, c, d, e, f; }
    [StructLayout(LayoutKind.Sequential)] struct EXT { public BASIC Basic; public IO Io; public UIntPtr ProcessMemoryLimit, JobMemoryLimit, PeakProcessMemoryUsed, PeakJobMemoryUsed; }
    [DllImport("kernel32.dll")] static extern IntPtr CreateJobObject(IntPtr a, string n);
    [DllImport("kernel32.dll")] static extern bool SetInformationJobObject(IntPtr j, int c, ref EXT i, uint l);
    [DllImport("kernel32.dll")] static extern bool QueryInformationJobObject(IntPtr j, int c, ref EXT i, uint l, IntPtr r);
    [DllImport("kernel32.dll")] static extern bool AssignProcessToJobObject(IntPtr j, IntPtr p);
    [DllImport("kernel32.dll")] static extern bool TerminateJobObject(IntPtr j, uint code);
    public static IntPtr Job(ulong bytes) {
        IntPtr j = CreateJobObject(IntPtr.Zero, null);
        EXT e = new EXT();
        e.Basic.LimitFlags = 0x200 | 0x2000; // JOB_MEMORY limit | KILL_ON_JOB_CLOSE
        e.JobMemoryLimit = (UIntPtr)bytes;
        SetInformationJobObject(j, 9, ref e, (uint)Marshal.SizeOf(typeof(EXT)));
        return j;
    }
    public static bool Assign(IntPtr j, IntPtr p) { return AssignProcessToJobObject(j, p); }
    public static void Kill(IntPtr j) { TerminateJobObject(j, 124); }
    public static long PeakMB(IntPtr j) {
        EXT e = new EXT();
        QueryInformationJobObject(j, 9, ref e, (uint)Marshal.SizeOf(typeof(EXT)), IntPtr.Zero);
        return (long)((ulong)e.PeakJobMemoryUsed / 1048576UL);
    }

    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, IntPtr l, uint f, uint t, out IntPtr r);
    [DllImport("user32.dll")] static extern uint MapVirtualKey(uint c, uint t);
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int hh, uint f);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int c);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint a, uint b, bool on);
    [DllImport("user32.dll")] static extern bool SetKeyboardState(byte[] s);
    [DllImport("user32.dll")] static extern bool GetKeyboardState(byte[] s);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [StructLayout(LayoutKind.Sequential)] struct MSG { public IntPtr h; public uint m; public IntPtr w, l; public uint t; public int x, y; }
    [DllImport("user32.dll")] static extern bool PeekMessage(out MSG m, IntPtr h, uint a, uint b, uint r);
    public static bool LastAttach;
    [DllImport("user32.dll")] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern bool IsWindow(IntPtr h);
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    // the visible top-level window of a process whose title ends with the notebook's name (0 when none yet)
    public static IntPtr FindWindow(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p == pid && IsWindowVisible(h)) {
                var s = new StringBuilder(256); GetWindowText(h, s, 256);
                if (s.ToString().EndsWith("MathCAS Notebook")) { found = h; return false; }
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public struct RECT { public int L, T, R, B; }
    public struct POINT { public int X, Y; }

    const uint WM_KEYDOWN = 0x100, WM_KEYUP = 0x101, WM_CHAR = 0x102, WM_MOUSEMOVE = 0x200, WM_LBUTTONDOWN = 0x201,
        WM_LBUTTONUP = 0x202, WM_LBUTTONDBLCLK = 0x203, WM_RBUTTONDOWN = 0x204, WM_RBUTTONUP = 0x205,
        WM_MBUTTONDOWN = 0x207, WM_MBUTTONUP = 0x208, WM_MOUSEWHEEL = 0x20A;
    public static IntPtr H;
    static Random R;
    static int mx = 100, my = 100;
    public static long Posted;

    public static void Init(IntPtr h, int seed) { H = h; R = new Random(seed); }
    public static bool Alive() { return IsWindow(H); }
    public static string Title() { var s = new StringBuilder(256); GetWindowText(H, s, 256); return s.ToString(); }
    // milliseconds the window took to answer, or -1 when it did not within the limit
    public static int Ping(uint ms) {
        IntPtr r; var t = Environment.TickCount;
        IntPtr ok = SendMessageTimeout(H, 0, IntPtr.Zero, IntPtr.Zero, 0x2, ms, out r);
        return ok == IntPtr.Zero ? -1 : Environment.TickCount - t;
    }
    static void Post(uint m, long w, long l) { PostMessage(H, m, (IntPtr)w, (IntPtr)l); Posted++; }
    static long KeyL(uint vk, bool up) {
        long sc = MapVirtualKey(vk, 0);
        bool ext = vk == 0x25 || vk == 0x26 || vk == 0x27 || vk == 0x28 || vk == 0x2D || vk == 0x2E || vk == 0x24 || vk == 0x23 || vk == 0x21 || vk == 0x22;
        long l = 1 | (sc << 16) | (ext ? (1L << 24) : 0);
        if (up) l |= (1L << 30) | (1L << 31);
        return l;
    }
    // modifiers: the window reads them from its thread's key state, so they are set there while the key is posted
    static void WithMods(bool ctrl, bool shift, bool alt, Action a) {
        uint pid; uint target = GetWindowThreadProcessId(H, out pid), me = GetCurrentThreadId();
        MSG msg; PeekMessage(out msg, IntPtr.Zero, 0, 0, 0); // this thread gets a message queue to attach
        bool attached = (ctrl || shift || alt) && AttachThreadInput(me, target, true);
        if (ctrl || shift || alt) LastAttach = attached;
        byte[] ks = new byte[256], old = new byte[256];
        if (attached) {
            GetKeyboardState(old); Array.Copy(old, ks, 256);
            if (ctrl) { ks[0x11] = 0x80; ks[0xA2] = 0x80; }
            if (shift) { ks[0x10] = 0x80; ks[0xA0] = 0x80; }
            if (alt) { ks[0x12] = 0x80; ks[0xA4] = 0x80; }
            SetKeyboardState(ks);
        }
        if (ctrl) Post(WM_KEYDOWN, 0x11, KeyL(0x11, false));
        if (shift) Post(WM_KEYDOWN, 0x10, KeyL(0x10, false));
        a();
        if (shift) Post(WM_KEYUP, 0x10, KeyL(0x10, true));
        if (ctrl) Post(WM_KEYUP, 0x11, KeyL(0x11, true));
        if (attached) {
            // the posted keys must be read before the state is put back; a sent message would be answered ahead of
            // them, so the window gets a moment to drain its queue
            System.Threading.Thread.Sleep(25); Ping(2000);
            SetKeyboardState(old);
            AttachThreadInput(me, target, false);
        }
    }
    public static void Key(uint vk, bool ctrl, bool shift, bool alt) {
        WithMods(ctrl, shift, alt, () => { Post(WM_KEYDOWN, vk, KeyL(vk, false)); Post(WM_KEYUP, vk, KeyL(vk, true)); });
    }
    public static void Text(string s) {
        foreach (char c in s) {
            if (c == '\n') { Key(0x0D, false, false, false); continue; }
            Post(WM_CHAR, c, 1);
        }
    }
    static void Size(out int w, out int h) { RECT r; GetClientRect(H, out r); w = Math.Max(1, r.R); h = Math.Max(1, r.B); }
    static long XY(int x, int y) { return (long)(ushort)x | ((long)(ushort)y << 16); }
    public static void Move(int x, int y) { mx = x; my = y; Post(WM_MOUSEMOVE, 0, XY(x, y)); }
    public static void RandomMove() { int w, h; Size(out w, out h); Move(R.Next(w), R.Next(h)); }
    public static void Click(int button, bool dbl) {
        int w, h; Size(out w, out h); int x = R.Next(w), y = R.Next(h); Move(x, y);
        uint dn = button == 0 ? WM_LBUTTONDOWN : button == 1 ? WM_RBUTTONDOWN : WM_MBUTTONDOWN;
        uint up = button == 0 ? WM_LBUTTONUP : button == 1 ? WM_RBUTTONUP : WM_MBUTTONUP;
        long mk = button == 0 ? 1 : button == 1 ? 2 : 0x10;
        Post(dn, mk, XY(x, y)); Post(up, 0, XY(x, y));
        if (dbl) { Post(WM_LBUTTONDBLCLK, 1, XY(x, y)); Post(WM_LBUTTONUP, 0, XY(x, y)); }
    }
    public static void Drag() {
        int w, h; Size(out w, out h); int x = R.Next(w), y = R.Next(h); Move(x, y);
        Post(WM_LBUTTONDOWN, 1, XY(x, y));
        int n = 5 + R.Next(30);
        for (int i = 0; i < n; i++) { x = Math.Max(0, Math.Min(w - 1, x + R.Next(-60, 61))); y = Math.Max(0, Math.Min(h + 40, y + R.Next(-60, 61))); Post(WM_MOUSEMOVE, 1, XY(x, y)); }
        Post(WM_LBUTTONUP, 0, XY(x, y));
    }
    public static void Wheel(bool ctrl) {
        POINT p = new POINT(); p.X = mx; p.Y = my; ClientToScreen(H, ref p);
        int d = (R.Next(2) == 0 ? -120 : 120) * (1 + R.Next(3));
        WithMods(ctrl, false, false, () => Post(WM_MOUSEWHEEL, ((long)(ushort)(short)d << 16) | (ctrl ? 8L : 0L), XY(p.X, p.Y)));
    }
    public static void Resize() {
        SetWindowPos(H, IntPtr.Zero, 40 + R.Next(200), 40 + R.Next(150), 360 + R.Next(1300), 260 + R.Next(850), 0x14);
    }
    public static void MinRestore() { ShowWindow(H, 6); ShowWindow(H, 9); }
}
"@

$texts = @('x', 'y', '1', '2', '+', '-', '*', '/', '^', '(', ')', '[', ']', '{', '}', ',', ' ', ' ', 'sin', 'n(pi, 30)',
    'table(i^2, i, 1, 50)', 'let a = 3', 'fn f(x) = x^2', 'f(a)', '"text"', '# note', 'sqrt(2)', '1/0', 'integrate(x^2, x)',
    "`n", "`n", 'alpha', [string][char]0xE4, [string][char]0x3C0, [string][char]0x222B, [string][char]0x2192)
$evals = @('1 + 1', 'n(pi, 40)', 'table(i^2, i, 1, 100)', 'expand((x + 1)^8)', 'sum(1/k^2, k, 1, 1000)',
    'fn g(x) = x + 1', 'g(41)', 'plot(sin(x), [x, 0, 10])', 'factor(x^6 - 1)', 'undefined_thing(3)', '1/0', 'solve(x^2 == 2, x)')
$rng = [Random]::new($Seed)
# key codes: Left Up Right Down Home End PageUp PageDown Back Delete Tab Escape Enter F2 F3
$nav = 0x25, 0x26, 0x27, 0x28, 0x24, 0x23, 0x21, 0x22, 0x08, 0x2E, 0x09, 0x1B, 0x0D, 0x71, 0x72
# Ctrl + letter/digit shortcuts that stay inside the window
$ctrlKeys = [int][char]'A', [int][char]'Z', [int][char]'Y', [int][char]'F', [int][char]'K', [int][char]'N', [int][char]'W', [int][char]'E', [int][char]'L', 0x31, 0x32, 0x39, 0x09, 0x21, 0x22, 0xBE, 0x30
function Act([string] $kind) {
    switch ($kind) {
        'char' { [Storm]::Text($texts[$rng.Next($texts.Count)]) }
        'nav' { [Storm]::Key($nav[$rng.Next($nav.Count)], $false, ($rng.Next(4) -eq 0), $false) }
        'ctrl' { [Storm]::Key($ctrlKeys[$rng.Next($ctrlKeys.Count)], $true, ($rng.Next(5) -eq 0), $false) }
        'alt' { [Storm]::Key(0x12, $false, $false, $false) ; [Storm]::Key(0x1B, $false, $false, $false) }
        'move' { [Storm]::RandomMove() }
        'click' { [Storm]::Click(0, ($rng.Next(4) -eq 0)) }
        'rclick' { [Storm]::Click(1, $false); [Storm]::Key(0x1B, $false, $false, $false) }
        'mclick' { [Storm]::Click(2, $false) }
        'drag' { [Storm]::Drag() }
        'wheel' { [Storm]::Wheel($false) }
        'zoom' { [Storm]::Wheel($true) }
        'resize' { [Storm]::Resize() }
        'minimize' { [Storm]::MinRestore() }
        'eval' { [Storm]::Key(0x28, $true, $false, $false); [Storm]::Text($evals[$rng.Next($evals.Count)]); [Storm]::Key(0x0D, $false, $true, $false) }
    }
}
$weights = @{
    type   = @{ char = 50; nav = 30; ctrl = 6; move = 5; click = 5; wheel = 4 }
    mouse  = @{ move = 40; click = 20; rclick = 3; mclick = 2; drag = 15; wheel = 15; zoom = 3; char = 2 }
    window = @{ resize = 30; minimize = 4; move = 20; click = 10; wheel = 10; zoom = 6; char = 10; nav = 10 }
    eval   = @{ eval = 15; char = 30; nav = 25; click = 10; wheel = 10; ctrl = 3 }
    all    = @{ char = 30; nav = 18; ctrl = 6; alt = 1; move = 12; click = 8; rclick = 1; mclick = 1; drag = 5; wheel = 6; zoom = 2; resize = 4; minimize = 1; eval = 2 }
}[$Mix]
$bag = foreach ($k in $weights.Keys) { , $k * $weights[$k] }
$bag = @($bag | ForEach-Object { $_ })

# the notebook copy, a settings folder of its own (no single-instance hand-over to a real window)
$cfg = Join-Path $Out 'cfg'; New-Item -ItemType Directory -Force $cfg | Out-Null
$argList = @()
if ($Notebook) {
    $copy = Join-Path $Out ([IO.Path]::GetFileName($Notebook)); Copy-Item $Notebook $copy -Force; $argList = @("`"$copy`"")
    $side = [IO.Path]::ChangeExtension($Notebook, $null).TrimEnd('.') + '.files'
    if (Test-Path $side) { Copy-Item $side (Join-Path $Out (Split-Path $side -Leaf)) -Recurse -Force }
}
$env:MCNB_CONFIG_DIR = $cfg
Remove-Item Env:MCNB_SELFTEST -ErrorAction SilentlyContinue
$job = [Storm]::Job([uint64]$MemMB * 1MB)
$p = if ($argList) { Start-Process -FilePath $Exe -ArgumentList $argList -PassThru -WorkingDirectory $Out } else { Start-Process -FilePath $Exe -PassThru -WorkingDirectory $Out }
[void][Storm]::Assign($job, $p.Handle)
$t0 = Get-Date
$hwnd = [IntPtr]::Zero
while ($hwnd -eq [IntPtr]::Zero -and ((Get-Date) - $t0).TotalSeconds -lt 15) { $null = $p.WaitForExit(100); $hwnd = [Storm]::FindWindow([uint32]$p.Id) }
if ($hwnd -eq [IntPtr]::Zero) { [Storm]::Kill($job); "FAIL no window"; exit 1 }
[Storm]::Init($hwnd, $Seed)
if ($Check) {
    $before = [Storm]::Title()
    [Storm]::Key([int][char]'N', $true, $false, $false)
    $t1 = Get-Date
    while ([Storm]::Title() -eq $before -and ((Get-Date) - $t1).TotalSeconds -lt 5) { $null = $p.WaitForExit(50) }
    $after = [Storm]::Title()
    [Storm]::Kill($job)
    "attached: $([Storm]::LastAttach)"
    if ($after -like 'Untitled*') { "ok Ctrl+N: '$before' -> '$after'" } else { "FAIL Ctrl+N: '$before' -> '$after'"; exit 1 }
    exit 0
}
$csv = Join-Path $Out 'samples.csv'
"t,cpu_s,ws_mb,private_mb,handles,threads,ping_ms,posted,title" | Set-Content $csv
$fail = $null; $start = Get-Date; $nextSample = 1.0; $nextReport = 5.0; $lastCpu = 0.0; $maxPing = 0; $samples = @()
$interval = 1000.0 / [Math]::Max(1, $Rate)
$sent = 0
while ($true) {
    $el = ((Get-Date) - $start).TotalSeconds
    if ($el -ge $Seconds) { break }
    if ($p.HasExited -or -not [Storm]::Alive()) { $fail = "window exited (code $($p.ExitCode))"; break }
    # a burst of what one 10 ms slice should hold
    $due = [int]($el * 1000 / $interval) - $sent
    for ($i = 0; $i -lt [Math]::Min($due, 50); $i++) { Act $bag[$rng.Next($bag.Count)]; $sent++ }
    if ($el -ge $nextSample) {
        $nextSample += 1.0
        $ping = [Storm]::Ping([uint32]($HangSec * 1000))
        if ($p.HasExited) { $fail = "window exited (code $($p.ExitCode))"; break }
        $p.Refresh()
        $row = [pscustomobject]@{ t = [Math]::Round($el, 1); cpu = $p.TotalProcessorTime.TotalSeconds; ws = [Math]::Round($p.WorkingSet64 / 1MB); pm = [Math]::Round($p.PrivateMemorySize64 / 1MB); h = $p.HandleCount; th = $p.Threads.Count; ping = $ping }
        $samples += $row
        "{0},{1:N2},{2},{3},{4},{5},{6},{7},`"{8}`"" -f $row.t, $row.cpu, $row.ws, $row.pm, $row.h, $row.th, $ping, [Storm]::Posted, ([Storm]::Title() -replace '"', "'") | Add-Content $csv
        if ($ping -lt 0) { $fail = "window did not answer within $HangSec s"; break }
        $maxPing = [Math]::Max($maxPing, $ping)
    }
    if ($el -ge $nextReport) {
        $nextReport += 5.0
        $r = $samples[-1]
        "{0,3:N0}% t={1,5:N1}s posted={2,7} cpu={3,6:N1}s ws={4,5}MB private={5,5}MB handles={6} threads={7} ping={8}ms" -f (100 * $el / $Seconds), $el, [Storm]::Posted, $r.cpu, $r.ws, $r.pm, $r.h, $r.th, $r.ping
    }
    $null = $p.WaitForExit(10)
}
$el = ((Get-Date) - $start).TotalSeconds
$peak = [Storm]::PeakMB($job)
$first = $samples | Select-Object -First 3 | Measure-Object -Property pm -Average
$last = $samples | Select-Object -Last 3 | Measure-Object -Property pm -Average
$cpuShare = if ($samples.Count -gt 1) { ($samples[-1].cpu - $samples[0].cpu) / ($samples[-1].t - $samples[0].t) } else { 0 }
[Storm]::Kill($job)
$summary = "{0} mix={1} seed={2} {3:N0}s posted={4} rate={5:N0}/s job_peak={6}MB private {7:N0}->{8:N0}MB cpu={9:N2} cores max_ping={10}ms" -f `
    $(if ($fail) { "FAIL $fail" } else { 'ok' }), $Mix, $Seed, $el, [Storm]::Posted, ([Storm]::Posted / [Math]::Max(1, $el)), $peak, $first.Average, $last.Average, $cpuShare, $maxPing
$summary | Add-Content (Join-Path $Out 'storm.log')
$summary
if ($fail) { exit 1 }
