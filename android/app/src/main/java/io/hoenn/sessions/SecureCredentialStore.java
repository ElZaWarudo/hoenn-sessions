package io.hoenn.sessions;

import android.content.Context;
import android.content.SharedPreferences;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;
import java.nio.charset.StandardCharsets;
import java.security.KeyStore;
import java.security.MessageDigest;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;
import org.json.JSONObject;

/** Small Android Keystore-backed vault used by the Rust authentication layer. */
final class SecureCredentialStore {
    static final class Account {
        final String username;
        final String userId;
        final String characterId;
        Account(String username, String userId, String characterId) {
            this.username = username;
            this.userId = userId;
            this.characterId = characterId;
        }
    }

    private static final String KEY_ALIAS = "hoenn-sessions-refresh-v1";
    private static final String PREFS = "secure_session_v1";
    private static final String ACCOUNT = "account";
    static final String TOKEN_SERVICE = "pokecrossroads-coop-launcher";
    private static Context app;

    private SecureCredentialStore() { }

    static synchronized void initialize(Context context) {
        app = context.getApplicationContext();
    }

    private static SharedPreferences preferences() {
        if (app == null) throw new IllegalStateException("Credential store is not initialized");
        return app.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    private static SecretKey key() throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        SecretKey existing = (SecretKey) store.getKey(KEY_ALIAS, null);
        if (existing != null) return existing;
        KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
        generator.init(new KeyGenParameterSpec.Builder(KEY_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setRandomizedEncryptionRequired(true)
            .build());
        return generator.generateKey();
    }

    private static String encrypt(String plain) throws Exception {
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.ENCRYPT_MODE, key());
        String iv = Base64.encodeToString(cipher.getIV(), Base64.NO_WRAP);
        String data = Base64.encodeToString(cipher.doFinal(plain.getBytes(StandardCharsets.UTF_8)), Base64.NO_WRAP);
        return iv + "." + data;
    }

    private static String decrypt(String encoded) throws Exception {
        int separator = encoded.indexOf('.');
        if (separator <= 0) throw new IllegalArgumentException("Invalid encrypted value");
        byte[] iv = Base64.decode(encoded.substring(0, separator), Base64.NO_WRAP);
        byte[] data = Base64.decode(encoded.substring(separator + 1), Base64.NO_WRAP);
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.DECRYPT_MODE, key(), new GCMParameterSpec(128, iv));
        return new String(cipher.doFinal(data), StandardCharsets.UTF_8);
    }

    private static String tokenKey(String service, String username) throws Exception {
        byte[] digest=MessageDigest.getInstance("SHA-256").digest((service+"\n"+username).getBytes(StandardCharsets.UTF_8));
        return "token:"+Base64.encodeToString(digest,Base64.NO_WRAP|Base64.URL_SAFE);
    }

    static synchronized boolean store(String service, String username, String token) {
        try {
            return preferences().edit().putString(tokenKey(service,username), encrypt(token)).commit();
        } catch (Exception ignored) {
            return false;
        }
    }

    static synchronized String load(String service, String username) {
        try {
            String value = preferences().getString(tokenKey(service,username), null);
            return value == null ? null : decrypt(value);
        } catch (Exception ignored) {
            return null;
        }
    }

    static synchronized boolean delete(String service, String username) {
        try {
            SharedPreferences.Editor editor = preferences().edit().remove(tokenKey(service,username));
            Account account = loadAccount();
            if (account != null && account.username.equals(username)) editor.remove(ACCOUNT);
            return editor.commit();
        } catch (Exception ignored) {
            return false;
        }
    }

    static synchronized boolean storeAccount(String username, String userId, String characterId) {
        try {
            JSONObject value = new JSONObject().put("username", username).put("user_id", userId)
                .put("character_id", characterId);
            return preferences().edit().putString(ACCOUNT, encrypt(value.toString())).commit();
        } catch (Exception ignored) {
            return false;
        }
    }

    static synchronized Account loadAccount() {
        try {
            String value = preferences().getString(ACCOUNT, null);
            if (value == null) return null;
            JSONObject account = new JSONObject(decrypt(value));
            return new Account(account.getString("username"), account.getString("user_id"),
                account.getString("character_id"));
        } catch (Exception ignored) {
            preferences().edit().remove(ACCOUNT).apply();
            return null;
        }
    }

    static synchronized void clearLocalAccount() {
        Account account = loadAccount();
        SharedPreferences.Editor editor = preferences().edit().remove(ACCOUNT);
        if (account != null) try { editor.remove(tokenKey(TOKEN_SERVICE,account.username)); } catch(Exception ignored) { }
        editor.apply();
    }
}
