package io.hoenn.sessions;

/** Keeps local cancellation sticky when a host stop arrives later. */
final class ArrivalStopState {
    private volatile boolean requested;
    private boolean canceled,hostStopped;

    synchronized void request(boolean fromHost) {
        if(fromHost)hostStopped=true;
        else canceled=true;
        requested=true;
    }

    boolean isRequested() {return requested;}

    synchronized boolean hostClosed() {return hostStopped && !canceled;}
}
