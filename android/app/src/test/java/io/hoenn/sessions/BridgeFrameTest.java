package io.hoenn.sessions;
import org.junit.Test;
import static org.junit.Assert.*;
import java.nio.*;

public class BridgeFrameTest {
    private static byte[] proofPayload(){
        byte[] payload=new byte[64];payload[0]=1;payload[16]=2;payload[48]=2;payload[52]=1;return payload;
    }
    @Test public void travelTypesAreStrictAndDoNotAliasRealtime(){
        assertEquals(0x18,BridgeFrame.decode(BridgeFrame.encode(0x18,1,42,"to_cormoria".getBytes(java.nio.charset.StandardCharsets.US_ASCII)),false).type);
        for(byte[] portal:new byte[][]{new byte[0],new byte[97],"To_cormoria".getBytes(),"to-cormoria".getBytes(),new byte[]{(byte)0xff}})assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x18,1,42,portal));
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.decode(BridgeFrame.encode(0x19,1,0,proofPayload()),true));
        for(int offset:new int[]{0,16,48,52}){byte[] proof=proofPayload();proof[offset]=0;assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x19,1,0,proof));}
        byte[] reserved=proofPayload();reserved[58]=1;assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x19,1,0,reserved));
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x19,1,0,new byte[63]));
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x11c,1,0,new byte[16]));
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x11c,1,0,new byte[15]));
        for(int type:new int[]{0x12,0x13,0x14,0x15,0x16,0x17}){
            BridgeFrame normal=BridgeFrame.decode(BridgeFrame.encode(type,1,42,new byte[0]),false);
            BridgeConnection.checkFrame(normal,42,false,false);
            BridgeFrame offline=BridgeFrame.decode(BridgeFrame.encode(type,1,0,new byte[0]),false);
            assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(offline,0,false,true));
        }
        BridgeFrame remote=BridgeFrame.decode(BridgeFrame.encode(0x111,1,42,new byte[0]),true);
        BridgeConnection.checkFrame(remote,42,true,false);
        assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(BridgeFrame.decode(BridgeFrame.encode(0x111,1,0,new byte[0]),true),0,true,true));
    }
    @Test public void rejectsNonzeroFramePaddingEvenWithValidCrc(){
        byte[] frame=BridgeFrame.encode(0x13,1,42,new byte[0]);frame[139]=1;
        java.util.zip.CRC32 crc=new java.util.zip.CRC32();crc.update(frame,0,140);ByteBuffer.wrap(frame).order(ByteOrder.LITTLE_ENDIAN).putInt(140,(int)crc.getValue());
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.decode(frame,false));
    }
    @Test public void gameplayAdmitsOnlyExactZeroEpochBootReady(){
        BridgeFrame boot=BridgeFrame.decode(BridgeFrame.encode(1,1,0,new byte[0]),false);
        BridgeConnection.checkFrame(boot,42,false,false);
        assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(boot,42,true,false));
        assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(boot,0,false,false));
        for(BridgeFrame bad:new BridgeFrame[]{
                BridgeFrame.decode(BridgeFrame.encode(1,2,0,new byte[0]),false),
                BridgeFrame.decode(BridgeFrame.encode(1,1,0,new byte[]{1}),false),
                BridgeFrame.decode(BridgeFrame.encode(2,1,0,new byte[0]),false),
                BridgeFrame.decode(BridgeFrame.encode(1,1,41,new byte[0]),false)}){
            assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(bad,42,false,false));
        }
        BridgeConnection.checkFrame(BridgeFrame.decode(BridgeFrame.encode(1,1,42,new byte[0]),false),42,false,false);
        BridgeConnection.checkFrame(boot,0,false,true);
    }
    @Test public void acceptsGroupTravelInItsDefinedDirection() {
        assertEquals(15,BridgeFrame.decode(BridgeFrame.encode(15,1,42,new byte[0]),false).type);
        assertEquals(0x10e,BridgeFrame.decode(BridgeFrame.encode(0x10e,1,42,new byte[0]),true).type);
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.decode(BridgeFrame.encode(15,1,42,new byte[0]),true));
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x1a,1,42,new byte[0]));
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x11d,1,42,new byte[0]));
    }
    @Test public void carriesRealtimeCoopMessagesBothWays() {
        // Companion state (0x10) through trade offer decisions (0x17) leave
        // the ROM; remote companions (0x10f) through trade offer statuses
        // (0x11b) enter it.
        for(int type:new int[]{0x10,0x11,0x13,0x15,0x16,0x17})assertEquals(type,BridgeFrame.decode(BridgeFrame.encode(type,1,42,new byte[0]),false).type);
        for(int type:new int[]{0x10f,0x111,0x114,0x118,0x119,0x11a,0x11b})assertEquals(type,BridgeFrame.decode(BridgeFrame.encode(type,1,42,new byte[0]),true).type);
        // The trade offer types keep their direction.
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.decode(BridgeFrame.encode(0x16,1,42,new byte[0]),true));
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.decode(BridgeFrame.encode(0x11a,1,42,new byte[0]),false));
    }
    @Test public void crcAndDirectionFailClosed() {
        byte[] f=BridgeFrame.encode(1,1,0,new byte[]{1,2,3});
        assertArrayEquals(new byte[]{1,2,3},BridgeFrame.decode(f,false).payload);
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.decode(f,true));
        f[40]^=1;assertThrows(IllegalArgumentException.class,()->BridgeFrame.decode(f,false));
    }
    @Test public void rejectsBadEpochAndZeroSequence() {
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(1,0,0,new byte[0]));
        byte[] f=BridgeFrame.encode(0x100,1,42,new byte[0]);
        assertThrows(SecurityException.class,()->NativeBridge.pushInbound(f,43));
    }
    @Test public void rejectsManifestHeaderDrift() {
        // Game protocol 5: friendly battle rules in the reserve, offer and manifest.
        byte[] memory=new byte[24];ByteBuffer.wrap(memory).order(ByteOrder.LITTLE_ENDIAN).putInt(1347109711).putShort((short)1).putShort((short)5).putInt(65536);
        NativeBridge.validate(memory);memory[6]=4;
        assertThrows(SecurityException.class,()->NativeBridge.validate(memory));memory[6]=5;memory[8]^=1;
        assertThrows(SecurityException.class,()->NativeBridge.validate(memory));
    }
    @Test public void arrivalFramesAreLimitedToOfflineVerifier() {
        BridgeFrame ready=BridgeFrame.decode(BridgeFrame.encode(1,1,0,new byte[0]),false);
        BridgeFrame proof=BridgeFrame.decode(BridgeFrame.encode(0x19,2,0,proofPayload()),false);
        BridgeFrame challenge=BridgeFrame.decode(BridgeFrame.encode(0x11c,1,0,new byte[]{1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0}),true);
        BridgeConnection.checkFrame(ready,0,false,true);
        BridgeConnection.checkFrame(proof,0,false,true);
        BridgeConnection.checkFrame(challenge,0,true,true);
        assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(ready,0,false,false));
        assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(proof,7,false,false));
        assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(challenge,7,true,false));
        assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(BridgeFrame.decode(BridgeFrame.encode(0x100,1,0,new byte[0]),true),0,true,true));
        assertThrows(SecurityException.class,()->BridgeConnection.checkFrame(BridgeFrame.decode(BridgeFrame.encode(13,1,0,new byte[4]),false),0,false,true));
    }
}
