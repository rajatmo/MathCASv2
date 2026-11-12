"""Install (or remove) the MathCAS v2 Jupyter kernel.

    python install.py [--kernel PATH] [--sys-prefix | --system] [--python PATH]
    python install.py --uninstall

Writes the kernel spec "mathcas" (kernel.json, the kernel module and the logos) into the user's Jupyter data folder
(default), the current environment (--sys-prefix) or for all users (--system). Jupyter Notebook, JupyterLab and VS
Code's Jupyter extension then offer "MathCAS v2" as a kernel. Needs ipykernel (pip install ipykernel).

--kernel is the mc-kernel executable (default: next to this file's folder, as in the distribution, or on the PATH);
--python the interpreter that runs the kernel (default: this one).
"""

import argparse
import json
import os
import shutil
import sys
import tempfile

NAME = "mathcas"
HERE = os.path.dirname(os.path.abspath(__file__))
FILES = ["mathcas_kernel.py", "logo-32x32.png", "logo-64x64.png"]


def default_kernel():
    for d in (os.path.dirname(HERE), HERE):
        for name in ("mc-kernel.exe", "mc-kernel"):
            p = os.path.join(d, name)
            if os.path.isfile(p):
                return p
    return shutil.which("mc-kernel")


def main():
    ap = argparse.ArgumentParser(description="Install the MathCAS v2 Jupyter kernel.")
    ap.add_argument("--kernel", help="path of mc-kernel(.exe)")
    ap.add_argument("--python", default=sys.executable, help="interpreter that runs the kernel")
    where = ap.add_mutually_exclusive_group()
    where.add_argument("--sys-prefix", action="store_true", help="install into the current environment")
    where.add_argument("--system", action="store_true", help="install for all users")
    ap.add_argument("--uninstall", action="store_true", help="remove the kernel spec (and the default setting)")
    ap.add_argument("--default", action="store_true",
                    help="make MathCAS the kernel new notebooks start with (Jupyter Notebook and JupyterLab)")
    args = ap.parse_args()

    from jupyter_client.kernelspec import KernelSpecManager

    ksm = KernelSpecManager()
    if args.uninstall:
        try:
            ksm.remove_kernel_spec(NAME)
            print("removed the MathCAS kernel spec")
        except Exception as err:  # not installed
            print("nothing to remove:", err)
        set_default(False)
        return 0

    try:
        import ipykernel  # noqa: F401  (the kernel needs it at run time)
    except ImportError:
        print("ipykernel is missing: pip install ipykernel", file=sys.stderr)
        return 1
    kernel = os.path.abspath(args.kernel) if args.kernel else default_kernel()
    if not kernel or not os.path.isfile(kernel):
        print("mc-kernel not found: pass --kernel PATH", file=sys.stderr)
        return 1

    spec = {
        "argv": [args.python, "{resource_dir}" + os.sep + "mathcas_kernel.py", "-f", "{connection_file}"],
        "display_name": "MathCAS v2",
        "language": "mathcas",
        "interrupt_mode": "message",
        "env": {"MATHCAS_KERNEL": kernel},
        "metadata": {"debugger": False},
    }
    with tempfile.TemporaryDirectory() as tmp:
        for f in FILES:
            shutil.copy(os.path.join(HERE, f), tmp)
        with open(os.path.join(tmp, "kernel.json"), "w", encoding="utf-8") as fh:
            json.dump(spec, fh, indent=2)
        dest = ksm.install_kernel_spec(tmp, NAME, user=not (args.sys_prefix or args.system),
                                       prefix=sys.prefix if args.sys_prefix else None)
    print("installed the MathCAS v2 kernel spec in", dest)
    print("  kernel:", kernel)
    if args.default:
        set_default(True)
    return 0


def set_default(on):
    """Make "mathcas" the default kernel of the Jupyter server (on), or drop that setting if it is ours (off).
    The setting is MultiKernelManager.default_kernel_name in the user's jupyter_server_config.json."""
    from jupyter_core.paths import jupyter_config_dir

    path = os.path.join(jupyter_config_dir(), "jupyter_server_config.json")
    try:
        with open(path, encoding="utf-8") as fh:
            cfg = json.load(fh)
    except (OSError, ValueError):
        cfg = {}
    mkm = cfg.setdefault("MultiKernelManager", {})
    if on:
        mkm["default_kernel_name"] = NAME
    elif mkm.get("default_kernel_name") == NAME:
        del mkm["default_kernel_name"]
        if not mkm:
            del cfg["MultiKernelManager"]
    else:
        return
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(cfg, fh, indent=2)
    print(("made MathCAS v2 the default kernel in " if on else "removed the default kernel setting from ") + path)


if __name__ == "__main__":
    sys.exit(main())
