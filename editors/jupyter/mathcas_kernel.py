"""MathCAS v2 kernel for Jupyter (Jupyter Notebook, JupyterLab and VS Code's notebook editor).

A thin wrapper: it starts `mc-kernel` (the MathCAS kernel protocol, JSON lines over standard input and output) and
translates between it and the Jupyter messaging protocol through ipykernel. One Jupyter kernel is one MathCAS session.

The `mc-kernel` executable is found, in order, from the environment variable MATHCAS_KERNEL (set in kernel.json by
install.py), next to this file, or on the PATH.

Run by Jupyter as `python mathcas_kernel.py -f <connection file>` (see install.py).
"""

import base64
import json
import os
import queue
import re
import shutil
import subprocess
import sys
import threading

from ipykernel.kernelapp import IPKernelApp
from ipykernel.kernelbase import Kernel

__version__ = "0.14.0"

# the value of a plot: image("path/to/file.png") (mc-plot writes paths with forward slashes, no escapes)
_IMAGE = re.compile(r'^image\("([^"\\]+\.png)"\)$')

# an identifier, optionally qualified (M.f), ending at the cursor: what completion completes
_WORD = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*\.?$")


def find_mc_kernel():
    """The path of the mc-kernel executable, or None."""
    env = os.environ.get("MATHCAS_KERNEL")
    if env and os.path.isfile(env):
        return env
    here = os.path.dirname(os.path.abspath(__file__))
    for name in ("mc-kernel.exe", "mc-kernel"):
        p = os.path.join(here, name)
        if os.path.isfile(p):
            return p
    return shutil.which("mc-kernel")


class Backend:
    """The mc-kernel process: requests go to its stdin; a reader thread queues its responses."""

    def __init__(self, exe):
        flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
        self.proc = subprocess.Popen(
            [exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            encoding="utf-8", errors="replace", bufsize=1, creationflags=flags)
        self.responses = queue.Queue()
        self.write_lock = threading.Lock()
        self.next_id = 0
        threading.Thread(target=self._read, daemon=True).start()
        hello = self.responses.get()
        if hello is None or hello.get("type") != "hello":
            raise RuntimeError("mc-kernel did not start (got %r)" % (hello,))
        self.banner = hello.get("engine", "MathCAS v2")
        self.running = None  # id of the eval in progress

    def _read(self):
        for line in self.proc.stdout:
            line = line.strip()
            if line:
                try:
                    self.responses.put(json.loads(line))
                except ValueError:
                    pass
        self.responses.put(None)  # the process ended

    def send(self, request):
        with self.write_lock:
            self.proc.stdin.write(json.dumps(request) + "\n")
            self.proc.stdin.flush()

    def request(self, request):
        """Send a request with a fresh id; yield its responses up to and including `done`."""
        self.next_id += 1
        rid = self.next_id
        request["id"] = rid
        self.send(request)
        while True:
            r = self.responses.get()
            if r is None:
                yield {"type": "error", "rendered": "mc-kernel ended unexpectedly", "message": "ended"}
                yield {"type": "done", "status": "error"}
                return
            if r.get("id") != rid:
                continue  # a response to an earlier, abandoned request
            yield r
            if r.get("type") == "done":
                return

    def interrupt(self):
        if self.running is not None:
            self.send({"type": "interrupt", "id": self.running})

    def close(self):
        try:
            self.send({"type": "shutdown"})
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


class MathCASKernel(Kernel):
    implementation = "mathcas"
    implementation_version = __version__
    language = "mathcas"
    language_version = "2"
    language_info = {
        "name": "mathcas",
        "mimetype": "text/x-mathcas",
        "file_extension": ".mc",
        "codemirror_mode": "python",
        "pygments_lexer": "python",
    }
    banner = "MathCAS v2"

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        exe = find_mc_kernel()
        if exe is None:
            raise RuntimeError("mc-kernel not found: set MATHCAS_KERNEL or put mc-kernel on the PATH")
        self.backend = Backend(exe)
        self.banner = self.backend.banner

    # interrupts arrive on the control channel (kernel.json: interrupt_mode "message"); ipykernel would signal the
    # process, which Windows cannot do, so forward them to mc-kernel, which stops at its next safe point
    def _send_interrupt_children(self):
        self.backend.interrupt()

    def _mime(self, printed):
        """The display data of a printed value: a plot's `image("file.png")` shows the picture itself."""
        data = {"text/plain": printed}
        m = _IMAGE.match(printed)
        if m and os.path.isfile(m.group(1)):
            with open(m.group(1), "rb") as fh:
                data["image/png"] = base64.b64encode(fh.read()).decode("ascii")
        return data

    def _stream(self, name, text):
        if not self.silent_now:
            self.send_response(self.iopub_socket, "stream", {"name": name, "text": text})

    async def do_execute(self, code, silent, store_history=True, user_expressions=None, allow_stdin=False,
                         **kwargs):
        self.silent_now = silent
        cell = str(self.execution_count)
        status, errors = "ok", []
        self.backend.running = self.backend.next_id + 1
        try:
            for r in self.backend.request({"type": "eval", "src": code, "cell": cell}):
                kind = r.get("type")
                if kind == "output":
                    self._stream("stdout", r["text"] + "\n")
                elif kind == "failure":
                    self._stream("stderr", r["printed"] + "\n")
                elif kind == "value" and not silent:
                    self.send_response(self.iopub_socket, "execute_result", {
                        "execution_count": self.execution_count,
                        "data": self._mime(r["printed"]),
                        "metadata": {},
                    })
                elif kind == "error":
                    errors.append(r.get("rendered") or r.get("message", "error"))
                elif kind == "done":
                    status = r.get("status", "ok")
        finally:
            self.backend.running = None
        if status in ("ok", "failure"):
            # a failure value is a value: the cell ran, so "run all" continues
            return {"status": "ok", "execution_count": self.execution_count, "payload": [],
                    "user_expressions": {}}
        ename = {"interrupted": "Interrupted", "limit": "Limit"}.get(status, "Error")
        if status == "interrupted" and not errors:
            errors = ["interrupted"]
        evalue = errors[0].splitlines()[0] if errors else status
        traceback = "\n".join(errors).splitlines()
        if not silent:
            self.send_response(self.iopub_socket, "error",
                               {"ename": ename, "evalue": evalue, "traceback": traceback})
        return {"status": "error", "execution_count": self.execution_count, "ename": ename, "evalue": evalue,
                "traceback": traceback}

    async def do_complete(self, code, cursor_pos):
        m = _WORD.search(code[:cursor_pos])
        prefix = m.group(0) if m else ""
        names = []
        for r in self.backend.request({"type": "complete", "prefix": prefix}):
            if r.get("type") == "completions":
                names = r.get("names", [])
        return {"status": "ok", "matches": names, "cursor_start": cursor_pos - len(prefix),
                "cursor_end": cursor_pos, "metadata": {}}

    async def do_is_complete(self, code):
        # open brackets continue the input, as in the REPL
        depth = 0
        for ch in re.sub(r'"(?:\\.|[^"\\])*"|#[^\n]*', "", code):
            depth += ch in "([{"
            depth -= ch in ")]}"
        return {"status": "incomplete", "indent": "  "} if depth > 0 else {"status": "complete"}

    async def do_shutdown(self, restart):
        # a restart starts a new process, so the session ends either way
        self.backend.close()
        return {"status": "ok", "restart": restart}


if __name__ == "__main__":
    IPKernelApp.launch_instance(kernel_class=MathCASKernel)
