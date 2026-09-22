#ifndef GUARD_COOP_GROUP_TRAVEL_H
#define GUARD_COOP_GROUP_TRAVEL_H

#include "coop/group_travel_protocol.h"

enum CoopGroupTravelBeginResult { COOP_GROUP_TRAVEL_BEGIN_NOT_GROUPED = 0, COOP_GROUP_TRAVEL_BEGIN_WAITING = 1, COOP_GROUP_TRAVEL_BEGIN_REJECTED = 2 };
enum CoopGroupTravelOfferState { COOP_GROUP_TRAVEL_OFFER_NONE = 0, COOP_GROUP_TRAVEL_OFFER_DEFERRED, COOP_GROUP_TRAVEL_OFFER_READY };
enum CoopGroupTravelDepartureContext
{
    COOP_GROUP_TRAVEL_DEPARTURE_NONE,
    COOP_GROUP_TRAVEL_DEPARTURE_TRAIN,
    COOP_GROUP_TRAVEL_DEPARTURE_FERRY,
    COOP_GROUP_TRAVEL_DEPARTURE_SSAQUA_MAIDEN,
    COOP_GROUP_TRAVEL_DEPARTURE_GATE,
};

void CoopGroupTravel_Init(void);
void CoopGroupTravel_Poll(void);
enum CoopGroupTravelBeginResult CoopGroupTravel_Begin(u8 route);
enum CoopGroupTravelBeginResult CoopGroupTravel_BeginFromScript(u8 route, u8 departure);
bool8 CoopGroupTravel_Cancel(void);
enum CoopGroupTravelOfferState CoopGroupTravel_GetOffer(struct CoopGroupTravelRecord *offer);
bool8 CoopGroupTravel_RespondToOffer(bool8 accept);
bool8 CoopGroupTravel_ReceiveServer(const struct CoopGroupTravelRecord *record);
void CoopGroupTravel_OnSessionReady(void);
void CoopGroupTravel_OnTransportLost(void);
bool8 CoopGroupTravel_IsManagingArrival(void);

void Special_CoopGroupTravelBegin(void);
void Special_CoopGroupTravelGetOffer(void);
void Special_CoopGroupTravelRespond(void);
void Special_CoopGroupTravelCancel(void);

#if TESTING
void CoopGroupTravel_TestSetSafe(bool8 safe);
void CoopGroupTravel_TestSetManagingArrival(bool8 managing);
u8 CoopGroupTravel_TestState(void);
void CoopGroupTravel_TestSeedRequest(const struct CoopGroupTravelRecord *record);
void CoopGroupTravel_TestSeedAppliedPending(const struct CoopGroupTravelRecord *record);
void CoopGroupTravel_TestSeedCommitting(const struct CoopGroupTravelRecord *record);
bool8 CoopGroupTravel_TestSemanticQueued(void);
bool8 CoopGroupTravel_TestAtExactDestination(u8 route);
void CoopGroupTravel_TestSetGrouped(bool8 grouped);
void CoopGroupTravel_TestSetDeparture(u8 departure);
const struct CoopGroupTravelRecord *CoopGroupTravel_TestRecord(void);
#endif

#endif
