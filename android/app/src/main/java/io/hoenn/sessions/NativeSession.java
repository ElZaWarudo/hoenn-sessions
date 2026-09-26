package io.hoenn.sessions;
final class NativeSession {
    static { System.loadLibrary("coop_android"); }
    static native boolean start(String privateDirectory,String username,String password,String userId,String characterId,String catalogSha256,boolean resume,boolean logoutOnly);
    static native String poll();
    static native void stop();
    static native void acknowledgeStopped();
    static native boolean isActive();
    static native boolean reconnect();
    static native void signOut();
    static native void arrivalVerifierClosed(long verificationId, boolean success, String reason);
}
