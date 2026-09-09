package io.hoenn.sessions;

import android.app.*;
import android.os.Bundle;
import java.io.*;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;

// Invoked explicitly by adb instrumentation. Input is app-private, never an Intent extra.
// No credentials, tokens, ROM or synthetic cloud save are packaged with this runner.
public final class DeviceSmoke extends Instrumentation {
    @Override public void onCreate(Bundle args) {super.onCreate(args);start();}
    @Override public void onStart() {
        Bundle result=new Bundle();StringBuilder report=new StringBuilder();CloudApi api=new CloudApi();
        File input=new File(getTargetContext().getFilesDir(),"device-smoke.json");
        try {
            NativeCore.close();report.append("native_load=PASS\n");
            if(NativeCore.readBridge(0x01000000)!=null)throw new AssertionError("Invalid bridge accepted");
            report.append("bridge_out_of_bounds=REJECTED\n");
            api.health();report.append("https_readiness=PASS\n");
            JSONObject config=new JSONObject(new String(CloudApi.bounded(new FileInputStream(input),4096),StandardCharsets.UTF_8));
            if(!input.delete())throw new IOException("Could not delete private test input");
            if(config.has("invitation_code")) {api.register(config.getString("username"),config.getString("password"),config.getString("invitation_code"));report.append("registration=PASS\n");}
            api.login(config.getString("username"),config.getString("password")); report.append("login=PASS\n");
            long revision=api.acquire();report.append("lease_acquire=PASS revision=").append(revision).append('\n');
            api.heartbeat();report.append("lease_heartbeat=PASS\n");
            try {api.verifyResume();report.append("pinned_server_signature=PASS\n");}
            catch(CloudApi.HttpError e){if(e.status!=404 || revision!=0)throw e;report.append("pinned_server_signature=UNTESTED no snapshot (HTTP 404, revision 0)\n");}
            api.release();report.append("lease_release=PASS\n");
            api.logout();report.append("logout=PASS\n");
            report.append("game_presence_cloud_save=UNTESTED matching ROM unavailable\n");
            result.putString("stream",report.toString());finish(Activity.RESULT_OK,result);
        } catch(Throwable e) {
            // Only controlled exception type/status is returned; do not print bodies or credentials.
            report.append("failure=").append(e.getClass().getSimpleName());
            if(e instanceof java.net.UnknownHostException)report.append(" DNS ").append(e.getMessage());
            if(e instanceof CloudApi.HttpError)report.append(" HTTP ").append(((CloudApi.HttpError)e).status);
            try{api.logout();}catch(Exception ignored){}
            result.putString("stream",report.toString());finish(Activity.RESULT_CANCELED,result);
        }finally{input.delete();}
    }
}
