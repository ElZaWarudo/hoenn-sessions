.macro johto_buffermoncategory stringvar:req, species:req
    callnative Script_JohtoBufferMonCategory, requests_effects=1
    .byte \stringvar
    .2byte \species
.endm
