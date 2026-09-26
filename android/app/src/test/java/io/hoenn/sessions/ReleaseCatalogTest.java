package io.hoenn.sessions;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.util.Base64;
import java.util.LinkedHashMap;
import org.bouncycastle.crypto.params.Ed25519PrivateKeyParameters;
import org.bouncycastle.crypto.signers.Ed25519Signer;
import org.json.*;
import org.junit.Test;
import static org.junit.Assert.*;

public class ReleaseCatalogTest {
    private final Ed25519PrivateKeyParameters signing=new Ed25519PrivateKeyParameters(new byte[32],0);

    private byte[] envelope(boolean includeManifest, long expiry) throws Exception {
        return envelope(includeManifest,expiry,false);
    }
    private byte[] envelope(boolean includeManifest, long expiry,boolean legacy) throws Exception {
        JSONArray artifacts=new JSONArray();
        for(int i=0;i<(legacy?11:2);i++) {
            String id=i==0?"rom":i==1?"compatibility-manifest":"other-"+i;
            if(!includeManifest && i==1) id="other-manifest";
            artifacts.put(new JSONObject().put("id",id).put("size",1024).put("sha256","a".repeat(64)));
        }
        long now=System.currentTimeMillis()/1000;
        JSONObject descriptor=new JSONObject().put("schema",1).put("release_id","release-123")
            .put("sequence",7).put("issued_at",now-60).put("expires_at",expiry)
            .put("platform",legacy?"windows-x86_64":"game").put("artifacts",artifacts);
        return sign(descriptor);
    }
    private byte[] sign(JSONObject descriptor) throws Exception {
        byte[] payload=descriptor.toString().getBytes(StandardCharsets.UTF_8);
        Ed25519Signer signer=new Ed25519Signer();signer.init(true,signing);signer.update(payload,0,payload.length);
        byte[] signature=signer.generateSignature();
        return new JSONObject().put("schema",1).put("key_id","test")
            .put("payload",Base64.getEncoder().encodeToString(payload))
            .put("signature",Base64.getEncoder().encodeToString(signature))
            .toString().getBytes(StandardCharsets.UTF_8);
    }
    private JSONObject artifact(String id,String hash) throws Exception {
        return new JSONObject().put("id",id).put("size",1024).put("sha256",hash);
    }
    private byte[] multiworldEnvelope(boolean missingPart) throws Exception {
        JSONArray artifacts=new JSONArray().put(artifact("rom","a".repeat(64)))
            .put(artifact("compatibility-manifest","b".repeat(64)))
            .put(artifact("region-catalog","c".repeat(64)));
        for(int id:new int[]{1,2}) {
            artifacts.put(artifact("world-"+id+"-rom",(id==1?"a":"d").repeat(64)))
                .put(artifact("world-"+id+"-compatibility",(id==1?"b":"e").repeat(64)));
            if(!missingPart || id!=2)
                artifacts.put(artifact("world-"+id+"-player-transfer","f".repeat(64)));
        }
        long now=System.currentTimeMillis()/1000;
        return sign(new JSONObject().put("schema",1).put("release_id","multiworld-123")
            .put("sequence",8).put("issued_at",now-60).put("expires_at",now+3600)
            .put("platform","game").put("artifacts",artifacts));
    }

    @Test public void acceptsSignedRomAndManifestAndRejectsMissingManifest() throws Exception {
        long expiry=System.currentTimeMillis()/1000+3600;
        String key=ReleaseCatalog.hex(signing.generatePublicKey().getEncoded());
        ReleaseCatalog.Release result=ReleaseCatalog.verify(envelope(true,expiry),"test",key);
        assertEquals("release-123",result.id);
        assertEquals(7,result.sequence);
        try {ReleaseCatalog.verify(envelope(false,expiry),"test",key);fail();}
        catch(SecurityException expected) { }
    }

