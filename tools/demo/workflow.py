"""Record the README's workflow animation: drive the real notebook (and its kernel) through a storyboard with the
notebook's self-test mode, take a screenshot per frame, and assemble an animated GIF.

    python tools/demo/workflow.py --exe <mc-notebook.exe> --out docs/media/workflow.gif [--work <scratch folder>]

The storyboard below is the whole demo: typing (a few characters per frame), typesetting keys, evaluation with
Shift+Enter, a plot and the pointer moving along it with the coordinate read-out. Every frame has its own display
time. Needs Pillow; nothing else.
"""
import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image

# window size in points (screenshots are at the display scale, e.g. 1500 px wide at 150 %)
WIDTH, HEIGHT = 1000, 680


class Board:
    """A self-test script being written, with the display time of every screenshot it takes."""

    def __init__(self, out_dir: Path):
        self.lines = [f"out {out_dir.as_posix()}", f"size {WIDTH} {HEIGHT}", "theme light", "wait 20"]
        self.frames = []  # (name, milliseconds)

    def shot(self, ms: int):
        name = f"f{len(self.frames):04d}"
        self.lines.append(f"shot {name}")
        self.frames.append((name, ms))

    def step(self, line: str, wait: int = 2):
        self.lines.append(line)
        if wait:
            self.lines.append(f"wait {wait}")

    def hold(self, ms: int):
        """Keep the last frame on screen longer."""
        name, old = self.frames[-1]
        self.frames[-1] = (name, old + ms)

    def type(self, text: str, per_frame: int = 2, ms: int = 70):
        """Type `text` a few characters per frame."""
        for i in range(0, len(text), per_frame):
            chunk = text[i:i + per_frame].replace("\\", "\\\\").replace('"', '\\"')
            self.step(f'keys "{chunk}"', wait=1)
            self.shot(ms)

    def keys(self, chord: str, ms: int = 120):
        self.step(f"keys {chord}", wait=2)
        self.shot(ms)

    def evaluate(self, wait_frames: int, hold_ms: int):
        self.step("keys Shift+Enter", wait=wait_frames)
        self.shot(hold_ms)


def storyboard(b: Board):
    b.shot(900)  # the empty notebook

    # 1. exact calculus
    b.type("integrate(x^2*sin(x), x)")
    b.hold(400)
    b.evaluate(wait_frames=150, hold_ms=1800)  # the first evaluation starts the kernel

    # 2. typeset input with the typesetting keys: d(x^2 + 1/x, x)
    b.type("d(x")
    b.keys("Ctrl+6")
    b.type("2")
    b.keys("Ctrl+Space")
    b.type(" + 1")
    b.keys("Ctrl+/")
    b.type("x")
    b.keys("Ctrl+Space")
    b.type(", x")  # the closing bracket was inserted with the opening one
    b.hold(700)
    b.evaluate(wait_frames=40, hold_ms=1800)

    # 3. a plot, then the pointer along a curve with the coordinate read-out
    b.type('plot([sin(x), cos(x)], [x, 0, 2pi], legend: ["sin", "cos"])', per_frame=4, ms=60)
    b.hold(400)
    b.evaluate(wait_frames=60, hold_ms=1200)
    return b


def hover(b: Board, x0: float, x1: float, y0: float, amp: float, steps: int = 18):
    """Move the pointer along the sine curve of the plot, a frame per position: (x0, y0) is the curve's point at
    x = 0 and x1 its point at x = 2 pi, amp the height of 1 (window points, y down)."""
    import math
    for k in range(steps + 1):
        t = 2 * math.pi * k / steps
        x = x0 + (x1 - x0) * k / steps
        y = y0 - amp * math.sin(t)
        b.step(f"move {round(x)} {round(y)}", wait=30)  # tooltips show after a short delay
        b.shot(120)
    b.hold(1200)


def finale(b: Board):
    # 4. solving
    b.step("move 980 660", wait=2)
    b.type("solve(x^3 - 6x^2 + 11x - 6 == 0, x)", per_frame=3, ms=60)
    b.hold(300)
    b.evaluate(wait_frames=40, hold_ms=2600)
    b.step("quit", wait=0)


# the plot's first colour (mc-plot's palette): the sine curve
SINE = (0x1B, 0x5E, 0x63)


