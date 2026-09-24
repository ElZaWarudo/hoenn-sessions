package io.hoenn.sessions;

import java.nio.charset.StandardCharsets;
import java.util.Base64;
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
        byte[] payload=descriptor.toString().getBytes(StandardCharsets.UTF_8);
        Ed25519Signer signer=new Ed25519Signer();signer.init(true,signing);signer.update(payload,0,payload.length);
        byte[] signature=signer.generateSignature();
        return new JSONObject().put("schema",1).put("key_id","test")
            .put("payload",Base64.getEncoder().encodeToString(payload))
            .put("signature",Base64.getEncoder().encodeToString(signature))
            .toString().getBytes(StandardCharsets.UTF_8);
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
}
