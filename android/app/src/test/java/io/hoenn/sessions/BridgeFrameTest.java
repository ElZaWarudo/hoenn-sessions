package io.hoenn.sessions;
import org.junit.Test;
import static org.junit.Assert.*;
import java.nio.*;

public class BridgeFrameTest {
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
        byte[] memory=new byte[9244];ByteBuffer.wrap(memory).order(ByteOrder.LITTLE_ENDIAN).putInt(1347109711).putShort((short)1).putShort((short)1).putInt(65536);
        NativeBridge.validate(memory);memory[8]^=1;
        assertThrows(SecurityException.class,()->NativeBridge.validate(memory));
    }
}
