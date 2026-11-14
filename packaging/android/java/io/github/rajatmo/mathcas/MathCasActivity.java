package io.github.rajatmo.mathcas;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.content.res.Configuration;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.view.View;
import android.view.WindowInsetsController;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.Callable;
import java.util.concurrent.FutureTask;
import java.util.concurrent.TimeUnit;

/**
 * The notebook's activity: the system's NativeActivity (which loads and runs the notebook's native library) with the
 * few things only Java can ask of Android: the share sheet, the clipboard, the document picker, and the file
 * another app hands over with "Open with". The native side calls these methods (crates/mc-ui/src/android.rs); text
 * crosses as UTF-8 bytes.
 */
public class MathCasActivity extends NativeActivity {
    private static final int PICK = 1;

    /** The path of a file handed to the app, copied into its cache, until the native side takes it. */
    private String incoming;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        take(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        take(intent);
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == PICK && result == RESULT_OK && data != null && data.getData() != null) {
            copyIn(data.getData());
        }
    }

    /** "Open with": the file the intent names. */
    private void take(Intent intent) {
        if (intent != null && Intent.ACTION_VIEW.equals(intent.getAction()) && intent.getData() != null) {
            copyIn(intent.getData());
        }
    }

    /** Copy a document another app lets this one read into the cache; the native side opens the copy. */
    private void copyIn(Uri uri) {
        try {
            File dir = new File(cache(), "incoming");
            dir.mkdirs();
            File out = new File(dir, nameOf(uri));
            try (InputStream in = getContentResolver().openInputStream(uri);
                 OutputStream to = new FileOutputStream(out)) {
                byte[] buffer = new byte[65536];
                for (int n; (n = in.read(buffer)) > 0; ) {
                    to.write(buffer, 0, n);
                }
            }
            synchronized (this) {
                incoming = out.getPath();
            }
        } catch (Exception e) {
            android.util.Log.e("MathCAS", "cannot read " + uri, e);
        }
    }

    /** The name the document's provider shows for it, else the last part of its address. */
    private String nameOf(Uri uri) {
        String name = null;
        try (Cursor c = getContentResolver().query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst() && !c.isNull(0)) {
                name = c.getString(0);
            }
        } catch (Exception e) {
            // a provider without names
        }
        if (name == null || name.isEmpty()) {
            name = uri.getLastPathSegment();
        }
        if (name == null || name.isEmpty()) {
            name = "Notebook.mcn";
        }
        name = name.substring(name.lastIndexOf('/') + 1);
        return name.endsWith(".mcn") ? name : name + ".mcn";
    }

    /** The native side's temporary folder (TMPDIR, set in mc_ui::android::start). */
    File cache() {
        return new File(getFilesDir(), "cache");
    }

    private static String text(byte[] utf8) {
        return new String(utf8, StandardCharsets.UTF_8);
    }

    private <T> T onUiThread(Callable<T> work) {
        FutureTask<T> task = new FutureTask<>(work);
        runOnUiThread(task);
        try {
            return task.get(2, TimeUnit.SECONDS);
        } catch (Exception e) {
            return null;
        }
    }

    // ------------------------------------------------------------------ called from the native side

    /** The path of a file handed to the app since the last call, or null. */
    public synchronized byte[] takeIncoming() {
        String path = incoming;
        incoming = null;
        return path == null ? null : path.getBytes(StandardCharsets.UTF_8);
    }

    /** Open the system's document picker; the answer comes to onActivityResult. */
    public void pickDocument() {
        runOnUiThread(() -> {
            Intent pick = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            pick.addCategory(Intent.CATEGORY_OPENABLE);
            pick.setType("*/*");
            startActivityForResult(pick, PICK);
        });
    }

    /** Offer a file of the shared folder to other apps. */
    public void shareFile(byte[] path, byte[] mime) {
        Uri uri = SharedFiles.uriOf(new File(text(path)));
        String type = text(mime);
        runOnUiThread(() -> {
            Intent send = new Intent(Intent.ACTION_SEND);
            send.setType(type);
            send.putExtra(Intent.EXTRA_STREAM, uri);
            send.setClipData(ClipData.newRawUri(null, uri));
            send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            startActivity(Intent.createChooser(send, null));
        });
    }

    /** Offer a text to other apps. */
    public void shareText(byte[] utf8) {
        String shared = text(utf8);
        runOnUiThread(() -> {
            Intent send = new Intent(Intent.ACTION_SEND);
            send.setType("text/plain");
            send.putExtra(Intent.EXTRA_TEXT, shared);
            startActivity(Intent.createChooser(send, null));
        });
    }

    /** Put a text on the clipboard. */
    public void copyText(byte[] utf8) {
        String copied = text(utf8);
        runOnUiThread(() -> clipboard().setPrimaryClip(ClipData.newPlainText("MathCAS", copied)));
    }

    /** Put a picture file of the shared folder on the clipboard. */
    public void copyImage(byte[] path) {
        Uri uri = SharedFiles.uriOf(new File(text(path)));
        runOnUiThread(() -> clipboard().setPrimaryClip(ClipData.newUri(getContentResolver(), "MathCAS picture", uri)));
    }

    /** The text on the clipboard, or null. */
    public byte[] clipboardText() {
        String got = onUiThread(() -> {
            ClipData clip = clipboard().getPrimaryClip();
            if (clip == null || clip.getItemCount() == 0) {
                return null;
            }
            CharSequence s = clip.getItemAt(0).coerceToText(this);
            return s == null ? null : s.toString();
        });
        return got == null ? null : got.getBytes(StandardCharsets.UTF_8);
    }

    /** One byte: 1 when the system shows its dark theme, else 0. */
    public byte[] nightMode() {
        int night = getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK;
        return new byte[] {(byte) (night == Configuration.UI_MODE_NIGHT_YES ? 1 : 0)};
    }

    /** The page under the system's bars is light (first byte 1) or dark (0): their clock and icons contrast. */
    public void lightBars(byte[] light) {
        boolean on = light.length > 0 && light[0] != 0;
        runOnUiThread(() -> {
            View view = getWindow().getDecorView();
            if (Build.VERSION.SDK_INT >= 30) {
                WindowInsetsController bars = view.getWindowInsetsController();
                if (bars != null) {
                    int both = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                            | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
                    bars.setSystemBarsAppearance(on ? both : 0, both);
                }
            } else {
                int both = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR;
                int flags = view.getSystemUiVisibility();
                view.setSystemUiVisibility(on ? flags | both : flags & ~both);
            }
        });
    }

    private ClipboardManager clipboard() {
        return (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
    }
}
