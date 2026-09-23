package io.hoenn.sessions;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import java.io.File;
import java.io.FileNotFoundException;

/** Read-only, single-file handoff to Android's package installer. */
public final class UpdateProvider extends ContentProvider {
    @Override public boolean onCreate() {return true;}
    @Override public ParcelFileDescriptor openFile(Uri uri,String mode) throws FileNotFoundException {
        if(!"r".equals(mode) || !"/update.apk".equals(uri.getPath()) || getContext()==null)
            throw new FileNotFoundException();
        File apk=new File(new File(getContext().getFilesDir(),"updates"),"update.apk");
        return ParcelFileDescriptor.open(apk,ParcelFileDescriptor.MODE_READ_ONLY);
    }
    @Override public String getType(Uri uri) {return "/update.apk".equals(uri.getPath())?"application/vnd.android.package-archive":null;}
    @Override public Cursor query(Uri uri,String[] projection,String selection,String[] selectionArgs,String sortOrder) {return null;}
    @Override public Uri insert(Uri uri,ContentValues values) {throw new UnsupportedOperationException();}
    @Override public int delete(Uri uri,String selection,String[] selectionArgs) {throw new UnsupportedOperationException();}
    @Override public int update(Uri uri,ContentValues values,String selection,String[] selectionArgs) {throw new UnsupportedOperationException();}
}
