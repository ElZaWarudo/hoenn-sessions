package io.hoenn.sessions;

import java.io.IOException;
import org.junit.Test;
import static org.junit.Assert.*;

public class CloudApiDownloadTest {
    @Test public void acceptsCompleteResponseAndExactResume() throws Exception {
        assertEquals(0,CloudApi.resumedLength(200,null,100,0,100));
        assertEquals(40,CloudApi.resumedLength(206,"bytes 40-99/100",60,40,100));
        assertEquals(0,CloudApi.resumedLength(200,null,100,40,100));
    }
    @Test public void rejectsMismatchedRangesAndLengths() throws Exception {
        assertInvalid(206,"bytes 40-98/100",59,40,100);
        assertInvalid(206,"bytes 40-99/101",60,40,100);
        assertInvalid(206,"bytes 40-99/100",59,40,100);
        assertInvalid(206,"bytes 41-99/100",59,40,100);
        assertInvalid(206,"bytes 0-99/100",100,0,100);
        assertInvalid(200,null,60,40,100);
        assertInvalid(200,null,-1,0,100);
    }
    private void assertInvalid(int code,String range,long length,long offset,long size) throws Exception {
        try {CloudApi.resumedLength(code,range,length,offset,size);fail("Accepted invalid range");}
        catch(IOException expected) { }
    }
}
