package io.hoenn.sessions;

import org.json.*;
import java.io.*;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.UUID;
import javax.net.ssl.HttpsURLConnection;

final class CloudApi {
    static final String BASE="https://169-128-190-115.sslip.io";
    private String access, refresh, character, userId;
    private JSONObject lease;
    private final String instance=UUID.randomUUID().toString();
    static class HttpError extends IOException {
        final int status;
        HttpError(int status) { super("HTTP "+status); this.status=status; }
    }
    static byte[] bounded(InputStream in, int max) throws IOException {
        try(InputStream input=in; ByteArrayOutputStream out=new ByteArrayOutputStream()) {
            byte[] b=new byte[8192]; int n;
            while((n=input.read(b))!=-1) { if(out.size()+n>max) throw new IOException("Respuesta demasiado grande"); out.write(b,0,n); }
            return out.toByteArray();
        }
    }
    synchronized byte[] request(String path, JSONObject body, int max) throws Exception {
        if(!path.startsWith("/") || path.startsWith("//")) throw new SecurityException("Ruta inválida");
        HttpsURLConnection c=(HttpsURLConnection)new URL(BASE+path).openConnection();
        // Platform trust store and hostname checks remain enabled; never follow redirects with tokens.
        c.setInstanceFollowRedirects(false); c.setConnectTimeout(15000); c.setReadTimeout(15000);
        c.setRequestProperty("Accept","application/json");
        if(access!=null) c.setRequestProperty("Authorization","Bearer "+access);
        if(lease!=null) {
            c.setRequestProperty("x-coop-session-id",lease.getString("session_id"));
            c.setRequestProperty("x-coop-session-epoch",lease.get("session_epoch").toString());
            c.setRequestProperty("x-coop-client-instance-id",instance);
        }
        try {
            if(body!=null) { c.setRequestMethod("POST"); c.setDoOutput(true); c.setRequestProperty("Content-Type","application/json"); byte[] data=body.toString().getBytes(StandardCharsets.UTF_8); c.setFixedLengthStreamingMode(data.length); try(OutputStream out=c.getOutputStream()){out.write(data);} }
            int code=c.getResponseCode();
            if(code<200 || code>=300) throw new HttpError(code);
            return bounded(c.getInputStream(),max);
        } finally {c.disconnect();}
    }
    JSONObject json(String path,JSONObject body) throws Exception { return new JSONObject(new String(request(path,body,65536),StandardCharsets.UTF_8)); }
    static JSONObject version() throws JSONException {return new JSONObject().put("api_version",1);}
    synchronized void health() throws Exception {request("/health/ready",null,8192);}
    synchronized void register(String user,String pass,String invitation) throws Exception {
        json("/v1/auth/register",version().put("username",user).put("password",pass).put("invitation_code",invitation));
    }
    synchronized void login(String user,String pass) throws Exception {
        if(access!=null) throw new IOException("Cierra la sesión actual primero");
        JSONObject r=json("/v1/auth/login",version().put("username",user).put("password",pass));
        if(r.getInt("api_version")!=1 || r.getLong("access_expires_at")<=System.currentTimeMillis()) throw new SecurityException("Login incompatible o expirado");
        userId=UUID.fromString(r.getString("user_id")).toString();
        character=UUID.fromString(r.getString("character_id")).toString(); access=r.getString("access_token"); refresh=r.getString("refresh_token");
    }
    synchronized void authenticate(String username, String password) throws Exception {
        access=null;userId=null;character=null;
        if(!password.isEmpty()) {
            login(username,password);
        } else {
            String saved=SecureCredentialStore.load(SecureCredentialStore.TOKEN_SERVICE,username);
            if(saved==null) throw new IOException("Sesión guardada no disponible");
            JSONObject response=json("/v1/auth/refresh",version().put("refresh_token",saved));
            if(response.getInt("api_version")!=1 || response.getLong("access_expires_at")<=System.currentTimeMillis())
                throw new SecurityException("Sesión renovada inválida");
            access=response.getString("access_token");
            refresh=response.getString("refresh_token");
        }
        if(!SecureCredentialStore.store(SecureCredentialStore.TOKEN_SERVICE,username,refresh)) throw new IOException("No se pudo guardar la sesión");
    }
    synchronized String userId() throws IOException {if(userId==null)throw new IOException("Identidad no disponible");return userId;}
    synchronized String characterId() throws IOException {if(character==null)throw new IOException("Personaje no disponible");return character;}
    synchronized void clearAccess() { access=null; refresh=null; userId=null; character=null; }
    synchronized void download(String path, File destination, long expectedSize, String expectedHash) throws Exception {
        long maximum=path.endsWith("/apk")?256L*1024*1024:64L*1024*1024;
        if(access==null || !path.startsWith("/v1/releases/") || path.contains("..") || expectedSize<=0 || expectedSize>maximum)
            throw new SecurityException("Descarga inválida");
        HttpsURLConnection connection=(HttpsURLConnection)new URL(BASE+path).openConnection();
        connection.setInstanceFollowRedirects(false);
        connection.setConnectTimeout(15000);
        connection.setReadTimeout(30000);
        connection.setRequestProperty("Authorization","Bearer "+access);
        MessageDigest digest=MessageDigest.getInstance("SHA-256");
        try {
            int code=connection.getResponseCode();
            if(code<200 || code>=300) throw new HttpError(code);
            if(connection.getContentLengthLong()!=expectedSize) throw new IOException("Tamaño de descarga inválido");
            long total=0;
            try(InputStream input=connection.getInputStream(); FileOutputStream output=new FileOutputStream(destination)) {
                byte[] buffer=new byte[65536]; int count;
                while((count=input.read(buffer))!=-1) {
                    total+=count;
                    if(total>expectedSize) throw new IOException("Descarga demasiado grande");
                    digest.update(buffer,0,count);
                    output.write(buffer,0,count);
                }
                output.getFD().sync();
            }
            if(total!=expectedSize || !ReleaseCatalog.hex(digest.digest()).equals(expectedHash)) throw new SecurityException("Descarga no verificada");
        } finally { connection.disconnect(); }
    }
    synchronized long acquire() throws Exception {
        if(access==null || lease!=null) throw new IOException("Estado de sesión inválido");
        JSONObject r=json("/v1/sessions/acquire",version().put("character_id",character).put("client_instance_id",instance).put("idempotency_key",UUID.randomUUID().toString()));
        if(r.getInt("api_version")!=1 || !character.equals(r.getString("character_id")) || !instance.equals(r.getString("client_instance_id")) || r.getLong("session_epoch")<=0 || r.getLong("session_epoch")>0xffffffffL) throw new SecurityException("Lease incompatible");
        lease=r; return r.getLong("current_revision");
    }
    private JSONObject fence() throws Exception {
        if(lease==null) throw new IOException("Sin lease");
        JSONObject f=version(); for(String name:new String[]{"session_id","character_id","current_revision","session_epoch","client_instance_id"}) f.put(name,lease.get(name)); return f;
    }
    synchronized void heartbeat() throws Exception { lease=json("/v1/sessions/heartbeat",fence()); }
    synchronized JSONObject verifyResume(String romHash,String build) throws Exception {
        if(lease==null) throw new IOException("Sin lease");
        String raw=new String(request("/v1/characters/"+character+"/resume-package",null,65536),StandardCharsets.UTF_8);
        return PinnedIdentity.verify(raw,character,lease.getLong("current_revision"),romHash,build);
    }
    synchronized void release() throws Exception {
        if(lease!=null) {json("/v1/sessions/release",fence().put("idempotency_key",UUID.randomUUID().toString())); lease=null;}
    }
    synchronized void logout() throws Exception {
        release();
        if(refresh!=null) {json("/v1/auth/logout",version().put("refresh_token",refresh)); access=null; refresh=null; character=null;}
    }
}
