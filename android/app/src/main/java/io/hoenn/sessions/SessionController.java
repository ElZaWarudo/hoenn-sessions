package io.hoenn.sessions;

/**
 * Owns the small state machine that coordinates the Android lifecycle with a
 * native session.  Native callbacks and the main thread can both advance this
 * state, so all transitions are synchronized here instead of being spread
 * across several independent volatile flags in the activity.
 */
enum SessionState {
    READY,
    STARTING,
    UPDATE_PROMPT,
    PLAYING,
    RECONNECTING,
    CLOSING,
    RETRY_WAIT,
    PAUSED,
    DESTROYED
}

final class SessionController {
    private SessionState state = SessionState.READY;
    private boolean inFlightStart;
    private boolean resumed;
    private boolean resumeAfterPause;
    private boolean autoResumePending;
    private int retryCount;

    synchronized SessionState state() {
        return state;
    }

    synchronized boolean beginStart() {
        if (inFlightStart || state == SessionState.STARTING || state == SessionState.UPDATE_PROMPT
                || state == SessionState.PLAYING || state == SessionState.RECONNECTING
                || state == SessionState.CLOSING || state == SessionState.DESTROYED) {
            return false;
        }
        state = SessionState.STARTING;
        inFlightStart = true;
        return true;
    }

    synchronized void finishStart() {
        inFlightStart = false;
        if (state == SessionState.STARTING) state = SessionState.READY;
    }

    synchronized boolean isStarting() {
        return inFlightStart;
    }

    synchronized boolean isUpdatePromptPending() {
        return state == SessionState.UPDATE_PROMPT;
    }

    synchronized void updatePromptShown() {
        if (state != SessionState.DESTROYED) state = SessionState.UPDATE_PROMPT;
    }

    synchronized void updatePromptFinished() {
        if (state == SessionState.UPDATE_PROMPT) state = SessionState.READY;
    }

    synchronized void markPlaying() {
        if (state != SessionState.DESTROYED) state = SessionState.PLAYING;
        resumeAfterPause = false;
        retryCount = 0;
    }

    synchronized void markReconnecting() {
        if (state == SessionState.PLAYING) state = SessionState.RECONNECTING;
    }

    synchronized void markClosing() {
        if (state != SessionState.DESTROYED) state = SessionState.CLOSING;
    }

    synchronized void markReady() {
        if (state != SessionState.DESTROYED) state = SessionState.READY;
    }

    synchronized void markRetryWaiting() {
        if (state != SessionState.DESTROYED) state = SessionState.RETRY_WAIT;
    }

    synchronized void resetRetries() {
        retryCount = 0;
        if (state == SessionState.RETRY_WAIT) state = SessionState.READY;
    }

    synchronized int incrementRetry() {
        retryCount++;
        return retryCount;
    }

    synchronized int retryCount() {
        return retryCount;
    }

    synchronized void setAutoResumePending(boolean pending) {
        autoResumePending = pending;
    }

    synchronized boolean takeAutoResumeIfReady(boolean busy) {
        if (!autoResumePending || busy || state == SessionState.DESTROYED) return false;
        autoResumePending = false;
        return true;
    }

    synchronized void setResumed(boolean value) {
        resumed = value;
    }

    synchronized boolean isResumed() {
        return resumed;
    }

    synchronized boolean mayAcceptLoad(boolean gamePrepared) {
        return state != SessionState.DESTROYED && resumed && !resumeAfterPause && gamePrepared;
    }

    synchronized void requestResumeAfterPause() {
        resumeAfterPause = true;
        state = SessionState.PAUSED;
    }

    synchronized boolean wantsResumeAfterPause() {
        return resumeAfterPause;
    }

    synchronized void clearResumeAfterPause() {
        resumeAfterPause = false;
    }

    synchronized void markDestroyed() {
        resumed = false;
        resumeAfterPause = false;
        autoResumePending = false;
        state = SessionState.DESTROYED;
    }

    synchronized boolean isDestroyed() {
        return state == SessionState.DESTROYED;
    }
}