    @Test public void rejectsWrongKeyAndExpiredRelease() throws Exception {
        long expiry=System.currentTimeMillis()/1000+3600;
        String key=ReleaseCatalog.hex(signing.generatePublicKey().getEncoded());
        try {ReleaseCatalog.verify(envelope(true,expiry),"wrong",key);fail();}
        catch(SecurityException expected) { }
        try {ReleaseCatalog.verify(envelope(true,System.currentTimeMillis()/1000-1),"test",key);fail();}
        catch(SecurityException expected) { }
    }

    @Test public void legacyWindowsEnvelopeOnlyLoadsViaMigrationPath() throws Exception {
        long expiry=System.currentTimeMillis()/1000+3600;
        String key=ReleaseCatalog.hex(signing.generatePublicKey().getEncoded());
        try {ReleaseCatalog.verify(envelope(true,expiry,true),"test",key);fail();}
        catch(SecurityException expected) { }
        assertEquals(7,ReleaseCatalog.verifyLegacy(envelope(true,expiry,true),"test",key,true).sequence);
    }

    @Test public void acceptsCompleteSignedWorldGroupsAndRejectsPartialGroups() throws Exception {
        String key=ReleaseCatalog.hex(signing.generatePublicKey().getEncoded());
        ReleaseCatalog.Release release=ReleaseCatalog.verify(multiworldEnvelope(false),"test",key);
        assertEquals(2,release.worlds.size());
        assertEquals("d".repeat(64),release.worlds.get(2).rom.sha256);
        try {ReleaseCatalog.verify(multiworldEnvelope(true),"test",key);fail();}
        catch(SecurityException expected) { }
    }

    @Test public void catalogMustBindEachWorldToSignedPathsAndHashes() throws Exception {
        LinkedHashMap<Integer,ReleaseCatalog.World> worlds=new LinkedHashMap<>();
        ReleaseCatalog.Artifact rom=new ReleaseCatalog.Artifact(1024,"d".repeat(64));
        ReleaseCatalog.Artifact bridge=new ReleaseCatalog.Artifact(1024,"e".repeat(64));
        ReleaseCatalog.Artifact transfer=new ReleaseCatalog.Artifact(1024,"f".repeat(64));
        worlds.put(1,new ReleaseCatalog.World(1,rom,bridge,transfer));
        JSONObject world=new JSONObject().put("world_id",1)
            .put("rom_path","worlds/1/game.gba").put("rom_sha256",rom.sha256)
            .put("bridge_path","worlds/1/bridge_manifest.json").put("bridge_sha256",bridge.sha256)
            .put("player_transfer_path","worlds/1/player_transfer.json").put("player_transfer_sha256",transfer.sha256);
        JSONObject catalog=new JSONObject().put("schema_version",1).put("worlds",new JSONArray().put(world));
        java.io.File file=Files.createTempFile("world-catalog-",".json").toFile();
        try {
            byte[] bytes=catalog.toString().getBytes(StandardCharsets.UTF_8);
            Files.write(file.toPath(),bytes);
            String digest=ReleaseCatalog.hex(MessageDigest.getInstance("SHA-256").digest(bytes));
            ReleaseCatalog.Release release=new ReleaseCatalog.Release("test",1,rom,bridge,
                new ReleaseCatalog.Artifact(bytes.length,digest),worlds);
            ReleaseCatalog.verifiedWorldCatalog(file,release);
            world.put("rom_path","worlds/2/game.gba");
            bytes=catalog.toString().getBytes(StandardCharsets.UTF_8);
            Files.write(file.toPath(),bytes);
            digest=ReleaseCatalog.hex(MessageDigest.getInstance("SHA-256").digest(bytes));
            ReleaseCatalog.Release tampered=new ReleaseCatalog.Release("test",1,rom,bridge,
                new ReleaseCatalog.Artifact(bytes.length,digest),worlds);
            try {ReleaseCatalog.verifiedWorldCatalog(file,tampered);fail();}
            catch(SecurityException expected) { }
        } finally {file.delete();}
    }
}
