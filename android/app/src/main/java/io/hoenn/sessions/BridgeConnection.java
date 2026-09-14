package io.hoenn.sessions;

import java.io.*;
import java.net.*;
import java.nio.*;
import java.util.concurrent.*;
import org.json.JSONObject;

/** Bounded in-process sidecar connection. Network I/O never runs on the GBA CPU thread. */
final class BridgeConnection implements AutoCloseable {
    private final long epoch;
    private final ArrayBlockingQueue<byte[]> inbound=new ArrayBlockingQueue<>(32);
    private final ArrayBlockingQueue<Send> outbound=new ArrayBlockingQueue<>(2);
    private volatile Socket socket;
    private final Thread reader;
    private volatile Thread writer;
    private final Object networkLock=new Object();
    private volatile boolean authenticated,closed;
    private volatile String failure;
    private Send pending;
    private int initializationFrames,frames;
    private boolean initialized;
    private long grantSerial=-1,grantGeneration,activeEpoch;
    private static final class Send {final byte[] bytes;volatile boolean sent;Send(byte[] b){bytes=b;}}
    BridgeConnection(JSONObject descriptor,long epoch) throws Exception {
        if(epoch<=0 || epoch>0xffffffffL || !descriptor.getString("host").equals("127.0.0.1") || !descriptor.getString("transport").equals("tcp"))throw new SecurityException("Descriptor inválido");
        this.epoch=epoch;int port=descriptor.getInt("port");String secret=descriptor.getString("secret");
        if(port<1 || port>65535 || !secret.matches("[0-9a-f]{32}"))throw new SecurityException("Descriptor inválido");
        reader=new Thread(()->connect(port,secret),"bridge-connect");reader.start();
    }
    private void connect(int port,String secret){
        try {
            Socket s=new Socket();synchronized(networkLock){if(closed){s.close();return;}socket=s;}
            s.connect(new InetSocketAddress("127.0.0.1",port),3000);s.setSoTimeout(3000);s.setTcpNoDelay(true);
            OutputStream output=s.getOutputStream();InputStream input=s.getInputStream();
            output.write(("{\"secret\":\""+secret+"\",\"bridge_abi\":1,\"protocol_version\":1}\n").getBytes(java.nio.charset.StandardCharsets.US_ASCII));
            ByteArrayOutputStream line=new ByteArrayOutputStream();int c;
            while((c=input.read())!=-1){line.write(c);if(line.size()>256)throw new IOException();if(c==10)break;}
            if(!line.toString("US-ASCII").equals("{\"ok\":true}\n"))throw new SecurityException();
            s.setSoTimeout(0);authenticated=true;
            synchronized(networkLock){if(closed)return;writer=new Thread(()->write(output),"bridge-write");writer.start();}
            DataInputStream data=new DataInputStream(input);
            while(!closed){byte[] frame=new byte[144];data.readFully(frame);BridgeFrame.decode(frame,true);if(!inbound.offer(frame))throw new IOException("Cola inbound llena");}
        }catch(Exception e){if(!closed)failure="Conexión local del bridge perdida";}
        finally{Socket s=socket;if(s!=null)try{s.close();}catch(IOException ignored){}}
    }
    private void write(OutputStream out){try{while(!closed){Send send=outbound.poll(200,TimeUnit.MILLISECONDS);if(send!=null){out.write(send.bytes);send.sent=true;}}}catch(Exception e){if(!closed)failure="Escritura del bridge fallida";}}
    void step() throws Exception {
        if(closed)return;if(failure!=null)throw new IOException(failure);
        if(!initialized){try{NativeBridge.validate(NativeCore.readBridge(NativeBridge.ADDRESS));initialized=true;}catch(SecurityException e){if(++initializationFrames>600)throw e;return;}}
        if(!authenticated)return;
        for(int i=0;i<32;i++){
            byte[] next=inbound.peek();if(next==null)break;BridgeFrame message=BridgeFrame.decode(next,true);
            if(message.epoch!=epoch)throw new SecurityException("Epoch entrante inválido");
            if(!NativeBridge.pushInbound(next,epoch))break;
            if(message.type==0x100){activeEpoch=epoch;grantSerial=-1;}
            if(message.type==0x10c){
                if(activeEpoch!=epoch || grantSerial>=0 || message.payload.length!=0)throw new SecurityException("Grant de guardado inválido");
                long[] proof=NativeCore.saveEvidence();grantSerial=proof[0];grantGeneration=proof[2];
            }
            inbound.remove();
        }
        if(pending!=null && pending.sent){
            BridgeFrame message=BridgeFrame.decode(pending.bytes,false);
            if(message.type==13 && NativeCore.saveEvidence()[2]!=generation(message))throw new SecurityException("Guardado cambió durante envío");
            if(!NativeBridge.commitOutbound(pending.bytes))throw new SecurityException("Consumer del bridge cambió");
            if(message.type==13)grantSerial=-1;
            pending=null;
        }
        if(pending==null){
            byte[] next=NativeBridge.peekOutbound();
            if(next!=null){BridgeFrame message=BridgeFrame.decode(next,false);
                if(message.type==13){
                    if(grantSerial<0 || message.epoch!=epoch)throw new SecurityException("Guardado sin grant");
                    long target=generation(message);long[] proof=NativeCore.saveEvidence();
                    if(target!=((grantGeneration+1)&0xffffffffL))throw new SecurityException("Generación no correlacionada");
                    if(proof[0]<=grantSerial || proof[1]!=target || proof[2]!=target)return;
                    if(!NativeCore.syncSave())throw new IOException("No se pudo sincronizar SAV canónico");
                }
                pending=new Send(next);if(!outbound.offer(pending))throw new IOException("Cola outbound llena");
            }
        }
        if(++frames%60==0)NativeCore.bridgeHeartbeat();
    }
    private long generation(BridgeFrame f){if(f.payload.length!=4)throw new SecurityException("Payload SAV inválido");return Integer.toUnsignedLong(ByteBuffer.wrap(f.payload).order(ByteOrder.LITTLE_ENDIAN).getInt());}
    @Override public void close(){
        synchronized(networkLock){closed=true;Socket s=socket;if(s!=null)try{s.close();}catch(IOException ignored){}}
        reader.interrupt();Thread w=writer;if(w!=null)w.interrupt();
        try{reader.join(1500);if(w!=null)w.join(1500);}catch(InterruptedException e){Thread.currentThread().interrupt();throw new IllegalStateException("Cierre del bridge interrumpido");}
        inbound.clear();outbound.clear();
        if(reader.isAlive() || (w!=null && w.isAlive()))throw new IllegalStateException("El bridge no confirmó su cierre");
    }
}
