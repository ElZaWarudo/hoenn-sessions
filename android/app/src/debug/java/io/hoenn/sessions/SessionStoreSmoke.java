package io.hoenn.sessions;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Context;
import android.content.SharedPreferences;
import android.os.Bundle;
import java.util.HashMap;
import java.util.Map;

/** Verifies Android Keystore encryption while restoring any prior debug-app state. */
public final class SessionStoreSmoke extends Instrumentation {
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }

    private static void check(boolean value) {
        if(!value) throw new AssertionError("Encrypted session storage check failed");
    }

    @Override public void onStart() {
        Context context=getTargetContext();
        SharedPreferences preferences=context.getSharedPreferences("secure_session_v1",Context.MODE_PRIVATE);
        Map<String,?> original=new HashMap<>(preferences.getAll());
        Bundle result=new Bundle();
        try {
            preferences.edit().clear().commit();
            SecureCredentialStore.initialize(context);
            check(SecureCredentialStore.store("pokecrossroads-coop-launcher","device-smoke","refresh-token-smoke-value"));
            check("refresh-token-smoke-value".equals(SecureCredentialStore.load("pokecrossroads-coop-launcher","device-smoke")));
            check(SecureCredentialStore.storeAccount("device-smoke","11111111-1111-1111-1111-111111111111","22222222-2222-2222-2222-222222222222"));
            SecureCredentialStore.Account account=SecureCredentialStore.loadAccount();
            check(account!=null && "device-smoke".equals(account.username));
            check(!preferences.getAll().toString().contains("refresh-token-smoke-value"));
            check(SecureCredentialStore.delete("pokecrossroads-coop-launcher","device-smoke"));
            check(SecureCredentialStore.loadAccount()==null);
            result.putString("session_store","PASS: Keystore encryption, account metadata, load and delete");
        } catch(Throwable error) {
            result.putString("failure",error.getClass().getSimpleName());
        } finally {
            SharedPreferences.Editor restore=preferences.edit().clear();
            for(Map.Entry<String,?> entry:original.entrySet()) {
                Object value=entry.getValue();
                if(value instanceof String) restore.putString(entry.getKey(),(String)value);
                else if(value instanceof Integer) restore.putInt(entry.getKey(),(Integer)value);
                else if(value instanceof Long) restore.putLong(entry.getKey(),(Long)value);
                else if(value instanceof Float) restore.putFloat(entry.getKey(),(Float)value);
                else if(value instanceof Boolean) restore.putBoolean(entry.getKey(),(Boolean)value);
            }
            restore.commit();
        }
        finish(result.containsKey("failure")?Activity.RESULT_CANCELED:Activity.RESULT_OK,result);
    }
}
