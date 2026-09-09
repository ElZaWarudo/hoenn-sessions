@ Selected Johto plots use automatic regrowth, not the host gardening menu.
Johto_BerryTreeScript::
    lock
    callnative Script_JohtoHarvestBerryTree, requests_effects=1
    goto_if_eq VAR_RESULT, JOHTO_BERRY_HARVEST_SUCCESS, Johto_BerryTree_Picked
    goto_if_eq VAR_RESULT, JOHTO_BERRY_HARVEST_BAG_FULL, Johto_BerryTree_Full
    message Johto_BerryTree_Text_Growing
    waitmessage
    waitbuttonpress
    release
    end

Johto_BerryTree_Full:
    message BerryTree_Text_BerryPocketFull
    waitmessage
    waitbuttonpress
    release
    end

Johto_BerryTree_Picked:
    message BerryTree_Text_PickedTheBerry
    playfanfare MUS_OBTAIN_BERRY
    waitmessage
    waitfanfare
    release
    end

Johto_BerryTree_Text_Growing:
    .string "It's a BERRY tree!\n"
    .string "These BERRIES aren't ripe yet.$"
