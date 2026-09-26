package io.hoenn.sessions;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class SessionControllerTest {
    @Test public void pauseDuringStartRequestsResumeAfterNativeStop() {
        SessionController controller = new SessionController();
        controller.setResumed(true);
        assertTrue(controller.beginStart());

        controller.requestResumeAfterPause();

        assertEquals(SessionState.PAUSED, controller.state());
        assertTrue(controller.wantsResumeAfterPause());
        assertTrue(controller.isStarting());
        assertFalse(controller.beginStart());
        assertFalse(controller.mayAcceptLoad(true));
        assertTrue(controller.isResumed());
        controller.setResumed(false);
        controller.finishStart();
        controller.clearResumeAfterPause();
        assertFalse(controller.wantsResumeAfterPause());
        controller.setResumed(true);
        assertTrue(controller.mayAcceptLoad(true));
        assertFalse(controller.mayAcceptLoad(false));
        assertTrue(controller.beginStart());
    }

    @Test public void updatePromptIsNotClearedByStartWorkerFinishing() {
        SessionController controller = new SessionController();
        assertTrue(controller.beginStart());
        controller.updatePromptShown();
        controller.finishStart();

        assertEquals(SessionState.UPDATE_PROMPT, controller.state());
        assertTrue(controller.isUpdatePromptPending());
        controller.updatePromptFinished();
        assertEquals(SessionState.READY, controller.state());
    }

    @Test public void retryStateTracksBackoffAttemptsAndResetsOnLoad() {
        SessionController controller = new SessionController();
        assertEquals(1, controller.incrementRetry());
        controller.markRetryWaiting();
        assertEquals(SessionState.RETRY_WAIT, controller.state());
        assertEquals(1, controller.retryCount());
        assertEquals(2, controller.incrementRetry());

        controller.resetRetries();

        assertEquals(SessionState.READY, controller.state());
        assertEquals(0, controller.retryCount());
    }

    @Test public void destroyRejectsNewWorkAndClearsLifecycleRequests() {
        SessionController controller = new SessionController();
        controller.setAutoResumePending(true);
        controller.requestResumeAfterPause();
        controller.setResumed(true);

        controller.markDestroyed();

        assertEquals(SessionState.DESTROYED, controller.state());
        assertTrue(controller.isDestroyed());
        assertFalse(controller.isResumed());
        assertFalse(controller.wantsResumeAfterPause());
        assertFalse(controller.takeAutoResumeIfReady(false));
        assertFalse(controller.beginStart());
    }
}
