package io.hoenn.sessions;

import org.junit.Test;
import static org.junit.Assert.*;

public class ArrivalStopStateTest {
    @Test public void onlyHostStopWithoutCancellationCanAcknowledge() {
        ArrivalStopState clean=new ArrivalStopState();
        clean.request(true);
        assertTrue(clean.isRequested());
        assertTrue(clean.hostClosed());

        ArrivalStopState cancelFirst=new ArrivalStopState();
        cancelFirst.request(false);
        cancelFirst.request(true);
        assertFalse(cancelFirst.hostClosed());

        ArrivalStopState hostFirst=new ArrivalStopState();
        hostFirst.request(true);
        hostFirst.request(false);
        assertFalse(hostFirst.hostClosed());
    }
}
