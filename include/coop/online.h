#ifndef GUARD_COOP_ONLINE_H
#define GUARD_COOP_ONLINE_H

#include "global.h"

void CoopOnline_Open(void);
void CoopOnline_PollInviteNotice(void);
bool8 CoopOnline_IsOpen(void);

#if TESTING
struct WindowTemplate;
const struct WindowTemplate *CoopOnline_TestWindowTemplate(void);
void CoopOnline_TestBegin(void);
void CoopOnline_TestBeginInvite(void);
bool8 CoopOnline_TestInput(u16 keys);
void CoopOnline_TestPoll(void);
bool8 CoopOnline_TestPending(void);
u8 CoopOnline_TestResult(void);
bool8 CoopOnline_TestIsLocationPage(void);
struct CoopBattleFriendlyRules;
bool8 CoopOnline_TestIsBattlePage(void);
void CoopOnline_TestGetBattleRules(struct CoopBattleFriendlyRules *rules);
#endif

#endif
