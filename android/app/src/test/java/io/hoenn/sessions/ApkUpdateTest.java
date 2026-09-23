package io.hoenn.sessions;

import org.json.JSONObject;
import org.junit.Test;
import static org.junit.Assert.*;

public class ApkUpdateTest {
    private JSONObject metadata(int version,String id) throws Exception {
        return new JSONObject().put("release_id",id).put("version_code",version)
            .put("size",12345).put("sha256","a".repeat(64));
    }
    @Test public void offersOnlyNewerPackage() throws Exception {
        assertNull(ApkUpdate.parse(metadata(4,"release-1"),4));
        assertEquals(5,ApkUpdate.parse(metadata(5,"release-2"),4).versionCode);
    }
    @Test public void rejectsUnsafeMetadata() throws Exception {
        try {ApkUpdate.parse(metadata(5,"../release"),4);fail();}
        catch(SecurityException expected) { }
        try {ApkUpdate.parse(metadata(5,"release-1").put("sha256","bad"),4);fail();}
        catch(SecurityException expected) { }
    }
}
