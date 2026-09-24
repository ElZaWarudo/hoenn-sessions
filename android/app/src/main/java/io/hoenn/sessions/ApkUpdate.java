package io.hoenn.sessions;

import android.app.Activity;
import android.app.PendingIntent;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;
import java.io.File;
import java.io.FileInputStream;
import java.io.OutputStream;
import java.nio.file.*;
import org.json.JSONObject;

/** Detects a privately published APK and hands installation to Android. */
final class ApkUpdate {
    static final String INSTALL_RESULT="io.hoenn.sessions.INSTALL_RESULT";
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
        download(api,activity,update,null);
    }
    static void download(CloudApi api,Activity activity,Available update,CloudApi.Progress progress) throws Exception {
        File directory=new File(activity.getFilesDir(),"updates");
        if(!directory.isDirectory() && !directory.mkdirs()) throw new java.io.IOException("No se pudo preparar la actualización");
        File temporary=new File(directory,update.releaseId+".tmp");
        File[] partials=directory.listFiles((parent,name)->name.endsWith(".tmp"));
        if(partials!=null)for(File partial:partials)if(!partial.equals(temporary))partial.delete();
        File ready=new File(directory,"update.apk");
        try {
            api.download("/v1/releases/android/"+update.releaseId+"/apk",temporary,update.size,update.sha256,progress);
            PackageInfo packageInfo=activity.getPackageManager().getPackageArchiveInfo(temporary.getAbsolutePath(),0);
            if(packageInfo==null || !activity.getPackageName().equals(packageInfo.packageName)
                || packageInfo.getLongVersionCode()!=update.versionCode)
                throw new SecurityException("Paquete Android incompatible");
            Files.move(temporary.toPath(),ready.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);
        } catch(SecurityException invalid) {temporary.delete();throw invalid;}
    }
    static boolean openInstaller(Activity activity) {
        if(Build.VERSION.SDK_INT>=26 && !activity.getPackageManager().canRequestPackageInstalls()) {
            Intent settings=new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                Uri.parse("package:"+activity.getPackageName()));
            activity.startActivity(settings);
            return false;
        }
        try {
        File apk=new File(new File(activity.getFilesDir(),"updates"),"update.apk");
        if(!apk.isFile())throw new java.io.IOException("Actualización descargada no disponible");
        PackageInstaller installer=activity.getPackageManager().getPackageInstaller();
        PackageInstaller.SessionParams params=new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
        params.setAppPackageName(activity.getPackageName());
        params.setSize(apk.length());
        if(Build.VERSION.SDK_INT>=31)params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED);
        int sessionId=installer.createSession(params);
        PackageInstaller.Session session=null;
        boolean committed=false;
        try {
            session=installer.openSession(sessionId);
            try(FileInputStream input=new FileInputStream(apk);
                OutputStream output=session.openWrite("base.apk",0,apk.length())) {
                byte[] buffer=new byte[65536];int count;
                while((count=input.read(buffer))!=-1)output.write(buffer,0,count);
                session.fsync(output);
            }
            Intent callback=new Intent(activity,MainActivity.class).setAction(INSTALL_RESULT)
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP|Intent.FLAG_ACTIVITY_CLEAR_TOP);
            PendingIntent pending=PendingIntent.getActivity(activity,sessionId,callback,
                PendingIntent.FLAG_UPDATE_CURRENT|(Build.VERSION.SDK_INT>=31?PendingIntent.FLAG_MUTABLE:0));
            session.commit(pending.getIntentSender());
            committed=true;
        } finally {
            if(session!=null)session.close();
            if(!committed)installer.abandonSession(sessionId);
        }
        return true;
        } catch(Exception error) {throw new IllegalStateException("No se pudo iniciar el instalador",error);}
    }
    /** Returns null for unrelated intents or a pending confirmation, otherwise a user-visible result. */
    static String handleInstallResult(Activity activity,Intent intent) {
        if(intent==null || !INSTALL_RESULT.equals(intent.getAction()))return null;
        int status=intent.getIntExtra(PackageInstaller.EXTRA_STATUS,PackageInstaller.STATUS_FAILURE);
        if(status==PackageInstaller.STATUS_PENDING_USER_ACTION) {
            Intent confirmation=intent.getParcelableExtra(Intent.EXTRA_INTENT);
            if(confirmation!=null)activity.startActivity(confirmation);
            return null;
        }
        if(status==PackageInstaller.STATUS_SUCCESS)return "Actualización instalada";
        return "No se pudo instalar la actualización ("+status+")";
    }
    static void cleanupInstalled(Activity activity) {
        File directory=new File(activity.getFilesDir(),"updates");
        File[] partials=directory.listFiles((parent,name)->name.endsWith(".tmp"));
        if(partials!=null)for(File partial:partials)
            if(System.currentTimeMillis()-partial.lastModified()>7L*24*60*60*1000)partial.delete();
        File ready=new File(directory,"update.apk");
        if(!ready.isFile())return;
        PackageInfo packageInfo=activity.getPackageManager().getPackageArchiveInfo(ready.getAbsolutePath(),0);
        if(packageInfo!=null && activity.getPackageName().equals(packageInfo.packageName)
            && packageInfo.getLongVersionCode()<=BuildConfig.VERSION_CODE)ready.delete();
    }
}