def calibrate(exe: str, work: Path) -> str:
    """Run the storyboard up to the plot, find the sine curve's pixels in the last frame and give the hover path
    (x0, x1, y0, amp in window points)."""
    cal = work / "calibrate"
    (cal / "frames").mkdir(parents=True)
    (cal / "cfg").mkdir()
    b = storyboard(Board(cal / "frames"))
    b.step("quit", wait=0)
    script = cal / "demo.txt"
    script.write_text("\n".join(b.lines) + "\n", encoding="utf-8")
    env = dict(os.environ, MCNB_SELFTEST=str(script), MCNB_CONFIG_DIR=str(cal / "cfg"))
    print("calibrating the hover path ...", flush=True)
    subprocess.run([exe], env=env, cwd=cal, timeout=600, check=False)
    im = Image.open(cal / "frames" / f"{b.frames[-1][0]}.png").convert("RGB")
    scale = im.width / WIDTH
    px = im.load()
    pts = [(x, y) for y in range(im.height) for x in range(im.width)
           if all(abs(c - t) <= 24 for c, t in zip(px[x, y], SINE))]
    if len(pts) < 50:
        sys.exit("calibration: the sine curve was not found in the plot")
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    x0, x1, ymin, ymax = min(xs), max(xs), min(ys), max(ys)
    return ",".join(f"{v / scale:.1f}" for v in (x0, x1, (ymin + ymax) / 2, (ymax - ymin) / 2))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--work", default=None)
    ap.add_argument("--hover", default=None,
                    help="x0,x1,y0,amp of the sine curve in the plot (window points; see hover()); found by a "
                         "calibration pass when not given")
    ap.add_argument("--width", type=int, default=960, help="width of the GIF in pixels")
    ap.add_argument("--mp4", default=None, help="also write an MP4 (H.264) here; needs ffmpeg")
    ap.add_argument("--ffmpeg", default=shutil.which("ffmpeg"), help="the ffmpeg program (default: on PATH)")
    a = ap.parse_args()

    work = Path(a.work or Path(os.environ.get("TEMP", ".")) / "mathcas-demo")
    if work.exists():
        shutil.rmtree(work)
    (work / "frames").mkdir(parents=True)
    (work / "cfg").mkdir()
    if a.hover is None:
        a.hover = calibrate(a.exe, work)
        print(f"hover path: {a.hover}", flush=True)
    b = storyboard(Board(work / "frames"))
    x0, x1, y0, amp = (float(v) for v in a.hover.split(","))
    hover(b, x0, x1, y0, amp)
    finale(b)
    script = work / "demo.txt"
    script.write_text("\n".join(b.lines) + "\n", encoding="utf-8")

    env = dict(os.environ, MCNB_SELFTEST=str(script), MCNB_CONFIG_DIR=str(work / "cfg"))
    print(f"recording {len(b.frames)} frames ...", flush=True)
    subprocess.run([a.exe], env=env, cwd=work, timeout=600, check=False)

    if a.ffmpeg:
        encode_with_ffmpeg(a, work, b.frames)
        return
    images, durations = [], []
    for i, (name, ms) in enumerate(b.frames):
        p = work / "frames" / f"{name}.png"
        if not p.exists():
            sys.exit(f"missing frame {p}: see {work / 'frames' / 'selftest.log'}")
        im = Image.open(p).convert("RGB")
        if im.width != a.width:
            im = im.resize((a.width, round(im.height * a.width / im.width)), Image.LANCZOS)
        images.append(im)
        durations.append(ms)
        if i % 20 == 0:
            print(f"[{i + 1}/{len(b.frames)}] {name}", flush=True)
    # one shared palette from a few representative frames keeps colours stable and the file small
    sample = Image.new("RGB", (images[0].width, images[0].height * 4))
    for k, im in enumerate([images[len(images) * j // 4] for j in range(4)]):
        sample.paste(im, (0, im.height * k))
    palette = sample.quantize(colors=128, method=Image.Quantize.MEDIANCUT)
    frames = [im.quantize(palette=palette, dither=Image.Dither.NONE) for im in images]
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    frames[0].save(out, save_all=True, append_images=frames[1:], duration=durations, loop=0, optimize=True, disposal=1)
    print(f"wrote {out} ({out.stat().st_size / 1e6:.1f} MB, {len(frames)} frames, {sum(durations) / 1000:.1f} s)")


def encode_with_ffmpeg(a, work: Path, frames):
    """The MP4 (H.264, full resolution, 30 fps) and a GIF with an optimised palette, both from the frames with
    their display times (an ffmpeg concat list)."""
    listing = work / "frames.txt"
    lines = []
    for name, ms in frames:
        p = work / "frames" / f"{name}.png"
        if not p.exists():
            sys.exit(f"missing frame {p}: see {work / 'frames' / 'selftest.log'}")
        lines += [f"file '{p.as_posix()}'", f"duration {ms / 1000:.3f}"]
    lines.append(f"file '{(work / 'frames' / (frames[-1][0] + '.png')).as_posix()}'")  # the last duration needs a next file
    listing.write_text("\n".join(lines) + "\n", encoding="utf-8")
    src = [a.ffmpeg, "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(listing)]
    if a.mp4:
        Path(a.mp4).parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(src + ["-vf", "fps=30,scale=trunc(iw/2)*2:trunc(ih/2)*2:flags=lanczos", "-c:v", "libx264",
                              "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
                              a.mp4], check=True)
        print(f"wrote {a.mp4} ({Path(a.mp4).stat().st_size / 1e6:.1f} MB)")
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    gif_filter = (f"fps=15,scale={a.width}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=128:"
                  f"stats_mode=diff[p];[s1][p]paletteuse=dither=none:diff_mode=rectangle")
    subprocess.run(src + ["-vf", gif_filter, "-loop", "0", a.out], check=True)
    total = sum(ms for _, ms in frames) / 1000
    print(f"wrote {a.out} ({Path(a.out).stat().st_size / 1e6:.1f} MB, {total:.1f} s)")


if __name__ == "__main__":
    main()
