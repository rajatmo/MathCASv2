package io.github.rajatmo.mathcas;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;

import java.io.File;
import java.io.FileNotFoundException;

/**
 * Lets the app chosen in the share sheet (or pasting from the clipboard) read one of the files the notebook put into
 * its shared folder: a plot as PNG, a notebook, a PDF. Read only, files of that one folder only, and only for the app
 * the share was granted to (the provider is not exported).
 */
public class SharedFiles extends ContentProvider {
    static final String AUTHORITY = "io.github.rajatmo.mathcas.shared";

    /** The address under which another app reads `file` (a file of the shared folder). */
    static Uri uriOf(File file) {
        return new Uri.Builder().scheme("content").authority(AUTHORITY).appendPath(file.getName()).build();
    }

    /** The shared folder: `mathcas-shared` in the native side's temporary folder. */
    private File folder() {
        return new File(new File(getContext().getFilesDir(), "cache"), "mathcas-shared");
    }

    private File fileOf(Uri uri) throws FileNotFoundException {
        String name = uri.getLastPathSegment();
        if (name == null || name.contains("/") || name.contains("..")) {
            throw new FileNotFoundException(String.valueOf(uri));
        }
        File file = new File(folder(), name);
        if (!file.isFile()) {
            throw new FileNotFoundException(String.valueOf(uri));
        }
        return file;
    }

    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        return ParcelFileDescriptor.open(fileOf(uri), ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public String getType(Uri uri) {
        String name = String.valueOf(uri.getLastPathSegment());
        if (name.endsWith(".png")) {
            return "image/png";
        }
        if (name.endsWith(".pdf")) {
            return "application/pdf";
        }
        return "application/octet-stream";
    }

    @Override
    public Cursor query(Uri uri, String[] columns, String selection, String[] arguments, String order) {
        if (columns == null) {
            columns = new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE};
        }
        MatrixCursor cursor = new MatrixCursor(columns, 1);
        try {
            File file = fileOf(uri);
            Object[] row = new Object[columns.length];
            for (int k = 0; k < columns.length; k++) {
                if (OpenableColumns.DISPLAY_NAME.equals(columns[k])) {
                    row[k] = file.getName();
                } else if (OpenableColumns.SIZE.equals(columns[k])) {
                    row[k] = file.length();
                }
            }
            cursor.addRow(row);
        } catch (FileNotFoundException e) {
            // no such file: no rows
        }
        return cursor;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        return null;
    }

    @Override
    public int delete(Uri uri, String selection, String[] arguments) {
        return 0;
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] arguments) {
        return 0;
    }
}
