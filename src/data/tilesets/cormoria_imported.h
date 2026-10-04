/* Authenticated Cormoria tileset preview; requires listed asset conversions. */
#include "global.h"
#include "global.fieldmap.h"
extern void InitTilesetAnim_BattleDome(void);
extern void InitTilesetAnim_BattleFrontierOutsideWest(void);
extern void InitTilesetAnim_BattlePyramid(void);
extern void InitTilesetAnim_BikeShop(void);
extern void InitTilesetAnim_Building(void);
extern void InitTilesetAnim_Fallarbor(void);
extern void InitTilesetAnim_Fortree(void);
extern void InitTilesetAnim_General(void);
extern void InitTilesetAnim_Lavaridge(void);
extern void InitTilesetAnim_Mauville(void);
extern void InitTilesetAnim_MauvilleGameCorner(void);
extern void InitTilesetAnim_Mossdeep(void);
extern void InitTilesetAnim_Pacifidlog(void);
extern void InitTilesetAnim_Petalburg(void);
extern void InitTilesetAnim_Rustboro(void);
extern void InitTilesetAnim_Slateport(void);
extern void InitTilesetAnim_Underwater(void);

const u32 Cormoria_gTilesetTiles_AncientMirroh[] = INCBIN_U32("data/tilesets/cormoria/secondary/ancient_mirroh/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_AncientMirroh[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_AncientMirroh[] = INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_AncientMirroh[] = INCBIN_U16("data/tilesets/cormoria/secondary/ancient_mirroh/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_AncientMirroh =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_AncientMirroh,
    .palettes = Cormoria_gTilesetPalettes_AncientMirroh,
    .metatiles = Cormoria_gMetatiles_AncientMirroh,
    .metatileAttributes = Cormoria_gMetatileAttributes_AncientMirroh,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Autumn[] = INCBIN_U32("data/tilesets/cormoria/primary/autumn/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Autumn[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/autumn/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Autumn[] = INCBIN_U16("data/tilesets/cormoria/primary/autumn/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Autumn[] = INCBIN_U16("data/tilesets/cormoria/primary/autumn/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Autumn =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_Autumn,
    .palettes = Cormoria_gTilesetPalettes_Autumn,
    .metatiles = Cormoria_gMetatiles_Autumn,
    .metatileAttributes = Cormoria_gMetatileAttributes_Autumn,
    .callback = InitTilesetAnim_General,
};

const u32 Cormoria_gTilesetTiles_BattleDome[] = INCBIN_U32("data/tilesets/cormoria/secondary/battle_dome/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BattleDome[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BattleDome[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BattleDome[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_dome/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BattleDome =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BattleDome,
    .palettes = Cormoria_gTilesetPalettes_BattleDome,
    .metatiles = Cormoria_gMetatiles_BattleDome,
    .metatileAttributes = Cormoria_gMetatileAttributes_BattleDome,
    .callback = InitTilesetAnim_BattleDome,
};

const u32 Cormoria_gTilesetTiles_BattleFactory[] = INCBIN_U32("data/tilesets/cormoria/secondary/battle_factory/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BattleFactory[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BattleFactory[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BattleFactory[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_factory/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BattleFactory =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BattleFactory,
    .palettes = Cormoria_gTilesetPalettes_BattleFactory,
    .metatiles = Cormoria_gMetatiles_BattleFactory,
    .metatileAttributes = Cormoria_gMetatileAttributes_BattleFactory,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_BattleFrontier[] = INCBIN_U32("data/tilesets/cormoria/secondary/battle_frontier/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BattleFrontier[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BattleFrontier[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BattleFrontier[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BattleFrontier =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BattleFrontier,
    .palettes = Cormoria_gTilesetPalettes_BattleFrontier,
    .metatiles = Cormoria_gMetatiles_BattleFrontier,
    .metatileAttributes = Cormoria_gMetatileAttributes_BattleFrontier,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_BattleFrontierOutsideWest[] = INCBIN_U32("data/tilesets/cormoria/secondary/battle_frontier_outside_west/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BattleFrontierOutsideWest[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BattleFrontierOutsideWest[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BattleFrontierOutsideWest[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_outside_west/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BattleFrontierOutsideWest =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BattleFrontierOutsideWest,
    .palettes = Cormoria_gTilesetPalettes_BattleFrontierOutsideWest,
    .metatiles = Cormoria_gMetatiles_BattleFrontierOutsideWest,
    .metatileAttributes = Cormoria_gMetatileAttributes_BattleFrontierOutsideWest,
    .callback = InitTilesetAnim_BattleFrontierOutsideWest,
};

const u32 Cormoria_gTilesetTiles_BattleFrontierRankingHall[] = INCBIN_U32("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BattleFrontierRankingHall[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BattleFrontierRankingHall[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BattleFrontierRankingHall[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_frontier_ranking_hall/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BattleFrontierRankingHall =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BattleFrontierRankingHall,
    .palettes = Cormoria_gTilesetPalettes_BattleFrontierRankingHall,
    .metatiles = Cormoria_gMetatiles_BattleFrontierRankingHall,
    .metatileAttributes = Cormoria_gMetatileAttributes_BattleFrontierRankingHall,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_BattlePike[] = INCBIN_U32("data/tilesets/cormoria/secondary/battle_pike/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BattlePike[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BattlePike[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BattlePike[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_pike/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BattlePike =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BattlePike,
    .palettes = Cormoria_gTilesetPalettes_BattlePike,
    .metatiles = Cormoria_gMetatiles_BattlePike,
    .metatileAttributes = Cormoria_gMetatileAttributes_BattlePike,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_BattlePyramid[] = INCBIN_U32("data/tilesets/cormoria/secondary/battle_pyramid/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BattlePyramid[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BattlePyramid[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BattlePyramid[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_pyramid/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BattlePyramid =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BattlePyramid,
    .palettes = Cormoria_gTilesetPalettes_BattlePyramid,
    .metatiles = Cormoria_gMetatiles_BattlePyramid,
    .metatileAttributes = Cormoria_gMetatileAttributes_BattlePyramid,
    .callback = InitTilesetAnim_BattlePyramid,
};

const u32 Cormoria_gTilesetTiles_BattleTent[] = INCBIN_U32("data/tilesets/cormoria/secondary/battle_tent/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BattleTent[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BattleTent[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BattleTent[] = INCBIN_U16("data/tilesets/cormoria/secondary/battle_tent/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BattleTent =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BattleTent,
    .palettes = Cormoria_gTilesetPalettes_BattleTent,
    .metatiles = Cormoria_gMetatiles_BattleTent,
    .metatileAttributes = Cormoria_gMetatileAttributes_BattleTent,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_BikeShop[] = INCBIN_U32("data/tilesets/cormoria/secondary/bike_shop/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BikeShop[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BikeShop[] = INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BikeShop[] = INCBIN_U16("data/tilesets/cormoria/secondary/bike_shop/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BikeShop =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BikeShop,
    .palettes = Cormoria_gTilesetPalettes_BikeShop,
    .metatiles = Cormoria_gMetatiles_BikeShop,
    .metatileAttributes = Cormoria_gMetatileAttributes_BikeShop,
    .callback = InitTilesetAnim_BikeShop,
};

const u32 Cormoria_gTilesetTiles_BrendansMaysHouse[] = INCBIN_U32("data/tilesets/cormoria/secondary/brendans_mays_house/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_BrendansMaysHouse[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_BrendansMaysHouse[] = INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_BrendansMaysHouse[] = INCBIN_U16("data/tilesets/cormoria/secondary/brendans_mays_house/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_BrendansMaysHouse =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_BrendansMaysHouse,
    .palettes = Cormoria_gTilesetPalettes_BrendansMaysHouse,
    .metatiles = Cormoria_gMetatiles_BrendansMaysHouse,
    .metatileAttributes = Cormoria_gMetatileAttributes_BrendansMaysHouse,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_InsideBuilding[] = INCBIN_U32("data/tilesets/cormoria/primary/building/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_InsideBuilding[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/building/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_InsideBuilding[] = INCBIN_U16("data/tilesets/cormoria/primary/building/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_InsideBuilding[] = INCBIN_U16("data/tilesets/cormoria/primary/building/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Building =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_InsideBuilding,
    .palettes = Cormoria_gTilesetPalettes_InsideBuilding,
    .metatiles = Cormoria_gMetatiles_InsideBuilding,
    .metatileAttributes = Cormoria_gMetatileAttributes_InsideBuilding,
    .callback = InitTilesetAnim_Building,
};

const u32 Cormoria_gTilesetTiles_CableClub[] = INCBIN_U32("data/tilesets/cormoria/secondary/cable_club/tiles.4bpp");

const u16 Cormoria_gTilesetPalettes_CableClub[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_CableClub[] = INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_CableClub[] = INCBIN_U16("data/tilesets/cormoria/secondary/cable_club/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_CableClub =
{
    .isCompressed = FALSE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_CableClub,
    .palettes = Cormoria_gTilesetPalettes_CableClub,
    .metatiles = Cormoria_gMetatiles_CableClub,
    .metatileAttributes = Cormoria_gMetatileAttributes_CableClub,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_CaveAlt[] = INCBIN_U32("data/tilesets/cormoria/secondary/cave_alt/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_CaveAlt[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_CaveAlt[] = INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_CaveAlt[] = INCBIN_U16("data/tilesets/cormoria/secondary/cave_alt/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_CaveAlt =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_CaveAlt,
    .palettes = Cormoria_gTilesetPalettes_CaveAlt,
    .metatiles = Cormoria_gMetatiles_CaveAlt,
    .metatileAttributes = Cormoria_gMetatileAttributes_CaveAlt,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_CaveCity[] = INCBIN_U32("data/tilesets/cormoria/primary/cave_city/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_CaveCity[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/cave_city/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_CaveCity[] = INCBIN_U16("data/tilesets/cormoria/primary/cave_city/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_CaveCity[] = INCBIN_U16("data/tilesets/cormoria/primary/cave_city/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_CaveCity =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_CaveCity,
    .palettes = Cormoria_gTilesetPalettes_CaveCity,
    .metatiles = Cormoria_gMetatiles_CaveCity,
    .metatileAttributes = Cormoria_gMetatileAttributes_CaveCity,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Contest[] = INCBIN_U32("data/tilesets/cormoria/secondary/contest/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Contest[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/contest/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Contest[] = INCBIN_U16("data/tilesets/cormoria/secondary/contest/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Contest[] = INCBIN_U16("data/tilesets/cormoria/secondary/contest/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Contest =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Contest,
    .palettes = Cormoria_gTilesetPalettes_Contest,
    .metatiles = Cormoria_gMetatiles_Contest,
    .metatileAttributes = Cormoria_gMetatileAttributes_Contest,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Doroa[] = INCBIN_U32("data/tilesets/cormoria/primary/doroa/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Doroa[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/doroa/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Doroa[] = INCBIN_U16("data/tilesets/cormoria/primary/doroa/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Doroa[] = INCBIN_U16("data/tilesets/cormoria/primary/doroa/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Doroa =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_Doroa,
    .palettes = Cormoria_gTilesetPalettes_Doroa,
    .metatiles = Cormoria_gMetatiles_Doroa,
    .metatileAttributes = Cormoria_gMetatileAttributes_Doroa,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Facility[] = INCBIN_U32("data/tilesets/cormoria/secondary/facility/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Facility[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/facility/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Facility[] = INCBIN_U16("data/tilesets/cormoria/secondary/facility/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Facility[] = INCBIN_U16("data/tilesets/cormoria/secondary/facility/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Facility =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Facility,
    .palettes = Cormoria_gTilesetPalettes_Facility,
    .metatiles = Cormoria_gMetatiles_Facility,
    .metatileAttributes = Cormoria_gMetatileAttributes_Facility,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Fallarbor[] = INCBIN_U32("data/tilesets/cormoria/secondary/fallarbor/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Fallarbor[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Fallarbor[] = INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Fallarbor[] = INCBIN_U16("data/tilesets/cormoria/secondary/fallarbor/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Fallarbor =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Fallarbor,
    .palettes = Cormoria_gTilesetPalettes_Fallarbor,
    .metatiles = Cormoria_gMetatiles_Fallarbor,
    .metatileAttributes = Cormoria_gMetatileAttributes_Fallarbor,
    .callback = InitTilesetAnim_Fallarbor,
};

const u32 Cormoria_gTilesetTiles_Fortree[] = INCBIN_U32("data/tilesets/cormoria/secondary/fortree/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Fortree[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/fortree/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Fortree[] = INCBIN_U16("data/tilesets/cormoria/secondary/fortree/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Fortree[] = INCBIN_U16("data/tilesets/cormoria/secondary/fortree/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Fortree =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Fortree,
    .palettes = Cormoria_gTilesetPalettes_Fortree,
    .metatiles = Cormoria_gMetatiles_Fortree,
    .metatileAttributes = Cormoria_gMetatileAttributes_Fortree,
    .callback = InitTilesetAnim_Fortree,
};

const u32 Cormoria_gTilesetTiles_Gastree[] = INCBIN_U32("data/tilesets/cormoria/secondary/gastree/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Gastree[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/gastree/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Gastree[] = INCBIN_U16("data/tilesets/cormoria/secondary/gastree/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Gastree[] = INCBIN_U16("data/tilesets/cormoria/secondary/gastree/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Gastree =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Gastree,
    .palettes = Cormoria_gTilesetPalettes_Gastree,
    .metatiles = Cormoria_gMetatiles_Gastree,
    .metatileAttributes = Cormoria_gMetatileAttributes_Gastree,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_General[] = INCBIN_U32("data/tilesets/cormoria/primary/general/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_General[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/general/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_General[] = INCBIN_U16("data/tilesets/cormoria/primary/general/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_General[] = INCBIN_U16("data/tilesets/cormoria/primary/general/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_General =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_General,
    .palettes = Cormoria_gTilesetPalettes_General,
    .metatiles = Cormoria_gMetatiles_General,
    .metatileAttributes = Cormoria_gMetatileAttributes_General,
    .callback = InitTilesetAnim_General,
};

const u32 Cormoria_gTilesetTiles_GenericBuilding[] = INCBIN_U32("data/tilesets/cormoria/secondary/generic_building/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_GenericBuilding[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_GenericBuilding[] = INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_GenericBuilding[] = INCBIN_U16("data/tilesets/cormoria/secondary/generic_building/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_GenericBuilding =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_GenericBuilding,
    .palettes = Cormoria_gTilesetPalettes_GenericBuilding,
    .metatiles = Cormoria_gMetatiles_GenericBuilding,
    .metatileAttributes = Cormoria_gMetatileAttributes_GenericBuilding,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_InsideOfTruck[] = INCBIN_U32("data/tilesets/cormoria/secondary/inside_of_truck/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_InsideOfTruck[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_InsideOfTruck[] = INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_InsideOfTruck[] = INCBIN_U16("data/tilesets/cormoria/secondary/inside_of_truck/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_InsideOfTruck =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_InsideOfTruck,
    .palettes = Cormoria_gTilesetPalettes_InsideOfTruck,
    .metatiles = Cormoria_gMetatiles_InsideOfTruck,
    .metatileAttributes = Cormoria_gMetatileAttributes_InsideOfTruck,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_InsideShip[] = INCBIN_U32("data/tilesets/cormoria/secondary/inside_ship/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_InsideShip[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_InsideShip[] = INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_InsideShip[] = INCBIN_U16("data/tilesets/cormoria/secondary/inside_ship/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_InsideShip =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_InsideShip,
    .palettes = Cormoria_gTilesetPalettes_InsideShip,
    .metatiles = Cormoria_gMetatiles_InsideShip,
    .metatileAttributes = Cormoria_gMetatileAttributes_InsideShip,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Lab[] = INCBIN_U32("data/tilesets/cormoria/secondary/lab/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Lab[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lab/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Lab[] = INCBIN_U16("data/tilesets/cormoria/secondary/lab/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Lab[] = INCBIN_U16("data/tilesets/cormoria/secondary/lab/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Lab =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Lab,
    .palettes = Cormoria_gTilesetPalettes_Lab,
    .metatiles = Cormoria_gMetatiles_Lab,
    .metatileAttributes = Cormoria_gMetatileAttributes_Lab,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Lavaridge[] = INCBIN_U32("data/tilesets/cormoria/secondary/lavaridge/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Lavaridge[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Lavaridge[] = INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Lavaridge[] = INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Lavaridge =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Lavaridge,
    .palettes = Cormoria_gTilesetPalettes_Lavaridge,
    .metatiles = Cormoria_gMetatiles_Lavaridge,
    .metatileAttributes = Cormoria_gMetatileAttributes_Lavaridge,
    .callback = InitTilesetAnim_Lavaridge,
};

const u32 Cormoria_gTilesetTiles_LavaridgeGym[] = INCBIN_U32("data/tilesets/cormoria/secondary/lavaridge_gym/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_LavaridgeGym[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_LavaridgeGym[] = INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_LavaridgeGym[] = INCBIN_U16("data/tilesets/cormoria/secondary/lavaridge_gym/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_LavaridgeGym =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_LavaridgeGym,
    .palettes = Cormoria_gTilesetPalettes_LavaridgeGym,
    .metatiles = Cormoria_gMetatiles_LavaridgeGym,
    .metatileAttributes = Cormoria_gMetatileAttributes_LavaridgeGym,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_LilycoveMuseum[] = INCBIN_U32("data/tilesets/cormoria/secondary/lilycove_museum/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_LilycoveMuseum[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_LilycoveMuseum[] = INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_LilycoveMuseum[] = INCBIN_U16("data/tilesets/cormoria/secondary/lilycove_museum/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_LilycoveMuseum =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_LilycoveMuseum,
    .palettes = Cormoria_gTilesetPalettes_LilycoveMuseum,
    .metatiles = Cormoria_gMetatiles_LilycoveMuseum,
    .metatileAttributes = Cormoria_gMetatileAttributes_LilycoveMuseum,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Mauville[] = INCBIN_U32("data/tilesets/cormoria/secondary/mauville/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Mauville[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Mauville[] = INCBIN_U16("data/tilesets/cormoria/secondary/mauville/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Mauville[] = INCBIN_U16("data/tilesets/cormoria/secondary/mauville/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Mauville =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Mauville,
    .palettes = Cormoria_gTilesetPalettes_Mauville,
    .metatiles = Cormoria_gMetatiles_Mauville,
    .metatileAttributes = Cormoria_gMetatileAttributes_Mauville,
    .callback = InitTilesetAnim_Mauville,
};

const u32 Cormoria_gTilesetTiles_MauvilleGameCorner[] = INCBIN_U32("data/tilesets/cormoria/secondary/mauville_game_corner/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_MauvilleGameCorner[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_MauvilleGameCorner[] = INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_MauvilleGameCorner[] = INCBIN_U16("data/tilesets/cormoria/secondary/mauville_game_corner/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_MauvilleGameCorner =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_MauvilleGameCorner,
    .palettes = Cormoria_gTilesetPalettes_MauvilleGameCorner,
    .metatiles = Cormoria_gMetatiles_MauvilleGameCorner,
    .metatileAttributes = Cormoria_gMetatileAttributes_MauvilleGameCorner,
    .callback = InitTilesetAnim_MauvilleGameCorner,
};

const u32 Cormoria_gTilesetTiles_MeteorFalls[] = INCBIN_U32("data/tilesets/cormoria/secondary/meteor_falls/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_MeteorFalls[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_MeteorFalls[] = INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_MeteorFalls[] = INCBIN_U16("data/tilesets/cormoria/secondary/meteor_falls/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_MeteorFalls =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_MeteorFalls,
    .palettes = Cormoria_gTilesetPalettes_MeteorFalls,
    .metatiles = Cormoria_gMetatiles_MeteorFalls,
    .metatileAttributes = Cormoria_gMetatileAttributes_MeteorFalls,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_MirrohSnow[] = INCBIN_U32("data/tilesets/cormoria/primary/mirroh_snow/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_MirrohSnow[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_MirrohSnow[] = INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_MirrohSnow[] = INCBIN_U16("data/tilesets/cormoria/primary/mirroh_snow/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_MirrohSnow =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_MirrohSnow,
    .palettes = Cormoria_gTilesetPalettes_MirrohSnow,
    .metatiles = Cormoria_gMetatiles_MirrohSnow,
    .metatileAttributes = Cormoria_gMetatileAttributes_MirrohSnow,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Mossdeep[] = INCBIN_U32("data/tilesets/cormoria/secondary/mossdeep/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Mossdeep[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Mossdeep[] = INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Mossdeep[] = INCBIN_U16("data/tilesets/cormoria/secondary/mossdeep/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Mossdeep =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Mossdeep,
    .palettes = Cormoria_gTilesetPalettes_Mossdeep,
    .metatiles = Cormoria_gMetatiles_Mossdeep,
    .metatileAttributes = Cormoria_gMetatileAttributes_Mossdeep,
    .callback = InitTilesetAnim_Mossdeep,
};

const u32 Cormoria_gTilesetTiles_Pacifidlog[] = INCBIN_U32("data/tilesets/cormoria/secondary/pacifidlog/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Pacifidlog[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Pacifidlog[] = INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Pacifidlog[] = INCBIN_U16("data/tilesets/cormoria/secondary/pacifidlog/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Pacifidlog =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Pacifidlog,
    .palettes = Cormoria_gTilesetPalettes_Pacifidlog,
    .metatiles = Cormoria_gMetatiles_Pacifidlog,
    .metatileAttributes = Cormoria_gMetatileAttributes_Pacifidlog,
    .callback = InitTilesetAnim_Pacifidlog,
};

const u32 Cormoria_gTilesetTiles_Petalburg[] = INCBIN_U32("data/tilesets/cormoria/secondary/petalburg/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Petalburg[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Petalburg[] = INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Petalburg[] = INCBIN_U16("data/tilesets/cormoria/secondary/petalburg/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Petalburg =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Petalburg,
    .palettes = Cormoria_gTilesetPalettes_Petalburg,
    .metatiles = Cormoria_gMetatiles_Petalburg,
    .metatileAttributes = Cormoria_gMetatileAttributes_Petalburg,
    .callback = InitTilesetAnim_Petalburg,
};

const u32 Cormoria_gTilesetTiles_PokemonCenter[] = INCBIN_U32("data/tilesets/cormoria/secondary/pokemon_center/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_PokemonCenter[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_PokemonCenter[] = INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_PokemonCenter[] = INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_center/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_PokemonCenter =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_PokemonCenter,
    .palettes = Cormoria_gTilesetPalettes_PokemonCenter,
    .metatiles = Cormoria_gMetatiles_PokemonCenter,
    .metatileAttributes = Cormoria_gMetatileAttributes_PokemonCenter,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_PokemonFanClub[] = INCBIN_U32("data/tilesets/cormoria/secondary/pokemon_fan_club/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_PokemonFanClub[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_PokemonFanClub[] = INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_PokemonFanClub[] = INCBIN_U16("data/tilesets/cormoria/secondary/pokemon_fan_club/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_PokemonFanClub =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_PokemonFanClub,
    .palettes = Cormoria_gTilesetPalettes_PokemonFanClub,
    .metatiles = Cormoria_gMetatiles_PokemonFanClub,
    .metatileAttributes = Cormoria_gMetatileAttributes_PokemonFanClub,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_PrettyPetalFlowerShop[] = INCBIN_U32("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_PrettyPetalFlowerShop[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_PrettyPetalFlowerShop[] = INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_PrettyPetalFlowerShop[] = INCBIN_U16("data/tilesets/cormoria/secondary/pretty_petal_flower_shop/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_PrettyPetalFlowerShop =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_PrettyPetalFlowerShop,
    .palettes = Cormoria_gTilesetPalettes_PrettyPetalFlowerShop,
    .metatiles = Cormoria_gMetatiles_PrettyPetalFlowerShop,
    .metatileAttributes = Cormoria_gMetatileAttributes_PrettyPetalFlowerShop,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Restaurant[] = INCBIN_U32("data/tilesets/cormoria/secondary/restaurant/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Restaurant[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Restaurant[] = INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Restaurant[] = INCBIN_U16("data/tilesets/cormoria/secondary/restaurant/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Restaurant =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Restaurant,
    .palettes = Cormoria_gTilesetPalettes_Restaurant,
    .metatiles = Cormoria_gMetatiles_Restaurant,
    .metatileAttributes = Cormoria_gMetatileAttributes_Restaurant,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Rivetshore[] = INCBIN_U32("data/tilesets/cormoria/primary/rivetshore/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Rivetshore[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Rivetshore[] = INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Rivetshore[] = INCBIN_U16("data/tilesets/cormoria/primary/rivetshore/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Rivetshore =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_Rivetshore,
    .palettes = Cormoria_gTilesetPalettes_Rivetshore,
    .metatiles = Cormoria_gMetatiles_Rivetshore,
    .metatileAttributes = Cormoria_gMetatileAttributes_Rivetshore,
    .callback = InitTilesetAnim_General,
};

const u32 Cormoria_gTilesetTiles_Rustboro[] = INCBIN_U32("data/tilesets/cormoria/secondary/rustboro/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Rustboro[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Rustboro[] = INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Rustboro[] = INCBIN_U16("data/tilesets/cormoria/secondary/rustboro/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Rustboro =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Rustboro,
    .palettes = Cormoria_gTilesetPalettes_Rustboro,
    .metatiles = Cormoria_gMetatiles_Rustboro,
    .metatileAttributes = Cormoria_gMetatileAttributes_Rustboro,
    .callback = InitTilesetAnim_Rustboro,
};

const u32 Cormoria_gTilesetTiles_SSElegant[] = INCBIN_U32("data/tilesets/cormoria/secondary/sselegant/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_SSElegant[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_SSElegant[] = INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_SSElegant[] = INCBIN_U16("data/tilesets/cormoria/secondary/sselegant/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_SSElegant =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_SSElegant,
    .palettes = Cormoria_gTilesetPalettes_SSElegant,
    .metatiles = Cormoria_gMetatiles_SSElegant,
    .metatileAttributes = Cormoria_gMetatileAttributes_SSElegant,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_SeashoreHouse[] = INCBIN_U32("data/tilesets/cormoria/secondary/seashore_house/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_SeashoreHouse[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_SeashoreHouse[] = INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_SeashoreHouse[] = INCBIN_U16("data/tilesets/cormoria/secondary/seashore_house/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_SeashoreHouse =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_SeashoreHouse,
    .palettes = Cormoria_gTilesetPalettes_SeashoreHouse,
    .metatiles = Cormoria_gMetatiles_SeashoreHouse,
    .metatileAttributes = Cormoria_gMetatileAttributes_SeashoreHouse,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Sewers[] = INCBIN_U32("data/tilesets/cormoria/secondary/sewers/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Sewers[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/sewers/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Sewers[] = INCBIN_U16("data/tilesets/cormoria/secondary/sewers/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Sewers[] = INCBIN_U16("data/tilesets/cormoria/secondary/sewers/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Sewers =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Sewers,
    .palettes = Cormoria_gTilesetPalettes_Sewers,
    .metatiles = Cormoria_gMetatiles_Sewers,
    .metatileAttributes = Cormoria_gMetatileAttributes_Sewers,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Shop[] = INCBIN_U32("data/tilesets/cormoria/secondary/shop/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Shop[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/shop/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Shop[] = INCBIN_U16("data/tilesets/cormoria/secondary/shop/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Shop[] = INCBIN_U16("data/tilesets/cormoria/secondary/shop/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Shop =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Shop,
    .palettes = Cormoria_gTilesetPalettes_Shop,
    .metatiles = Cormoria_gMetatiles_Shop,
    .metatileAttributes = Cormoria_gMetatileAttributes_Shop,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Silversun[] = INCBIN_U32("data/tilesets/cormoria/secondary/silversun/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Silversun[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/silversun/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Silversun[] = INCBIN_U16("data/tilesets/cormoria/secondary/silversun/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Silversun[] = INCBIN_U16("data/tilesets/cormoria/secondary/silversun/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Silversun =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Silversun,
    .palettes = Cormoria_gTilesetPalettes_Silversun,
    .metatiles = Cormoria_gMetatiles_Silversun,
    .metatileAttributes = Cormoria_gMetatileAttributes_Silversun,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_SilversunPrimary[] = INCBIN_U32("data/tilesets/cormoria/primary/silversun_primary/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_SilversunPrimary[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_SilversunPrimary[] = INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_SilversunPrimary[] = INCBIN_U16("data/tilesets/cormoria/primary/silversun_primary/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_SilversunPrimary =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_SilversunPrimary,
    .palettes = Cormoria_gTilesetPalettes_SilversunPrimary,
    .metatiles = Cormoria_gMetatiles_SilversunPrimary,
    .metatileAttributes = Cormoria_gMetatileAttributes_SilversunPrimary,
    .callback = InitTilesetAnim_General,
};

const u32 Cormoria_gTilesetTiles_Slateport[] = INCBIN_U32("data/tilesets/cormoria/secondary/slateport/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Slateport[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/slateport/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Slateport[] = INCBIN_U16("data/tilesets/cormoria/secondary/slateport/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Slateport[] = INCBIN_U16("data/tilesets/cormoria/secondary/slateport/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Slateport =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Slateport,
    .palettes = Cormoria_gTilesetPalettes_Slateport,
    .metatiles = Cormoria_gMetatiles_Slateport,
    .metatileAttributes = Cormoria_gMetatileAttributes_Slateport,
    .callback = InitTilesetAnim_Slateport,
};

const u32 Cormoria_gTilesetTiles_Snow[] = INCBIN_U32("data/tilesets/cormoria/primary/snow/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Snow[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/snow/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Snow[] = INCBIN_U16("data/tilesets/cormoria/primary/snow/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Snow[] = INCBIN_U16("data/tilesets/cormoria/primary/snow/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Snow =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_Snow,
    .palettes = Cormoria_gTilesetPalettes_Snow,
    .metatiles = Cormoria_gMetatiles_Snow,
    .metatileAttributes = Cormoria_gMetatileAttributes_Snow,
    .callback = InitTilesetAnim_General,
};

const u32 Cormoria_gTilesetTiles_SnowCableCar[] = INCBIN_U32("data/tilesets/cormoria/secondary/snow_cable_car/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_SnowCableCar[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_SnowCableCar[] = INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_SnowCableCar[] = INCBIN_U16("data/tilesets/cormoria/secondary/snow_cable_car/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_SnowCableCar =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_SnowCableCar,
    .palettes = Cormoria_gTilesetPalettes_SnowCableCar,
    .metatiles = Cormoria_gMetatiles_SnowCableCar,
    .metatileAttributes = Cormoria_gMetatileAttributes_SnowCableCar,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Swamp[] = INCBIN_U32("data/tilesets/cormoria/primary/swamp/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Swamp[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/swamp/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Swamp[] = INCBIN_U16("data/tilesets/cormoria/primary/swamp/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Swamp[] = INCBIN_U16("data/tilesets/cormoria/primary/swamp/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Swamp =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_Swamp,
    .palettes = Cormoria_gTilesetPalettes_Swamp,
    .metatiles = Cormoria_gMetatiles_Swamp,
    .metatileAttributes = Cormoria_gMetatileAttributes_Swamp,
    .callback = InitTilesetAnim_General,
};

const u32 Cormoria_gTilesetTiles_Swampforest[] = INCBIN_U32("data/tilesets/cormoria/secondary/swampforest/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Swampforest[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Swampforest[] = INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Swampforest[] = INCBIN_U16("data/tilesets/cormoria/secondary/swampforest/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Swampforest =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Swampforest,
    .palettes = Cormoria_gTilesetPalettes_Swampforest,
    .metatiles = Cormoria_gMetatiles_Swampforest,
    .metatileAttributes = Cormoria_gMetatileAttributes_Swampforest,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_Underwater[] = INCBIN_U32("data/tilesets/cormoria/secondary/underwater/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Underwater[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/12.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/13.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/14.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/underwater/palettes/15.gbapal"),
};

const u16 Cormoria_gMetatiles_Underwater[] = INCBIN_U16("data/tilesets/cormoria/secondary/underwater/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Underwater[] = INCBIN_U16("data/tilesets/cormoria/secondary/underwater/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Underwater =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_Underwater,
    .palettes = Cormoria_gTilesetPalettes_Underwater,
    .metatiles = Cormoria_gMetatiles_Underwater,
    .metatileAttributes = Cormoria_gMetatileAttributes_Underwater,
    .callback = InitTilesetAnim_Underwater,
};

const u32 Cormoria_gTilesetTiles_Vilethorn[] = INCBIN_U32("data/tilesets/cormoria/primary/vilethorn/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_Vilethorn[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_Vilethorn[] = INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_Vilethorn[] = INCBIN_U16("data/tilesets/cormoria/primary/vilethorn/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_Vilethorn =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_Vilethorn,
    .palettes = Cormoria_gTilesetPalettes_Vilethorn,
    .metatiles = Cormoria_gMetatiles_Vilethorn,
    .metatileAttributes = Cormoria_gMetatileAttributes_Vilethorn,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_WinterlilyGym[] = INCBIN_U32("data/tilesets/cormoria/secondary/winterlily_gym/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_WinterlilyGym[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_WinterlilyGym[] = INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_WinterlilyGym[] = INCBIN_U16("data/tilesets/cormoria/secondary/winterlily_gym/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_WinterlilyGym =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_WinterlilyGym,
    .palettes = Cormoria_gTilesetPalettes_WinterlilyGym,
    .metatiles = Cormoria_gMetatiles_WinterlilyGym,
    .metatileAttributes = Cormoria_gMetatileAttributes_WinterlilyGym,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_ZarudeForest[] = INCBIN_U32("data/tilesets/cormoria/primary/zarude_forest/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_ZarudeForest[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_ZarudeForest[] = INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_ZarudeForest[] = INCBIN_U16("data/tilesets/cormoria/primary/zarude_forest/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_ZarudeForest =
{
    .isCompressed = TRUE,
    .isSecondary = FALSE,
    .tiles = Cormoria_gTilesetTiles_ZarudeForest,
    .palettes = Cormoria_gTilesetPalettes_ZarudeForest,
    .metatiles = Cormoria_gMetatiles_ZarudeForest,
    .metatileAttributes = Cormoria_gMetatileAttributes_ZarudeForest,
    .callback = NULL,
};

const u32 Cormoria_gTilesetTiles_ZarudeSecondary[] = INCBIN_U32("data/tilesets/cormoria/secondary/zarude_secondary/tiles.4bpp.lz");

const u16 Cormoria_gTilesetPalettes_ZarudeSecondary[][16] =
{
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/00.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/01.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/02.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/03.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/04.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/05.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/06.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/07.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/08.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/09.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/10.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/11.gbapal"),
    INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/palettes/12.gbapal"),
};

const u16 Cormoria_gMetatiles_ZarudeSecondary[] = INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/metatiles.bin");

const u16 Cormoria_gMetatileAttributes_ZarudeSecondary[] = INCBIN_U16("data/tilesets/cormoria/secondary/zarude_secondary/metatile_attributes.bin");

const struct Tileset Cormoria_gTileset_ZarudeSecondary =
{
    .isCompressed = TRUE,
    .isSecondary = TRUE,
    .tiles = Cormoria_gTilesetTiles_ZarudeSecondary,
    .palettes = Cormoria_gTilesetPalettes_ZarudeSecondary,
    .metatiles = Cormoria_gMetatiles_ZarudeSecondary,
    .metatileAttributes = Cormoria_gMetatileAttributes_ZarudeSecondary,
    .callback = NULL,
};
