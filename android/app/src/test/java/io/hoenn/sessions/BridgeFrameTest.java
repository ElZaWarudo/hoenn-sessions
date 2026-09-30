package io.hoenn.sessions;
import org.junit.Test;
import static org.junit.Assert.*;
import java.nio.*;

public class BridgeFrameTest {
    @Test public void acceptsGroupTravelInItsDefinedDirection() {
        assertEquals(15,BridgeFrame.decode(BridgeFrame.encode(15,1,42,new byte[0]),false).type);
        assertEquals(0x10e,BridgeFrame.decode(BridgeFrame.encode(0x10e,1,42,new byte[0]),true).type);
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.decode(BridgeFrame.encode(15,1,42,new byte[0]),true));
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x18,1,42,new byte[0]));
        assertThrows(IllegalArgumentException.class,()->BridgeFrame.encode(0x11c,1,42,new byte[0]));
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
}
