#ifndef GUARD_COOP_ONLINE_H
#define GUARD_COOP_ONLINE_H

#include "global.h"

void CoopOnline_Open(void);

#if TESTING
struct WindowTemplate;
const struct WindowTemplate *CoopOnline_TestWindowTemplate(void);
void CoopOnline_TestBegin(void);
bool8 CoopOnline_TestInput(u16 keys);
void CoopOnline_TestPoll(void);
bool8 CoopOnline_TestPending(void);
u8 CoopOnline_TestResult(void);
#endif

#endif
