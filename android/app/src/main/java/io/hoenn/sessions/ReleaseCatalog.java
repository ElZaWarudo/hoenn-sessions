package io.hoenn.sessions;

import java.io.*;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Base64;
import java.util.LinkedHashMap;
import java.util.Map;
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
        final Artifact regionCatalog;
        final Map<Integer,World> worlds;
        Release(String id,long sequence,Artifact rom,Artifact manifest,Artifact regionCatalog,Map<Integer,World> worlds) {
            this.id=id;this.sequence=sequence;this.rom=rom;this.manifest=manifest;
            this.regionCatalog=regionCatalog;this.worlds=worlds;
        }
    }
    static final class World {
        final int id;
        final Artifact rom, manifest, playerTransfer;
        World(int id,Artifact rom,Artifact manifest,Artifact playerTransfer) {
            this.id=id;this.rom=rom;this.manifest=manifest;this.playerTransfer=playerTransfer;
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
        Artifact rom=null,manifest=null,regionCatalog=null;
        Map<Integer,Artifact[]> worldParts=new LinkedHashMap<>();
        JSONArray artifacts=descriptor.getJSONArray("artifacts");
        if(artifacts.length()<(legacy?11:2) || artifacts.length()>(legacy?11:51))
            throw new SecurityException("Lista de archivos incompleta");
        for(int i=0;i<artifacts.length();i++) {
            JSONObject artifact=artifacts.getJSONObject(i);
            if(artifact.length()!=3) throw new SecurityException("Archivo de versión inválido");
            String name=artifact.getString("id");
            int worldId=0,kind=-1;
            if(name.startsWith("world-")) {
                String[] parts=name.split("-",4);
                if(parts.length!=3 && parts.length!=4) throw new SecurityException("Mundo inválido");
                try {worldId=Integer.parseInt(parts[1]);} catch(NumberFormatException error) {throw new SecurityException("Mundo inválido",error);}
                if(worldId<1 || worldId>65535 || !parts[1].equals(Integer.toString(worldId)))
                    throw new SecurityException("Mundo inválido");
                String type=parts.length==3?parts[2]:parts[2]+"-"+parts[3];
                kind=type.equals("rom")?0:type.equals("compatibility")?1:type.equals("player-transfer")?2:-1;
            }
            if(!legacy && !name.equals("rom") && !name.equals("compatibility-manifest")
                && !name.equals("region-catalog") && kind<0)
                throw new SecurityException("Archivo de versión inválido");
            if(name.equals("rom") || name.equals("compatibility-manifest") || name.equals("region-catalog") || kind>=0) {
                long size=artifact.getLong("size");
                String hash=artifact.getString("sha256");
                long max=name.equals("rom") || kind==0?64L*1024*1024:name.equals("region-catalog")?256*1024:1024*1024;
                if(size<=0 || size>max || !hash.matches("[0-9a-f]{64}"))
                    throw new SecurityException("Archivo de versión inválido");
                if(name.equals("rom")) {if(rom!=null) throw new SecurityException("ROM duplicada");rom=new Artifact(size,hash);}
                else if(name.equals("compatibility-manifest")) {if(manifest!=null) throw new SecurityException("Manifiesto duplicado");manifest=new Artifact(size,hash);}
                else if(name.equals("region-catalog")) {if(regionCatalog!=null) throw new SecurityException("Catálogo duplicado");regionCatalog=new Artifact(size,hash);}
                else {
                    Artifact[] parts=worldParts.computeIfAbsent(worldId,key->new Artifact[3]);
                    if(parts[kind]!=null) throw new SecurityException("Archivo de mundo duplicado");
                    parts[kind]=new Artifact(size,hash);
                }
            }
        }
        if(rom==null || manifest==null) throw new SecurityException("Versión sin ROM o manifiesto");
        if((regionCatalog==null)!=(worldParts.isEmpty()) || worldParts.size()>16)
            throw new SecurityException("Catálogo de mundos incompleto");
        Map<Integer,World> worlds=new LinkedHashMap<>();
        for(Map.Entry<Integer,Artifact[]> entry:worldParts.entrySet()) {
            Artifact[] parts=entry.getValue();
            if(parts[0]==null || parts[1]==null || parts[2]==null)
                throw new SecurityException("Mundo incompleto");
            worlds.put(entry.getKey(),new World(entry.getKey(),parts[0],parts[1],parts[2]));
        }
        if(!worlds.isEmpty()) {
            World main=worlds.get(1);
            if(main==null || !main.rom.sha256.equals(rom.sha256) || main.rom.size!=rom.size
                || !main.manifest.sha256.equals(manifest.sha256) || main.manifest.size!=manifest.size)
                throw new SecurityException("Mundo principal incompatible");
        }
        return new Release(id,descriptor.getLong("sequence"),rom,manifest,regionCatalog,worlds);
    }
    static void verifiedWorldCatalog(File file,Release release) throws Exception {
        if(release.regionCatalog==null) return;
        verifyFile(file,release.regionCatalog);
        JSONObject catalog=new JSONObject(new String(CloudApi.bounded(new FileInputStream(file),256*1024),StandardCharsets.UTF_8));
        JSONArray entries=catalog.getJSONArray("worlds");
        if(catalog.getInt("schema_version")!=1 || entries.length()!=release.worlds.size() || entries.length()==0)
            throw new SecurityException("Catálogo de mundos inválido");
        java.util.HashSet<Integer> seen=new java.util.HashSet<>();
        for(int i=0;i<entries.length();i++) {
            JSONObject entry=entries.getJSONObject(i);
            int id=entry.getInt("world_id");
            World world=release.worlds.get(id);
            if(world==null || !seen.add(id)) throw new SecurityException("Mundo desconocido");
            String prefix="worlds/"+id+"/";
            if(!entry.getString("rom_path").equals(prefix+"game.gba")
                || !entry.getString("bridge_path").equals(prefix+"bridge_manifest.json")
                || !entry.getString("player_transfer_path").equals(prefix+"player_transfer.json")
                || !entry.getString("rom_sha256").equals(world.rom.sha256)
                || !entry.getString("bridge_sha256").equals(world.manifest.sha256)
                || !entry.getString("player_transfer_sha256").equals(world.playerTransfer.sha256))
                throw new SecurityException("Catálogo y archivos de mundo incompatibles");
        }
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
