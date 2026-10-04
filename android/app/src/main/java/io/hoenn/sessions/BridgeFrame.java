package io.hoenn.sessions;

import java.nio.*;
import java.util.Arrays;
import java.util.zip.CRC32;

/** The same fixed 144-byte ABI as bridge/protocol.lua. No account secrets cross this ABI. */
final class BridgeFrame {
    /** Must equal coop-sidecar's codec; a Rust test reads these four lines. */
    static final int BRIDGE_ABI=1;
    static final int PROTOCOL_VERSION=5;
    static final int LAST_OUTBOUND_TYPE=0x19;
    static final int LAST_INBOUND_TYPE=0x11c;
    final int type;
    final long sequence, epoch;
    final byte[] payload;
    private BridgeFrame(int type,long sequence,long epoch,byte[] payload) {this.type=type;this.sequence=sequence;this.epoch=epoch;this.payload=payload;}
    static BridgeFrame decode(byte[] bytes,boolean inbound) {
        if(bytes==null || bytes.length!=144)throw new IllegalArgumentException("Tamaño del bridge inválido");
        ByteBuffer b=ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        int type=b.getShort()&65535,length=b.getShort()&65535;
        long sequence=Integer.toUnsignedLong(b.getInt()),epoch=Integer.toUnsignedLong(b.getInt());
        if(length>128 || sequence==0 || (inbound?(type<0x100 || type>LAST_INBOUND_TYPE):(type<1 || type>LAST_OUTBOUND_TYPE)))throw new IllegalArgumentException("Cabecera del bridge inválida");
        CRC32 crc=new CRC32();crc.update(bytes,0,140);
        if(crc.getValue()!=Integer.toUnsignedLong(b.getInt(140)))throw new IllegalArgumentException("CRC del bridge inválido");
        for(int i=12+length;i<140;i++)if(bytes[i]!=0)throw new IllegalArgumentException("Padding del bridge inválido");
        byte[] payload=Arrays.copyOfRange(bytes,12,12+length);
        validateTravelPayload(type,payload);
        return new BridgeFrame(type,sequence,epoch,payload);
    }
    private static boolean nonzero(byte[] payload,int start,int end) {
        for(int i=start;i<end;i++)if(payload[i]!=0)return true;
        return false;
    }
    private static void validateTravelPayload(int type,byte[] payload) {
        if(type==0x18){
            if(payload.length<1 || payload.length>96 || payload[0]<'a' || payload[0]>'z')throw new IllegalArgumentException("Portal inválido");
            for(byte value:payload)if(!((value>='a' && value<='z') || (value>='0' && value<='9') || value=='_'))throw new IllegalArgumentException("Portal inválido");
        }else if(type==0x19){
            if(payload.length!=64 || !nonzero(payload,0,16) || !nonzero(payload,16,48) || !nonzero(payload,48,52) || !nonzero(payload,52,56))throw new IllegalArgumentException("Prueba de llegada inválida");
            for(int i=58;i<64;i++)if(payload[i]!=0)throw new IllegalArgumentException("Padding de llegada inválido");
        }else if(type==0x11c && (payload.length!=16 || !nonzero(payload,0,16)))throw new IllegalArgumentException("Reto de llegada inválido");
    }
    static byte[] encode(int type,long sequence,long epoch,byte[] payload) {
        if(!((type>=1 && type<=LAST_OUTBOUND_TYPE)||(type>=0x100 && type<=LAST_INBOUND_TYPE)))throw new IllegalArgumentException("Tipo del bridge inválido");
        if(sequence<=0 || sequence>0xffffffffL || epoch<0 || epoch>0xffffffffL || payload.length>128)throw new IllegalArgumentException("Rango del bridge inválido");
        byte[] bytes=new byte[144];ByteBuffer b=ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        b.putShort((short)type).putShort((short)payload.length).putInt((int)sequence).putInt((int)epoch).put(payload);
        CRC32 crc=new CRC32();crc.update(bytes,0,140);b.putInt(140,(int)crc.getValue());
        decode(bytes,type>=0x100);return bytes;
    }
}
