package io.hoenn.sessions;

import java.nio.*;
import java.util.Arrays;
import java.util.zip.CRC32;

/** The same fixed 144-byte ABI as bridge/protocol.lua. No account secrets cross this ABI. */
final class BridgeFrame {
    final int type;
    final long sequence, epoch;
    final byte[] payload;
    private BridgeFrame(int type,long sequence,long epoch,byte[] payload) {this.type=type;this.sequence=sequence;this.epoch=epoch;this.payload=payload;}
    static BridgeFrame decode(byte[] bytes,boolean inbound) {
        if(bytes==null || bytes.length!=144)throw new IllegalArgumentException("Tamaño del bridge inválido");
        ByteBuffer b=ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        int type=b.getShort()&65535,length=b.getShort()&65535;
        long sequence=Integer.toUnsignedLong(b.getInt()),epoch=Integer.toUnsignedLong(b.getInt());
        if(length>128 || sequence==0 || (inbound?(type<0x100 || type>0x10d):(type<1 || type>14)))throw new IllegalArgumentException("Cabecera del bridge inválida");
        CRC32 crc=new CRC32();crc.update(bytes,0,140);
        if(crc.getValue()!=Integer.toUnsignedLong(b.getInt(140)))throw new IllegalArgumentException("CRC del bridge inválido");
        return new BridgeFrame(type,sequence,epoch,Arrays.copyOfRange(bytes,12,12+length));
    }
    static byte[] encode(int type,long sequence,long epoch,byte[] payload) {
        if(!((type>=1 && type<=14)||(type>=0x100 && type<=0x10d)))throw new IllegalArgumentException("Tipo del bridge inválido");
        if(sequence<=0 || sequence>0xffffffffL || epoch<0 || epoch>0xffffffffL || payload.length>128)throw new IllegalArgumentException("Rango del bridge inválido");
        byte[] bytes=new byte[144];ByteBuffer b=ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        b.putShort((short)type).putShort((short)payload.length).putInt((int)sequence).putInt((int)epoch).put(payload);
        CRC32 crc=new CRC32();crc.update(bytes,0,140);b.putInt(140,(int)crc.getValue());
        decode(bytes,type>=0x100);return bytes;
    }
}
