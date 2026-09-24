package io.hoenn.sessions;

import android.graphics.Bitmap;

final class NativeCore {
    static { System.loadLibrary("hoenn"); }
    static synchronized native String identity();
    static synchronized native boolean open(String rom, String save);
    static synchronized native void close();
    static synchronized native int frame(int keys, Bitmap bitmap, short[] audio);
    static synchronized native byte[] bridgeHeader();
    static synchronized native byte[] bridgeSlot(int index);
    static synchronized native boolean bridgeCounter(boolean inbound, int expected, int value);
    static synchronized native boolean bridgePush(byte[] frame);
    static synchronized native void configureBridge(int address, int generationAddress);
    static synchronized native long[] saveEvidence();
    static synchronized native boolean syncSave();
    static synchronized native void bridgeHeartbeat();
}
