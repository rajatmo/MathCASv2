MathCAS v2 for Android
======================

The package MathCAS-v2-<version>-<commit>-android.apk holds the notebook and the kernel for phones and tablets
(arm64-v8a) and for the emulators of a PC (x86_64). Android 9 or later.

Install: copy the .apk to the device and open it (the system asks once to allow installing from that source), or
from a computer:  adb install MathCAS-v2-<version>-<commit>-android.apk

The app has its own layout, made for three things: opening a notebook, a quick calculation, and handing a plot or a
result to another app. The cells, the typeset output, the plots and the kernel are those of the desktop notebook.

  Home        "New calculation" starts an empty notebook with the keyboard up. Below it the notebooks kept in the
              app, newest first; the dots beside one rename, share or delete it. "Open a file..." opens Android's
              file picker (Downloads, Drive, a USB stick...): the chosen notebook is copied into the app and opened.
              A .mcn file opened from another app ("Open with") arrives the same way.
  Notebook    The bar at the top: back to Home, the name (tap it to rename), the kernel (a dot; a stop button while
              it works), and the dots for the rest: Evaluate all, Clear results, Delete this cell, Restart kernel,
              Share notebook, Share as PDF, Theme, Text size, About.
  Editing     Tap a cell to edit it. A row of keys sits on the keyboard: brackets, operators, caret left and right,
              tab, and Run (what Shift+Enter is on a desktop). Names that complete what is typed show above the
              keys; tap one to take it. The round + adds a new input at the end.
  Results     Tap a result or a picture: Copy, Share (the system's share sheet; a picture goes as a PNG) and
              Delete show at the bottom; a result also has LaTeX (copies it as LaTeX).
  Touch       A finger that moves scrolls. A tap is a click. A finger that rests for half a second and then moves
              drags as the mouse does (it selects text, turns a selected 3D picture, resizes a picture).
  Saving      There is no Save: every change is written within two seconds and when the notebook is left. A new
              calculation is named "Calculation N" once it has content. The files are in
              Android/data/io.github.rajatmo.mathcas/files, which a computer reaches over USB; removing the app
              removes them (share a notebook to keep a copy elsewhere).
  Not there   Cell styles and grouping, find, preferences, the documentation window, the REPL and the language
              server; pasting from other apps (copying to them works).

Building it (packaging/android/build-apk.sh, on Linux, macOS or Windows with Git Bash): needs the Android SDK with a
platform, build-tools and the NDK (Android Studio installs all three), a JDK (Android Studio's own is found), and

  rustup target add aarch64-linux-android x86_64-linux-android

The app's few lines of Java (java/: the share sheet, the clipboard, the file picker) are compiled by the script with
javac and d8; there is no Gradle project. The package is signed with a debug key unless KEYSTORE, KEY_ALIAS and
KEYSTORE_PASS name another: fine for installing by hand, not for a store. packaging/android/smoke.sh PACKAGE.apk
installs it on a running emulator or a connected device and runs the notebook's self-test there (the start screen,
a new calculation, an integral, a plot, screenshots).

The layout can be tried on a desktop: MCNB_LAYOUT=mobile mc-notebook (MCNB_DOCUMENTS names the notebooks folder).
