package io.hoenn.sessions;

import org.json.*;
import org.erdtman.jcs.JsonCanonicalizer;
import org.bouncycastle.math.ec.rfc8032.Ed25519;
import java.nio.charset.StandardCharsets;

final class PinnedIdentity {
    static final String KEY_ID = "pilot-v1";
    static final String PUBLIC_KEY = "f239614d143272c416c185e4d52d95a883f7ad89cadf55e1cf1dbc9dda8fe188";
    static byte[] hex(String s) {
        if (!s.matches("[0-9a-f]+") || (s.length() & 1) != 0) throw new SecurityException("Hex inválido");
        byte[] b = new byte[s.length()/2];
        for (int i=0; i<b.length; i++) b[i]=(byte)Integer.parseInt(s.substring(i*2,i*2+2),16);
        return b;
    }
    static JSONObject verify(String raw, String character, long revision, String romHash, String build) throws Exception {
        // Parse through JCS first: rejects duplicate keys before JSONObject can discard them.
        new JsonCanonicalizer(raw).getEncodedUTF8();
        JSONObject e=new JSONObject(raw);
        if (e.length()!=5 || e.getInt("package_version")!=1 || e.getInt("signing_algorithm_version")!=1 || !KEY_ID.equals(e.getString("signing_key_id"))) throw new SecurityException("Identidad no admitida");
        JSONObject m=e.getJSONObject("manifest");
        JSONArray sig=e.getJSONArray("signature");
        if(sig.length()!=64) throw new SecurityException("Firma inválida");
        byte[] signature=new byte[64];
        for(int i=0;i<64;i++) { Object value=sig.get(i); if(!(value instanceof Integer) || (int)value<0 || (int)value>255) throw new SecurityException("Firma inválida"); signature[i]=(byte)(int)value; }
        byte[] bytes=new JsonCanonicalizer(m.toString()).getEncodedUTF8();
        if (!verifySignature(hex(PUBLIC_KEY), bytes, signature)) throw new SecurityException("Firma inválida");
        if(m.length()!=17 || m.getInt("package_version")!=1 || !character.equals(m.getString("character_id")) || m.getLong("revision")!=revision || revision<1 || m.getLong("parent_revision")!=revision-1 || !romHash.equals(m.getString("rom_sha256")) || !build.equals(m.getString("game_build_id")) || m.getInt("bridge_abi")!=1 || m.getInt("protocol_version")!=1) throw new SecurityException("Paquete incompatible");
        return m;
    }
    static boolean verifySignature(byte[] key, byte[] message, byte[] signature) {
        return key.length==32 && signature.length==64 && Ed25519.validatePublicKeyFull(key,0) && Ed25519.validatePublicKeyFull(signature,0) && Ed25519.verify(signature,0,key,0,message,0,message.length);
    }
}
