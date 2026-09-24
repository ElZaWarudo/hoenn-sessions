#include "global.h"
#include "test/test.h"
#include "field_name_box.h"

TEST("DestroyNamebox clears a speaker even when no window was shown")
{
    static const u8 sSpeaker[] = _("Cormoria Guide");

    ResetNameboxData();
    gSpeakerName = sSpeaker;
    DestroyNamebox();

    EXPECT(gSpeakerName == NULL);
}
