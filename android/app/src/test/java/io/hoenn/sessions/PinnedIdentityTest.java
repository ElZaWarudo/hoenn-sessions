package io.hoenn.sessions;

import org.junit.Test;
import static org.junit.Assert.*;

public class PinnedIdentityTest {
    @Test public void verifiesRfc8032VectorAndRejectsTampering() {
        byte[] key=PinnedIdentity.hex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
        byte[] sig=PinnedIdentity.hex("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
        assertTrue(PinnedIdentity.verifySignature(key,new byte[0],sig));
        assertFalse(PinnedIdentity.verifySignature(PinnedIdentity.hex(PinnedIdentity.PUBLIC_KEY),new byte[0],sig));
        assertFalse(PinnedIdentity.verifySignature(key,new byte[]{1},sig));
        sig[0]^=1;assertFalse(PinnedIdentity.verifySignature(key,new byte[0],sig));
    }
    @Test public void rejectsWeakKeys() {
        byte[] identity=new byte[32];identity[0]=1;
        assertFalse(PinnedIdentity.verifySignature(identity,new byte[0],new byte[64]));
    }
    @Test public void rejectsUntrustedEnvelope() throws Exception {
        try {PinnedIdentity.verify("{\"package_version\":1,\"signing_algorithm_version\":1,\"signing_key_id\":\"attacker\",\"manifest\":{},\"signature\":[]}","test",1);fail();}catch(SecurityException expected){}
    }
}
