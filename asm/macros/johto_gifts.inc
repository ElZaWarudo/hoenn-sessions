@ Johto gift and static encounter adapters.

.macro johto_givenamedmon giftId:req
    callnative Script_JohtoGiveNamedMon, requests_effects=1
    .2byte \giftId
.endm

.macro johto_giveoddegg which:req
    callnative Script_JohtoGiveOddEgg, requests_effects=1
    .2byte \which
.endm

.macro johto_setwildbattleshiny species:req, level:req, item=ITEM_NONE
    setwildbattle \species, \level, \item
    callnative Script_JohtoMarkWildShiny, requests_effects=1
.endm
