MathCAS v2 for macOS
====================

Install: drag "MathCAS Notebook.app" to the Applications folder.

First launch: the application is not signed with an Apple developer identity, so macOS refuses a double click the
first time. Open it once from the context menu (Control-click the application, choose Open, then Open again), or
allow it under System Settings > Privacy & Security. After that it starts normally.

The programs are inside the bundle, in "MathCAS Notebook.app/Contents/MacOS":

  mc-notebook    the notebook (what the application starts)
  mc-docs        the documentation window (also opened from the notebook with F1)
  mc-repl        the REPL and script runner:  mc-repl            start the REPL
                                              mc-repl file.mc    run a script
                                              mc-repl -e "1+1"   evaluate an expression
  mc-kernel      the kernel the notebook and Jupyter talk to
  mc-lsp         the language server for editors

To use the REPL from a terminal, add that folder to PATH, for example in ~/.zshrc:

  export PATH="/Applications/MathCAS Notebook.app/Contents/MacOS:$PATH"

Notebooks are .mcn files; the application opens them. examples/ has scripts and a notebook, quickstart.pdf is the
guide, editors/vscode and jupyter/ hold the editor support.
