@ Johto party quest script adapters.

.macro johto_removenamedmon giftId:req
    callnative Script_JohtoRemoveNamedMon, requests_effects=1
    .2byte \giftId
.endm

.macro johto_removegenericmon species:req
    callnative Script_JohtoRemoveGenericMon, requests_effects=1
    .2byte \species
.endm

.macro johto_baobacheckmon checkId:req
    callnative Script_JohtoBaobaCheckMon, requests_effects=1
    .byte \checkId
.endm
