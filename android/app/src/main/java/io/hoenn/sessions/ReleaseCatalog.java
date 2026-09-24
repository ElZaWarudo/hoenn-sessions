package io.hoenn.sessions;

import java.io.*;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Base64;
import org.json.*;

/** Verifies signed game releases; installed Windows envelopes remain readable for migration. */
final class ReleaseCatalog {
    static final class Artifact {
        final long size;
        final String sha256;
        Artifact(long size, String sha256) { this.size=size; this.sha256=sha256; }
    }
    static final class Release {
        final String id;
        final long sequence;
        final Artifact rom, manifest;
        Release(String id,long sequence,Artifact rom,Artifact manifest) {
            this.id=id;this.sequence=sequence;this.rom=rom;this.manifest=manifest;
        }
    }
    private ReleaseCatalog() { }
    static String hex(byte[] bytes) {
        StringBuilder out=new StringBuilder(bytes.length*2);
        for(byte value:bytes) out.append(String.format("%02x",value&255));
        return out.toString();
    }
    static boolean safeId(String value) {
        return value!=null && value.length()>0 && value.length()<=128 && !value.equals(".") && !value.equals("..")
            && value.matches("[A-Za-z0-9._-]+");
    }
    static Release verify(byte[] bytes,String keyId,String publicKey) throws Exception {
        return verify(bytes,keyId,publicKey,true,false);
    }
    static Release verify(byte[] bytes,String keyId,String publicKey,boolean requireFresh) throws Exception {
        return verify(bytes,keyId,publicKey,requireFresh,false);
    }
    static Release verifyLegacy(byte[] bytes,String keyId,String publicKey,boolean requireFresh) throws Exception {
        return verify(bytes,keyId,publicKey,requireFresh,true);
    }
    private static Release verify(byte[] bytes,String keyId,String publicKey,boolean requireFresh,boolean legacy) throws Exception {
        if(bytes.length==0 || bytes.length>65536 || keyId.isEmpty() || publicKey.length()!=64)
            throw new SecurityException("Firma de versión no configurada");
        JSONObject envelope=new JSONObject(new String(bytes,StandardCharsets.UTF_8));
        if(envelope.length()!=4 || envelope.getInt("schema")!=1 || !keyId.equals(envelope.getString("key_id")))
            throw new SecurityException("Sobre de versión inválido");
        byte[] payload=Base64.getDecoder().decode(envelope.getString("payload"));
        byte[] signature=Base64.getDecoder().decode(envelope.getString("signature"));
        if(payload.length==0 || payload.length>32768 || !PinnedIdentity.verifySignature(PinnedIdentity.hex(publicKey),payload,signature))
            throw new SecurityException("Firma de versión inválida");
        JSONObject descriptor=new JSONObject(new String(payload,StandardCharsets.UTF_8));
        String id=descriptor.getString("release_id");
        long now=System.currentTimeMillis()/1000;
        long issued=descriptor.getLong("issued_at"), expires=descriptor.getLong("expires_at");
        if(descriptor.length()!=7 || descriptor.getInt("schema")!=1 || !(legacy?"windows-x86_64":"game").equals(descriptor.getString("platform"))
            || !safeId(id) || descriptor.getLong("sequence")<1 || issued>now+300 || (requireFresh && expires<=now) || expires<=issued
            || expires-issued>90L*24*60*60)
            throw new SecurityException("Versión incompatible o expirada");
        Artifact rom=null,manifest=null;
        JSONArray artifacts=descriptor.getJSONArray("artifacts");
        if(artifacts.length()!=(legacy?11:2)) throw new SecurityException("Lista de archivos incompleta");
        for(int i=0;i<artifacts.length();i++) {
            JSONObject artifact=artifacts.getJSONObject(i);
            if(artifact.length()!=3) throw new SecurityException("Archivo de versión inválido");
            String name=artifact.getString("id");
            if(!legacy && !name.equals("rom") && !name.equals("compatibility-manifest"))
                throw new SecurityException("Archivo de versión inválido");
            if(name.equals("rom") || name.equals("compatibility-manifest")) {
                long size=artifact.getLong("size");
                String hash=artifact.getString("sha256");
                if(size<=0 || size>(name.equals("rom")?64L*1024*1024:1024*1024) || !hash.matches("[0-9a-f]{64}"))
                    throw new SecurityException("Archivo de versión inválido");
                if(name.equals("rom")) {if(rom!=null) throw new SecurityException("ROM duplicada");rom=new Artifact(size,hash);}
                else {if(manifest!=null) throw new SecurityException("Manifiesto duplicado");manifest=new Artifact(size,hash);}
            }
        }
        if(rom==null || manifest==null) throw new SecurityException("Versión sin ROM o manifiesto");
        return new Release(id,descriptor.getLong("sequence"),rom,manifest);
    }
    static void verifyFile(File file,Artifact expected) throws Exception {
        if(!file.isFile() || file.length()!=expected.size) throw new SecurityException("Archivo de versión incompleto");
        MessageDigest digest=MessageDigest.getInstance("SHA-256");
        try(InputStream input=new FileInputStream(file)) {
            byte[] buffer=new byte[65536];int n;
            while((n=input.read(buffer))!=-1) digest.update(buffer,0,n);
        }
        if(!hex(digest.digest()).equals(expected.sha256)) throw new SecurityException("Archivo de versión modificado");
    }
    static JSONObject verifiedManifest(File file,String romHash) throws Exception {
        byte[] bytes=CloudApi.bounded(new FileInputStream(file),1024*1024);
        JSONObject manifest=new JSONObject(new String(bytes,StandardCharsets.UTF_8));
        if(manifest.getInt("schema_version")!=4 || !romHash.equals(manifest.getJSONObject("game_build").getString("rom_sha256"))
            || manifest.getJSONObject("net_bridge").getInt("abi_version")!=1
            || manifest.getJSONObject("net_bridge").getInt("game_protocol_version")!=1)
            throw new SecurityException("ROM y manifiesto incompatibles");
        return manifest;
    }
}
