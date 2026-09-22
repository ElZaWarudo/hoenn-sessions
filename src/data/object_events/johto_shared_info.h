// Generated from the pinned donor tables; each shared Johto symbol is namespaced.

static const union AnimCmd sJohtoShared_FaceSouth[] =
{
    ANIMCMD_FRAME(0, 16),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_FaceNorth[] =
{
    ANIMCMD_FRAME(1, 16),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_FaceWest[] =
{
    ANIMCMD_FRAME(2, 16),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_FaceEast[] =
{
    ANIMCMD_FRAME(2, 16, .hFlip = TRUE),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoSouth[] =
{
    ANIMCMD_FRAME(3, 8),
    ANIMCMD_FRAME(0, 8),
    ANIMCMD_FRAME(4, 8),
    ANIMCMD_FRAME(0, 8),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoNorth[] =
{
    ANIMCMD_FRAME(5, 8),
    ANIMCMD_FRAME(1, 8),
    ANIMCMD_FRAME(6, 8),
    ANIMCMD_FRAME(1, 8),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoWest[] =
{
    ANIMCMD_FRAME(7, 8),
    ANIMCMD_FRAME(2, 8),
    ANIMCMD_FRAME(8, 8),
    ANIMCMD_FRAME(2, 8),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoEast[] =
{
    ANIMCMD_FRAME(7, 8, .hFlip = TRUE),
    ANIMCMD_FRAME(2, 8, .hFlip = TRUE),
    ANIMCMD_FRAME(8, 8, .hFlip = TRUE),
    ANIMCMD_FRAME(2, 8, .hFlip = TRUE),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFastSouth[] =
{
    ANIMCMD_FRAME(3, 4),
    ANIMCMD_FRAME(0, 4),
    ANIMCMD_FRAME(4, 4),
    ANIMCMD_FRAME(0, 4),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFastNorth[] =
{
    ANIMCMD_FRAME(5, 4),
    ANIMCMD_FRAME(1, 4),
    ANIMCMD_FRAME(6, 4),
    ANIMCMD_FRAME(1, 4),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFastWest[] =
{
    ANIMCMD_FRAME(7, 4),
    ANIMCMD_FRAME(2, 4),
    ANIMCMD_FRAME(8, 4),
    ANIMCMD_FRAME(2, 4),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFastEast[] =
{
    ANIMCMD_FRAME(7, 4, .hFlip = TRUE),
    ANIMCMD_FRAME(2, 4, .hFlip = TRUE),
    ANIMCMD_FRAME(8, 4, .hFlip = TRUE),
    ANIMCMD_FRAME(2, 4, .hFlip = TRUE),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFasterSouth[] =
{
    ANIMCMD_FRAME(3, 2),
    ANIMCMD_FRAME(0, 2),
    ANIMCMD_FRAME(4, 2),
    ANIMCMD_FRAME(0, 2),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFasterNorth[] =
{
    ANIMCMD_FRAME(5, 2),
    ANIMCMD_FRAME(1, 2),
    ANIMCMD_FRAME(6, 2),
    ANIMCMD_FRAME(1, 2),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFasterWest[] =
{
    ANIMCMD_FRAME(7, 2),
    ANIMCMD_FRAME(2, 2),
    ANIMCMD_FRAME(8, 2),
    ANIMCMD_FRAME(2, 2),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFasterEast[] =
{
    ANIMCMD_FRAME(7, 2, .hFlip = TRUE),
    ANIMCMD_FRAME(2, 2, .hFlip = TRUE),
    ANIMCMD_FRAME(8, 2, .hFlip = TRUE),
    ANIMCMD_FRAME(2, 2, .hFlip = TRUE),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFastestSouth[] =
{
    ANIMCMD_FRAME(3, 1),
    ANIMCMD_FRAME(0, 1),
    ANIMCMD_FRAME(4, 1),
    ANIMCMD_FRAME(0, 1),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFastestNorth[] =
{
    ANIMCMD_FRAME(5, 1),
    ANIMCMD_FRAME(1, 1),
    ANIMCMD_FRAME(6, 1),
    ANIMCMD_FRAME(1, 1),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFastestWest[] =
{
    ANIMCMD_FRAME(7, 1),
    ANIMCMD_FRAME(2, 1),
    ANIMCMD_FRAME(8, 1),
    ANIMCMD_FRAME(2, 1),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd sJohtoShared_GoFastestEast[] =
{
    ANIMCMD_FRAME(7, 1, .hFlip = TRUE),
    ANIMCMD_FRAME(2, 1, .hFlip = TRUE),
    ANIMCMD_FRAME(8, 1, .hFlip = TRUE),
    ANIMCMD_FRAME(2, 1, .hFlip = TRUE),
    ANIMCMD_JUMP(0),
};

static const union AnimCmd *const sJohtoSharedAnimTable_Standard[] = {
    [ANIM_STD_FACE_SOUTH] = sJohtoShared_FaceSouth,
    [ANIM_STD_FACE_NORTH] = sJohtoShared_FaceNorth,
    [ANIM_STD_FACE_WEST] = sJohtoShared_FaceWest,
    [ANIM_STD_FACE_EAST] = sJohtoShared_FaceEast,
    [ANIM_STD_GO_SOUTH] = sJohtoShared_GoSouth,
    [ANIM_STD_GO_NORTH] = sJohtoShared_GoNorth,
    [ANIM_STD_GO_WEST] = sJohtoShared_GoWest,
    [ANIM_STD_GO_EAST] = sJohtoShared_GoEast,
    [ANIM_STD_GO_FAST_SOUTH] = sJohtoShared_GoFastSouth,
    [ANIM_STD_GO_FAST_NORTH] = sJohtoShared_GoFastNorth,
    [ANIM_STD_GO_FAST_WEST] = sJohtoShared_GoFastWest,
    [ANIM_STD_GO_FAST_EAST] = sJohtoShared_GoFastEast,
    [ANIM_STD_GO_FASTER_SOUTH] = sJohtoShared_GoFasterSouth,
    [ANIM_STD_GO_FASTER_NORTH] = sJohtoShared_GoFasterNorth,
    [ANIM_STD_GO_FASTER_WEST] = sJohtoShared_GoFasterWest,
    [ANIM_STD_GO_FASTER_EAST] = sJohtoShared_GoFasterEast,
    [ANIM_STD_GO_FASTEST_SOUTH] = sJohtoShared_GoFastestSouth,
    [ANIM_STD_GO_FASTEST_NORTH] = sJohtoShared_GoFastestNorth,
    [ANIM_STD_GO_FASTEST_WEST] = sJohtoShared_GoFastestWest,
    [ANIM_STD_GO_FASTEST_EAST] = sJohtoShared_GoFastestEast,
};

static const struct SpriteFrameImage sJohtoSharedPicTable_BEAUTY[] = {
    overworld_frame(gObjectEventPic_JohtoShared_BEAUTY, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_BEAUTY, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_BEAUTY, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_BEAUTY, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_BEAUTY, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_BEAUTY, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_BEAUTY, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_BEAUTY, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_BEAUTY, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_BIG_WAILMER_DOLL[] = {
    obj_frame_tiles(gObjectEventPic_JohtoShared_BIG_WAILMER_DOLL),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_BLACK_BELT[] = {
    overworld_frame(gObjectEventPic_JohtoShared_BLACK_BELT, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_BLACK_BELT, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_BLACK_BELT, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_BLACK_BELT, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_BLACK_BELT, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_BLACK_BELT, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_BLACK_BELT, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_BLACK_BELT, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_BLACK_BELT, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_BREAKABLE_ROCK[] = {
    overworld_frame(gObjectEventPic_JohtoShared_BREAKABLE_ROCK, 2, 2, 0),
    overworld_frame(gObjectEventPic_JohtoShared_BREAKABLE_ROCK, 2, 2, 1),
    overworld_frame(gObjectEventPic_JohtoShared_BREAKABLE_ROCK, 2, 2, 2),
    overworld_frame(gObjectEventPic_JohtoShared_BREAKABLE_ROCK, 2, 2, 3),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_BUG_CATCHER[] = {
    overworld_frame(gObjectEventPic_JohtoShared_BUG_CATCHER, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_BUG_CATCHER, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_BUG_CATCHER, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_BUG_CATCHER, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_BUG_CATCHER, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_BUG_CATCHER, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_BUG_CATCHER, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_BUG_CATCHER, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_BUG_CATCHER, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_CAMPER[] = {
    overworld_frame(gObjectEventPic_JohtoShared_CAMPER, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_CAMPER, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_CAMPER, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_CAMPER, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_CAMPER, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_CAMPER, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_CAMPER, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_CAMPER, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_CAMPER, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_CAPTAIN[] = {
    overworld_frame(gObjectEventPic_JohtoShared_CAPTAIN, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_CAPTAIN, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_CAPTAIN, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_CAPTAIN, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_CAPTAIN, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_CAPTAIN, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_CAPTAIN, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_CAPTAIN, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_CAPTAIN, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_CLERK[] = {
    overworld_frame(gObjectEventPic_JohtoShared_CLERK, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_CLERK, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_CLERK, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_CLERK, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_CLERK, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_CLERK, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_CLERK, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_CLERK, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_CLERK, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_CUTTABLE_TREE[] = {
    overworld_frame(gObjectEventPic_JohtoShared_CUTTABLE_TREE, 2, 2, 0),
    overworld_frame(gObjectEventPic_JohtoShared_CUTTABLE_TREE, 2, 2, 1),
    overworld_frame(gObjectEventPic_JohtoShared_CUTTABLE_TREE, 2, 2, 2),
    overworld_frame(gObjectEventPic_JohtoShared_CUTTABLE_TREE, 2, 2, 3),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_FAT_MAN[] = {
    overworld_frame(gObjectEventPic_JohtoShared_FAT_MAN, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_FAT_MAN, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_FAT_MAN, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_FAT_MAN, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_FAT_MAN, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_FAT_MAN, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_FAT_MAN, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_FAT_MAN, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_FAT_MAN, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_FISHER[] = {
    overworld_frame(gObjectEventPic_JohtoShared_FISHER, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_FISHER, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_FISHER, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_FISHER, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_FISHER, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_FISHER, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_FISHER, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_FISHER, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_FISHER, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_FOSSIL[] = {
    obj_frame_tiles(gObjectEventPic_JohtoShared_FOSSIL),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_GBA_KID[] = {
    overworld_frame(gObjectEventPic_JohtoShared_GBA_KID, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_GBA_KID, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_GBA_KID, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_GBA_KID, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_GBA_KID, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_GBA_KID, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_GBA_KID, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_GBA_KID, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_GBA_KID, 2, 4, 2),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_GENTLEMAN[] = {
    overworld_frame(gObjectEventPic_JohtoShared_GENTLEMAN, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_GENTLEMAN, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_GENTLEMAN, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_GENTLEMAN, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_GENTLEMAN, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_GENTLEMAN, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_GENTLEMAN, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_GENTLEMAN, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_GENTLEMAN, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_GIOVANNI[] = {
    overworld_frame(gObjectEventPic_JohtoShared_GIOVANNI, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_GIOVANNI, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_GIOVANNI, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_GIOVANNI, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_GIOVANNI, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_GIOVANNI, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_GIOVANNI, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_GIOVANNI, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_GIOVANNI, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_GIRL_1[] = {
    overworld_frame(gObjectEventPic_JohtoShared_GIRL_1, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_GIRL_1, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_GIRL_1, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_GIRL_1, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_GIRL_1, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_GIRL_1, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_GIRL_1, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_GIRL_1, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_GIRL_1, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_GYM_GUY[] = {
    overworld_frame(gObjectEventPic_JohtoShared_GYM_GUY, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_GYM_GUY, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_GYM_GUY, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_GYM_GUY, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_GYM_GUY, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_GYM_GUY, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_GYM_GUY, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_GYM_GUY, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_GYM_GUY, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_HIKER[] = {
    overworld_frame(gObjectEventPic_JohtoShared_HIKER, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_HIKER, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_HIKER, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_HIKER, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_HIKER, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_HIKER, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_HIKER, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_HIKER, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_HIKER, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_LANCE[] = {
    overworld_frame(gObjectEventPic_JohtoShared_LANCE, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_LANCE, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_LANCE, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_LANCE, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_LANCE, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_LANCE, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_LANCE, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_LANCE, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_LANCE, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_LAPRAS[] = {
    overworld_frame(gObjectEventPic_JohtoShared_LAPRAS, 4, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_LAPRAS, 4, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_LAPRAS, 4, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_LAPRAS, 4, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_LAPRAS, 4, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_LAPRAS, 4, 4, 5),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_LASS[] = {
    overworld_frame(gObjectEventPic_JohtoShared_LASS, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_LASS, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_LASS, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_LASS, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_LASS, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_LASS, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_LASS, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_LASS, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_LASS, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_LITTLE_BOY[] = {
    overworld_frame(gObjectEventPic_JohtoShared_LITTLE_BOY, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_LITTLE_BOY, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_LITTLE_BOY, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_LITTLE_BOY, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_LITTLE_BOY, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_LITTLE_BOY, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_LITTLE_BOY, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_LITTLE_BOY, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_LITTLE_BOY, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_MOM[] = {
    overworld_frame(gObjectEventPic_JohtoShared_MOM, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_MOM, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_MOM, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_MOM, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_MOM, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_MOM, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_MOM, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_MOM, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_MOM, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_MR_FUJI[] = {
    overworld_frame(gObjectEventPic_JohtoShared_MR_FUJI, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_MR_FUJI, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_MR_FUJI, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_MR_FUJI, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_MR_FUJI, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_MR_FUJI, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_MR_FUJI, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_MR_FUJI, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_MR_FUJI, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_MYSTERY_GIFT_MAN[] = {
    overworld_frame(gObjectEventPic_JohtoShared_MYSTERY_GIFT_MAN, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_MYSTERY_GIFT_MAN, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_MYSTERY_GIFT_MAN, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_MYSTERY_GIFT_MAN, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_MYSTERY_GIFT_MAN, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_MYSTERY_GIFT_MAN, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_MYSTERY_GIFT_MAN, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_MYSTERY_GIFT_MAN, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_MYSTERY_GIFT_MAN, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_NURSE[] = {
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 8),
    overworld_frame(gObjectEventPic_JohtoShared_NURSE, 2, 4, 9),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_OLD_MAN_1[] = {
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_1, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_1, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_1, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_1, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_1, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_1, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_1, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_1, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_1, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_OLD_MAN_2[] = {
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_2, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_2, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_2, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_2, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_2, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_2, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_2, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_2, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_MAN_2, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_OLD_WOMAN[] = {
    overworld_frame(gObjectEventPic_JohtoShared_OLD_WOMAN, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_WOMAN, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_WOMAN, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_WOMAN, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_WOMAN, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_WOMAN, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_WOMAN, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_WOMAN, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_OLD_WOMAN, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_PICNICKER[] = {
    overworld_frame(gObjectEventPic_JohtoShared_PICNICKER, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_PICNICKER, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_PICNICKER, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_PICNICKER, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_PICNICKER, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_PICNICKER, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_PICNICKER, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_PICNICKER, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_PICNICKER, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_POKE_BALL[] = {
    overworld_frame(gObjectEventPic_JohtoShared_POKE_BALL, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_POKE_BALL, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_POKE_BALL, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_POKE_BALL, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_POKE_BALL, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_POKE_BALL, 2, 4, 0),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_POLICEMAN[] = {
    overworld_frame(gObjectEventPic_JohtoShared_POLICEMAN, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_POLICEMAN, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_POLICEMAN, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_POLICEMAN, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_POLICEMAN, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_POLICEMAN, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_POLICEMAN, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_POLICEMAN, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_POLICEMAN, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_PROF_OAK[] = {
    overworld_frame(gObjectEventPic_JohtoShared_PROF_OAK, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_PROF_OAK, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_PROF_OAK, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_PROF_OAK, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_PROF_OAK, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_PROF_OAK, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_PROF_OAK, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_PROF_OAK, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_PROF_OAK, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_PSYCHIC_M[] = {
    overworld_frame(gObjectEventPic_JohtoShared_PSYCHIC_M, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_PSYCHIC_M, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_PSYCHIC_M, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_PSYCHIC_M, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_PSYCHIC_M, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_PSYCHIC_M, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_PSYCHIC_M, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_PSYCHIC_M, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_PSYCHIC_M, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_PUSHABLE_BOULDER[] = {
    obj_frame_tiles(gObjectEventPic_JohtoShared_PUSHABLE_BOULDER),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_RAYQUAZA[] = {
    overworld_frame(gObjectEventPic_JohtoShared_RAYQUAZA, 8, 8, 0),
    overworld_frame(gObjectEventPic_JohtoShared_RAYQUAZA, 8, 8, 1),
    overworld_frame(gObjectEventPic_JohtoShared_RAYQUAZA, 8, 8, 2),
    overworld_frame(gObjectEventPic_JohtoShared_RAYQUAZA, 8, 8, 3),
    overworld_frame(gObjectEventPic_JohtoShared_RAYQUAZA, 8, 8, 4),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_RED_NORMAL[] = {
    overworld_frame(gObjectEventPic_JohtoShared_RED_NORMAL, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_RED_NORMAL, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_RED_NORMAL, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_RED_NORMAL, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_RED_NORMAL, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_RED_NORMAL, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_RED_NORMAL, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_RED_NORMAL, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_RED_NORMAL, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_ROCKET_F[] = {
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_F, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_F, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_F, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_F, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_F, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_F, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_F, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_F, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_F, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_ROCKET_M[] = {
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_M, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_M, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_M, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_M, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_M, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_M, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_M, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_M, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_ROCKET_M, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_SABRINA[] = {
    overworld_frame(gObjectEventPic_JohtoShared_SABRINA, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_SABRINA, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_SABRINA, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_SABRINA, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_SABRINA, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_SABRINA, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_SABRINA, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_SABRINA, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_SABRINA, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_SAILOR[] = {
    overworld_frame(gObjectEventPic_JohtoShared_SAILOR, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_SAILOR, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_SAILOR, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_SAILOR, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_SAILOR, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_SAILOR, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_SAILOR, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_SAILOR, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_SAILOR, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_SWIMMER_F_WATER[] = {
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_F_WATER, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_F_WATER, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_F_WATER, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_F_WATER, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_F_WATER, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_F_WATER, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_F_WATER, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_F_WATER, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_F_WATER, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_SWIMMER_M_WATER[] = {
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_M_WATER, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_M_WATER, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_M_WATER, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_M_WATER, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_M_WATER, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_M_WATER, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_M_WATER, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_M_WATER, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_SWIMMER_M_WATER, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_TWIN[] = {
    overworld_frame(gObjectEventPic_JohtoShared_TWIN, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_TWIN, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_TWIN, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_TWIN, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_TWIN, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_TWIN, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_TWIN, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_TWIN, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_TWIN, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_WOMAN_1[] = {
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_1, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_1, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_1, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_1, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_1, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_1, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_1, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_1, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_1, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_WOMAN_2[] = {
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_2, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_2, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_2, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_2, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_2, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_2, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_2, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_2, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_2, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_WOMAN_3[] = {
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_3, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_3, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_3, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_3, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_3, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_3, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_3, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_3, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_WOMAN_3, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_WORKER_M[] = {
    overworld_frame(gObjectEventPic_JohtoShared_WORKER_M, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_WORKER_M, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_WORKER_M, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_WORKER_M, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_WORKER_M, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_WORKER_M, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_WORKER_M, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_WORKER_M, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_WORKER_M, 2, 4, 8),
};

static const struct SpriteFrameImage sJohtoSharedPicTable_YOUNGSTER[] = {
    overworld_frame(gObjectEventPic_JohtoShared_YOUNGSTER, 2, 4, 0),
    overworld_frame(gObjectEventPic_JohtoShared_YOUNGSTER, 2, 4, 1),
    overworld_frame(gObjectEventPic_JohtoShared_YOUNGSTER, 2, 4, 2),
    overworld_frame(gObjectEventPic_JohtoShared_YOUNGSTER, 2, 4, 3),
    overworld_frame(gObjectEventPic_JohtoShared_YOUNGSTER, 2, 4, 4),
    overworld_frame(gObjectEventPic_JohtoShared_YOUNGSTER, 2, 4, 5),
    overworld_frame(gObjectEventPic_JohtoShared_YOUNGSTER, 2, 4, 6),
    overworld_frame(gObjectEventPic_JohtoShared_YOUNGSTER, 2, 4, 7),
    overworld_frame(gObjectEventPic_JohtoShared_YOUNGSTER, 2, 4, 8),
};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_BEAUTY = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_BEAUTY, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_BIG_WAILMER_DOLL = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK, OBJ_EVENT_PAL_TAG_NONE, 512, 32, 32, 5, SHADOW_SIZE_M, TRUE, FALSE, TRACKS_NONE, &gObjectEventBaseOam_32x32, sOamTables_32x32, sAnimTable_Inanimate, sJohtoSharedPicTable_BIG_WAILMER_DOLL, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_BLACK_BELT = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_BLACK_BELT, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_BREAKABLE_ROCK = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 128, 16, 16, 2, SHADOW_SIZE_S, TRUE, FALSE, TRACKS_NONE, &gObjectEventBaseOam_16x16, sOamTables_16x16, sAnimTable_BreakableRock, sJohtoSharedPicTable_BREAKABLE_ROCK, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_BUG_CATCHER = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_BUG_CATCHER, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_CAMPER = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_CAMPER, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_CAPTAIN = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_CAPTAIN, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_CLERK = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_CLERK, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_CUTTABLE_TREE = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN, OBJ_EVENT_PAL_TAG_NONE, 128, 16, 16, 4, SHADOW_SIZE_NONE, TRUE, FALSE, TRACKS_NONE, &gObjectEventBaseOam_16x16, sOamTables_16x16, sAnimTable_CuttableTree, sJohtoSharedPicTable_CUTTABLE_TREE, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_FAT_MAN = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_FAT_MAN, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_FISHER = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 3, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_FISHER, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_FOSSIL = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_ROCKET_1, OBJ_EVENT_PAL_TAG_NONE, 128, 16, 16, 2, SHADOW_SIZE_S, TRUE, FALSE, TRACKS_NONE, &gObjectEventBaseOam_16x16, sOamTables_16x16, sAnimTable_Inanimate, sJohtoSharedPicTable_FOSSIL, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_GBA_KID = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_GBA_KID, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_GENTLEMAN = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_GENTLEMAN, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_GIOVANNI = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_ROCKET_1, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_GIOVANNI, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_GIRL_1 = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 3, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_GIRL_1, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_GYM_GUY = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_GYM_GUY, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_HIKER = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_HIKER, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_LANCE = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_LANCE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_LANCE, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_LAPRAS = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_LAPRAS, OBJ_EVENT_PAL_TAG_NONE, 512, 32, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_32x32, sOamTables_32x32, sAnimTable_Following, sJohtoSharedPicTable_LAPRAS, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_LASS = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_LASS, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_LITTLE_BOY = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_LITTLE_BOY, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_MOM = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_MOM, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_MR_FUJI = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_MR_FUJI, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_MYSTERY_GIFT_MAN = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_MYSTERY_GIFT_MAN, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_NURSE = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sAnimTable_Nurse, sJohtoSharedPicTable_NURSE, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_OLD_MAN_1 = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_OLD_MAN_1, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_OLD_MAN_2 = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_OLD_MAN_2, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_OLD_WOMAN = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_OLD_WOMAN, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_PICNICKER = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_PICNICKER, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_POKE_BALL = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_ROCKET_1, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, TRUE, FALSE, TRACKS_NONE, &gObjectEventBaseOam_16x32, sOamTables_16x32, sAnimTable_Following, sJohtoSharedPicTable_POKE_BALL, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_POLICEMAN = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_POLICEMAN, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_PROF_OAK = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_ROCKET_1, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_PROF_OAK, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_PSYCHIC_M = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_PSYCHIC_M, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_PUSHABLE_BOULDER = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 128, 16, 16, 2, SHADOW_SIZE_S, TRUE, FALSE, TRACKS_NONE, &gObjectEventBaseOam_16x16, sOamTables_16x16, sAnimTable_Inanimate, sJohtoSharedPicTable_PUSHABLE_BOULDER, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_RAYQUAZA = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_TOWER_BEAM, OBJ_EVENT_PAL_TAG_NONE, 2048, 64, 64, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_64x64, sOamTables_64x64, sAnimTable_Rayquaza, sJohtoSharedPicTable_RAYQUAZA, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_RED_NORMAL = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_RED, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_RED_NORMAL, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_ROCKET_F = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_ROCKET_1, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_ROCKET_F, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_ROCKET_M = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_ROCKET_1, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_ROCKET_M, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_SABRINA = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_SABRINA, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_SAILOR = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_SAILOR, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_SWIMMER_F_WATER = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 3, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_SWIMMER_F_WATER, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_SWIMMER_M_WATER = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_SWIMMER_M_WATER, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_TWIN = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 3, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_TWIN, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_WOMAN_1 = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_WOMAN_1, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_WOMAN_2 = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 4, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_WOMAN_2, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_WOMAN_3 = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_WOMAN_3, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_WORKER_M = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 5, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_WORKER_M, gDummySpriteAffineAnimTable};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_JohtoShared_YOUNGSTER = {TAG_NONE, OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE, OBJ_EVENT_PAL_TAG_NONE, 256, 16, 32, 2, SHADOW_SIZE_M, FALSE, FALSE, TRACKS_FOOT, &gObjectEventBaseOam_16x32, sOamTables_16x32, sJohtoSharedAnimTable_Standard, sJohtoSharedPicTable_YOUNGSTER, gDummySpriteAffineAnimTable};
