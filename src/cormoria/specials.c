#include "global.h"
#include "credits.h"
#include "cormoria/voltorb_flip.h"
#include "event_data.h"
#include "main.h"
#include "overworld.h"
#include "pokeball.h"
#include "pokemon.h"
#include "region_map.h"
#include "script.h"
#include "tv.h"
#include "constants/party_menu.h"

#if ROM_WORLD == 2
void Special_ViewVoltorbFlip(void)
{
    gMain.savedCallback = CB2_ReturnToField;
    SetMainCallback2(CB2_ShowVoltorbFlip);
    LockPlayerFieldControls();
}

/* Dreamstone's gift script changes the ball of an existing party Pokémon. */
void SetMonPokeball(void)
{
    enum PokeBall ball;

    if (gSpecialVar_0x8004 >= gPlayerPartyCount)
        return;
    ball = ItemIdToBallId(gSpecialVar_0x8005);
    SetMonData(&gPlayerParty[gSpecialVar_0x8004], MON_DATA_POKEBALL, &ball);
}

void StartCredits(void)
{
    SetMainCallback2(CB2_StartCreditsSequence);
}

void StartFly(void)
{
    SetMainCallback2(CB2_OpenFlyMap);
}

void ChangeBoxPokemonNickname(void)
{
    gSpecialVar_0x8004 = PC_MON_CHOSEN;
    ChangePokemonNickname();
}
#endif
