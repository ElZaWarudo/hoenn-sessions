package io.hoenn.sessions;

import java.nio.*;
import java.util.Arrays;

/** JNI replacement for memory.lua. Inert until an owner with a server-issued epoch calls it.
 * The preview does NOT yet connect this boundary to the Rust sidecar's cloud state machine.
 */
final class NativeBridge {
    static final int ADDRESS=BuildConfig.BRIDGE_ADDRESS;
    static void validate(byte[] memory) {
        if(memory==null || memory.length!=9244)throw new IllegalStateException("Bridge no disponible");
        ByteBuffer b=ByteBuffer.wrap(memory).order(ByteOrder.LITTLE_ENDIAN);
        if(b.getInt()!=1347109711 || b.getShort()!=1 || b.getShort()!=1 || b.getInt()!=65536)throw new SecurityException("ABI de ROM incompatible");
    }
    static byte[] peekOutbound() {
        synchronized(NativeCore.class) {
            byte[] memory=NativeCore.readBridge(ADDRESS);validate(memory);
            ByteBuffer b=ByteBuffer.wrap(memory).order(ByteOrder.LITTLE_ENDIAN);
            int read=b.getShort(20)&65535,write=b.getShort(22)&65535,depth=(write-read)&65535;
            if(depth>32) {NativeCore.bridgeCounter(false,read,write);throw new IllegalStateException("Cola corrupta descartada");}
            if(depth==0)return null;
            byte[] frame=Arrays.copyOfRange(memory,24+(read&31)*144,24+(read&31)*144+144);
            try{BridgeFrame.decode(frame,false);}catch(IllegalArgumentException e){NativeCore.bridgeCounter(false,read,(read+1)&65535);throw e;}
            return frame;
        }
    }
    static boolean commitOutbound(byte[] expected) {
        if(expected==null)return false;
        synchronized(NativeCore.class) {
            if(!Arrays.equals(expected,peekOutbound()))return false;
            ByteBuffer b=ByteBuffer.wrap(NativeCore.readBridge(ADDRESS)).order(ByteOrder.LITTLE_ENDIAN);
            int read=b.getShort(20)&65535;
            return NativeCore.bridgeCounter(false,read,(read+1)&65535);
        }
    }
    static boolean pushInbound(byte[] frame,long serverEpoch) {
        if(serverEpoch<=0 || serverEpoch>0xffffffffL || BridgeFrame.decode(frame,true).epoch!=serverEpoch)throw new SecurityException("Epoch del bridge inválido");
        synchronized(NativeCore.class) {validate(NativeCore.readBridge(ADDRESS));return NativeCore.bridgePush(frame);}
    }
}
