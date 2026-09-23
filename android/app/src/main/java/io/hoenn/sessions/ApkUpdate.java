package io.hoenn.sessions;

import android.app.Activity;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;
import java.io.File;
import java.nio.file.*;
import org.json.JSONObject;

/** Detects a privately published APK and hands installation to Android. */
final class ApkUpdate {
    static final class Available {
        final String releaseId, sha256;
        final long size;
        final int versionCode;
        Available(String releaseId,String sha256,long size,int versionCode) {
            this.releaseId=releaseId;this.sha256=sha256;this.size=size;this.versionCode=versionCode;
        }
    }
    private ApkUpdate() { }
    static Available check(CloudApi api) throws Exception {
        JSONObject metadata;
        try {metadata=api.json("/v1/releases/android/latest",null);}
        catch(CloudApi.HttpError error) {if(error.status==404)return null;throw error;}
        return parse(metadata,BuildConfig.VERSION_CODE);
    }
    static Available parse(JSONObject metadata,int installedVersion) throws Exception {
        String id=metadata.getString("release_id"), hash=metadata.getString("sha256");
        long size=metadata.getLong("size");
        int version=metadata.getInt("version_code");
        if(metadata.length()!=4 || !ReleaseCatalog.safeId(id) || !hash.matches("[0-9a-f]{64}")
            || size<=0 || size>256L*1024*1024 || version<=0)
            throw new SecurityException("Metadatos APK inválidos");
        return version>installedVersion?new Available(id,hash,size,version):null;
    }
    static void download(CloudApi api,Activity activity,Available update) throws Exception {
        File directory=new File(activity.getFilesDir(),"updates");
        if(!directory.isDirectory() && !directory.mkdirs()) throw new java.io.IOException("No se pudo preparar la actualización");
        File temporary=new File(directory,"update.tmp");
        File ready=new File(directory,"update.apk");
        try {
            api.download("/v1/releases/android/"+update.releaseId+"/apk",temporary,update.size,update.sha256);
            PackageInfo packageInfo=activity.getPackageManager().getPackageArchiveInfo(temporary.getAbsolutePath(),0);
            if(packageInfo==null || !activity.getPackageName().equals(packageInfo.packageName)
                || packageInfo.getLongVersionCode()!=update.versionCode)
                throw new SecurityException("Paquete Android incompatible");
            Files.move(temporary.toPath(),ready.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);
        } finally {temporary.delete();}
    }
    static boolean openInstaller(Activity activity) {
        if(Build.VERSION.SDK_INT>=26 && !activity.getPackageManager().canRequestPackageInstalls()) {
            Intent settings=new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                Uri.parse("package:"+activity.getPackageName()));
            activity.startActivity(settings);
            return false;
        }
        Uri uri=Uri.parse("content://"+activity.getPackageName()+".updates/update.apk");
        Intent install=new Intent(Intent.ACTION_INSTALL_PACKAGE);
        install.setDataAndType(uri,"application/vnd.android.package-archive");
        install.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        activity.startActivity(install);
        return true;
    }
}
