MathCAS v2 for Linux (x86-64)
=============================

Run the programs from this folder, or install them for your user with

  ./install.sh                 programs in ~/.local/lib/mathcas-v2, links in ~/.local/bin, menu entries for the
                               notebook and the documentation, the .mcn file type
  ./install.sh --uninstall     remove them again

bin/mc-notebook     the notebook window (bin/mc-notebook file.mcn)
bin/mc-docs         the documentation window (bin/mc-docs factor)
bin/mc-repl         the REPL and script runner (bin/mc-repl -e "factor(x^4 - 1)"; --help lists the options)
bin/mc-kernel       the kernel for notebooks and editors (JSON lines over stdio)
bin/mc-lsp          the language server (editors/vscode holds the VS Code extension)
bin/mc-docs-pages   checks or regenerates documentation pages
bin/mc-bench        the benchmark runner (bench/cases)
jupyter/            a Jupyter kernel: python3 jupyter/install.py --kernel "$PWD/bin/mc-kernel"

The windows need a desktop session (Wayland or X11) with OpenGL or Vulkan drivers (Mesa is enough); file dialogs
use the desktop portal (xdg-desktop-portal) or zenity. The REPL, kernel and language server need nothing beyond the
C library. On a machine without working GPU drivers a window may need software rendering:
  WGPU_BACKEND=gl LIBGL_ALWAYS_SOFTWARE=1 bin/mc-notebook

LICENSE is the licence (GNU General Public License, version 3 or later); THIRD-PARTY-NOTICES.txt lists the
third-party components built into the programs and their licences. README.md describes the language;
quickstart.pdf is a short guide; examples/ has scripts and notebooks.
