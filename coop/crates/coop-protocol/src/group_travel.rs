//! Strict fixed-size records for consented Johto group travel.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const GROUP_TRAVEL_RECORD_SIZE: usize = 32;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelRoute {
    TrainOriginal = 1,
    TrainLater = 2,
    FerryOriginal = 3,
    FerryLater = 4,
    GateOriginal = 5,
    GateLater = 6,
    /// Consent-gated Fly to the Hoenn home town.  This first Fly route is
    /// intentionally fixed while the cloud progress ledger remains the
    /// authority for the player's unlocked Fly destinations.
    FlyLittleroot = 7,
    FlyJohtoNewbark = 8,
    FlyJohtoCherrygrove = 9,
    FlyJohtoViolet = 10,
    FlyJohtoAzalea = 11,
    FlyJohtoGoldenrod = 12,
    FlyJohtoEcruteak = 13,
    FlyJohtoOlivine = 14,
    FlyJohtoCianwood = 15,
    FlyJohtoMahogany = 16,
    FlyJohtoBlackthorn = 17,
    FlyHoennOldale = 18,
    FlyHoennDewford = 19,
    FlyHoennLavaridge = 20,
    FlyHoennFallarbor = 21,
    FlyHoennVerdanturf = 22,
    FlyHoennPacifidlog = 23,
    FlyHoennPetalburg = 24,
    FlyHoennSlateport = 25,
    FlyHoennMauville = 26,
    FlyHoennRustboro = 27,
    FlyHoennFortree = 28,
    FlyHoennLilycove = 29,
    FlyHoennMossdeep = 30,
    FlyHoennSootopolis = 31,
    // Wire value 32 is reserved for the omitted multi-point Ever Grande
    // destination and must remain rejected by from_wire.
    FlyKantoOriginalPallet = 33,
    FlyKantoOriginalViridian = 34,
    FlyKantoOriginalPewter = 35,
    FlyKantoOriginalCerulean = 36,
    FlyKantoOriginalLavender = 37,
    FlyKantoOriginalVermilion = 38,
    FlyKantoOriginalCeladon = 39,
    FlyKantoOriginalFuchsia = 40,
    FlyKantoOriginalCinnabar = 41,
    FlyKantoOriginalIndigo = 42,
    FlyKantoOriginalSaffron = 43,
    FlyKantoLaterPallet = 44,
    FlyKantoLaterViridian = 45,
    FlyKantoLaterPewter = 46,
    FlyKantoLaterCerulean = 47,
    FlyKantoLaterLavender = 48,
    FlyKantoLaterVermilion = 49,
    FlyKantoLaterCeladon = 50,
    FlyKantoLaterFuchsia = 51,
    FlyKantoLaterSaffron = 52,
    FlyKantoLaterCinnabar = 53,
    FlySeviiOneIsland = 54,
    FlySeviiTwoIsland = 55,
    FlySeviiThreeIsland = 56,
    FlySeviiFourIsland = 57,
    FlySeviiFiveIsland = 58,
    FlySeviiSevenIsland = 59,
    FlySeviiSixIsland = 60,
    FlyKantoRoute4PokemonCenter = 61,
    FlyKantoRoute10PokemonCenter = 62,
    FlyHoennEverGrandeCenter = 63,
    FlyHoennEverGrandeLeague = 64,
    FlyHoennBattleFrontier = 65,
    ReturnFerryOriginal = 66,
    ReturnFerryLater = 67,
    ReturnTrainOriginal = 68,
    ReturnTrainLater = 69,
    ReturnGateOriginal = 70,
    ReturnGateLater = 71,
    FerryOlivineSouthernIsland = 72,
    FerryOlivineBirthIsland = 73,
    FerryOlivineFarawayIsland = 74,
    FerryOlivineBattleFrontier = 75,
    FerryVermilionSouthernIsland = 76,
    FerryVermilionBirthIsland = 77,
    FerryVermilionFarawayIsland = 78,
    FerryVermilionBattleFrontier = 79,
    FerrySouthernIslandLilycove = 80,
    FerryBirthIslandLilycove = 81,
    FerryFarawayIslandLilycove = 82,
    FerryBattleFrontierSlateport = 83,
    FerryBattleFrontierLilycove = 84,
    FerryLilycoveSouthernIsland = 85,
    FerryLilycoveNavelRock = 86,
    FerryLilycoveBirthIsland = 87,
    FerryLilycoveFarawayIsland = 88,
    FerryLilycoveBattleFrontier = 89,
    FerrySlateportBattleFrontier = 90,
    FerryNavelRockLilycove = 91,
    FerrySSTidalSlateportBoard = 92,
    FerrySSTidalLilycoveBoard = 93,
    FerrySSTidalLilycoveExit = 94,
    FerrySSTidalSlateportExit = 95,
    /// First Briney voyage remains absent from the public route catalog until
    /// its scene checkpoint and save receipts are wired end to end.
    FerryBrineyHouseDewford = 96,
    FerryDewfordBrineyHouse = 97,
    FerryDewfordRoute109 = 98,
    FerryRoute109Dewford = 99,
    SeagallopVermilionOne = 100,
    SeagallopVermilionTwo = 101,
    SeagallopVermilionThree = 102,
    SeagallopVermilionFour = 103,
    SeagallopVermilionFive = 104,
    SeagallopVermilionSix = 105,
    SeagallopVermilionSeven = 106,
    SeagallopOneVermilion = 107,
    SeagallopOneTwo = 108,
    SeagallopOneThree = 109,
    SeagallopOneFour = 110,
    SeagallopOneFive = 111,
    SeagallopOneSix = 112,
    SeagallopOneSeven = 113,
    SeagallopTwoVermilion = 114,
    SeagallopTwoOne = 115,
    SeagallopTwoThree = 116,
    SeagallopTwoFour = 117,
    SeagallopTwoFive = 118,
    SeagallopTwoSix = 119,
    SeagallopTwoSeven = 120,
    SeagallopThreeVermilion = 121,
    SeagallopThreeOne = 122,
    SeagallopThreeTwo = 123,
    SeagallopThreeFour = 124,
    SeagallopThreeFive = 125,
    SeagallopThreeSix = 126,
    SeagallopThreeSeven = 127,
    SeagallopFourVermilion = 128,
    SeagallopFourOne = 129,
    SeagallopFourTwo = 130,
    SeagallopFourThree = 131,
    SeagallopFourFive = 132,
    SeagallopFourSix = 133,
    SeagallopFourSeven = 134,
    SeagallopFiveVermilion = 135,
    SeagallopFiveOne = 136,
    SeagallopFiveTwo = 137,
    SeagallopFiveThree = 138,
    SeagallopFiveFour = 139,
    SeagallopFiveSix = 140,
    SeagallopFiveSeven = 141,
    SeagallopSixVermilion = 142,
    SeagallopSixOne = 143,
    SeagallopSixTwo = 144,
    SeagallopSixThree = 145,
    SeagallopSixFour = 146,
    SeagallopSixFive = 147,
    SeagallopSixSeven = 148,
    SeagallopSevenVermilion = 149,
    SeagallopSevenOne = 150,
    SeagallopSevenTwo = 151,
    SeagallopSevenThree = 152,
    SeagallopSevenFour = 153,
    SeagallopSevenFive = 154,
    SeagallopSevenSix = 155,
    SeagallopVermilionNavel = 156,
    SeagallopNavelVermilion = 157,
    SeagallopVermilionBirth = 158,
    SeagallopBirthVermilion = 159,
    SeagallopBillCinnabarOne = 160,
    SeagallopBillOneCinnabar = 161,
    CableCarRoute112MtChimney = 162,
    CableCarMtChimneyRoute112 = 163,
    /// Consent-gated Dig to the exact map/coordinate reported by the ROM.
    Dig = 164,
    /// Consent-gated Escape Rope to the exact map/coordinate reported by the ROM.
    EscapeRope = 165,
}

impl GroupTravelRoute {
    #[must_use]
    pub const fn is_dynamic(self) -> bool {
        matches!(self, Self::Dig | Self::EscapeRope)
    }

    #[must_use]
    pub const fn era(self) -> GroupTravelEra {
        match self {
            Self::TrainOriginal
            | Self::FerryOriginal
            | Self::GateOriginal
            | Self::ReturnFerryOriginal
            | Self::ReturnTrainOriginal
            | Self::ReturnGateOriginal => GroupTravelEra::Original,
            Self::TrainLater
            | Self::FerryLater
            | Self::GateLater
            | Self::ReturnFerryLater
            | Self::ReturnTrainLater
            | Self::ReturnGateLater => GroupTravelEra::Later,
            Self::FerryOlivineSouthernIsland
            | Self::FerryOlivineBirthIsland
            | Self::FerryOlivineFarawayIsland
            | Self::FerryOlivineBattleFrontier
            | Self::FerryVermilionSouthernIsland
            | Self::FerryVermilionBirthIsland
            | Self::FerryVermilionFarawayIsland
            | Self::FerryVermilionBattleFrontier
            | Self::FerrySouthernIslandLilycove
            | Self::FerryBirthIslandLilycove
            | Self::FerryFarawayIslandLilycove
            | Self::FerryBattleFrontierSlateport
            | Self::FerryBattleFrontierLilycove
            | Self::FerryLilycoveSouthernIsland
            | Self::FerryLilycoveNavelRock
            | Self::FerryLilycoveBirthIsland
            | Self::FerryLilycoveFarawayIsland
            | Self::FerryLilycoveBattleFrontier
            | Self::FerrySlateportBattleFrontier
            | Self::FerryNavelRockLilycove
            | Self::FerrySSTidalSlateportBoard
            | Self::FerrySSTidalLilycoveBoard
            | Self::FerrySSTidalLilycoveExit
            | Self::FerrySSTidalSlateportExit
            | Self::FerryBrineyHouseDewford
            | Self::FerryDewfordBrineyHouse
            | Self::FerryDewfordRoute109
            | Self::FerryRoute109Dewford => GroupTravelEra::Hoenn,
            Self::SeagallopVermilionOne
            | Self::SeagallopVermilionTwo
            | Self::SeagallopVermilionThree
            | Self::SeagallopVermilionFour
            | Self::SeagallopVermilionFive
            | Self::SeagallopVermilionSix
            | Self::SeagallopVermilionSeven
            | Self::SeagallopOneTwo
            | Self::SeagallopOneThree
            | Self::SeagallopOneFour
            | Self::SeagallopOneFive
            | Self::SeagallopOneSix
            | Self::SeagallopOneSeven
            | Self::SeagallopTwoOne
            | Self::SeagallopTwoThree
            | Self::SeagallopTwoFour
            | Self::SeagallopTwoFive
            | Self::SeagallopTwoSix
            | Self::SeagallopTwoSeven
            | Self::SeagallopThreeOne
            | Self::SeagallopThreeTwo
            | Self::SeagallopThreeFour
            | Self::SeagallopThreeFive
            | Self::SeagallopThreeSix
            | Self::SeagallopThreeSeven
            | Self::SeagallopFourOne
            | Self::SeagallopFourTwo
            | Self::SeagallopFourThree
            | Self::SeagallopFourFive
            | Self::SeagallopFourSix
            | Self::SeagallopFourSeven
            | Self::SeagallopFiveOne
            | Self::SeagallopFiveTwo
            | Self::SeagallopFiveThree
            | Self::SeagallopFiveFour
            | Self::SeagallopFiveSix
            | Self::SeagallopFiveSeven
            | Self::SeagallopSixOne
            | Self::SeagallopSixTwo
            | Self::SeagallopSixThree
            | Self::SeagallopSixFour
            | Self::SeagallopSixFive
            | Self::SeagallopSixSeven
            | Self::SeagallopSevenOne
            | Self::SeagallopSevenTwo
            | Self::SeagallopSevenThree
            | Self::SeagallopSevenFour
            | Self::SeagallopSevenFive
            | Self::SeagallopSevenSix
            | Self::SeagallopVermilionNavel
            | Self::SeagallopVermilionBirth => GroupTravelEra::Sevii,
            Self::SeagallopBillCinnabarOne => GroupTravelEra::Original,
            Self::SeagallopOneVermilion
            | Self::SeagallopTwoVermilion
            | Self::SeagallopThreeVermilion
            | Self::SeagallopFourVermilion
            | Self::SeagallopFiveVermilion
            | Self::SeagallopSixVermilion
            | Self::SeagallopSevenVermilion
            | Self::SeagallopNavelVermilion
            | Self::SeagallopBirthVermilion
            | Self::SeagallopBillOneCinnabar => GroupTravelEra::Original,
            Self::FlyLittleroot
            | Self::FlyHoennOldale
            | Self::FlyHoennDewford
            | Self::FlyHoennLavaridge
            | Self::FlyHoennFallarbor
            | Self::FlyHoennVerdanturf
            | Self::FlyHoennPacifidlog
            | Self::FlyHoennPetalburg
            | Self::FlyHoennSlateport
            | Self::FlyHoennMauville
            | Self::FlyHoennRustboro
            | Self::FlyHoennFortree
            | Self::FlyHoennLilycove
            | Self::FlyHoennMossdeep
            | Self::FlyHoennSootopolis
            | Self::FlyHoennEverGrandeCenter
            | Self::FlyHoennEverGrandeLeague
            | Self::FlyHoennBattleFrontier => GroupTravelEra::Hoenn,
            Self::CableCarRoute112MtChimney | Self::CableCarMtChimneyRoute112 => {
                GroupTravelEra::Hoenn
            }
            Self::Dig | Self::EscapeRope => GroupTravelEra::Hoenn,
            Self::FlyJohtoNewbark
            | Self::FlyJohtoCherrygrove
            | Self::FlyJohtoViolet
            | Self::FlyJohtoAzalea
            | Self::FlyJohtoGoldenrod
            | Self::FlyJohtoEcruteak
            | Self::FlyJohtoOlivine
            | Self::FlyJohtoCianwood
            | Self::FlyJohtoMahogany
            | Self::FlyJohtoBlackthorn => GroupTravelEra::Johto,
            Self::FlySeviiOneIsland
            | Self::FlySeviiTwoIsland
            | Self::FlySeviiThreeIsland
            | Self::FlySeviiFourIsland
            | Self::FlySeviiFiveIsland
            | Self::FlySeviiSevenIsland
            | Self::FlySeviiSixIsland => GroupTravelEra::Sevii,
            Self::FlyKantoOriginalPallet
            | Self::FlyKantoOriginalViridian
            | Self::FlyKantoOriginalPewter
            | Self::FlyKantoOriginalCerulean
            | Self::FlyKantoOriginalLavender
            | Self::FlyKantoOriginalVermilion
            | Self::FlyKantoOriginalCeladon
            | Self::FlyKantoOriginalFuchsia
            | Self::FlyKantoOriginalCinnabar
            | Self::FlyKantoOriginalIndigo
            | Self::FlyKantoOriginalSaffron
            | Self::FlyKantoRoute4PokemonCenter
            | Self::FlyKantoRoute10PokemonCenter => GroupTravelEra::Original,
            Self::FlyKantoLaterPallet
            | Self::FlyKantoLaterViridian
            | Self::FlyKantoLaterPewter
            | Self::FlyKantoLaterCerulean
            | Self::FlyKantoLaterLavender
            | Self::FlyKantoLaterVermilion
            | Self::FlyKantoLaterCeladon
            | Self::FlyKantoLaterFuchsia
            | Self::FlyKantoLaterSaffron
            | Self::FlyKantoLaterCinnabar => GroupTravelEra::Later,
        }
    }

    #[must_use]
    pub const fn is_fly(self) -> bool {
        self as u8 >= Self::FlyLittleroot as u8 && self as u8 <= Self::FlyHoennBattleFrontier as u8
    }

    #[must_use]
    pub const fn destination(self) -> GroupTravelDestination {
        match self {
            Self::TrainOriginal => GroupTravelDestination::OriginalSaffron,
            Self::TrainLater => GroupTravelDestination::LaterSaffron,
            Self::FerryOriginal => GroupTravelDestination::OriginalVermilion,
            Self::FerryLater => GroupTravelDestination::LaterVermilion,
            Self::GateOriginal => GroupTravelDestination::OriginalRoute22,
            Self::GateLater => GroupTravelDestination::LaterRoute22,
            Self::ReturnFerryOriginal | Self::ReturnFerryLater => {
                GroupTravelDestination::JohtoOlivine
            }
            Self::ReturnTrainOriginal | Self::ReturnTrainLater => {
                GroupTravelDestination::JohtoGoldenrod
            }
            Self::ReturnGateOriginal | Self::ReturnGateLater => {
                GroupTravelDestination::JohtoReceptionGate
            }
            Self::FerryOlivineSouthernIsland
            | Self::FerryVermilionSouthernIsland
            | Self::FerryLilycoveSouthernIsland => GroupTravelDestination::SouthernIsland,
            Self::FerryOlivineBirthIsland
            | Self::FerryVermilionBirthIsland
            | Self::FerryLilycoveBirthIsland => GroupTravelDestination::BirthIsland,
            Self::FerryOlivineFarawayIsland
            | Self::FerryVermilionFarawayIsland
            | Self::FerryLilycoveFarawayIsland => GroupTravelDestination::FarawayIsland,
            Self::FerryOlivineBattleFrontier
            | Self::FerryVermilionBattleFrontier
            | Self::FerryLilycoveBattleFrontier
            | Self::FerrySlateportBattleFrontier => GroupTravelDestination::BattleFrontier,
            Self::FerryLilycoveNavelRock => GroupTravelDestination::NavelRock,
            Self::FerrySouthernIslandLilycove
            | Self::FerryBirthIslandLilycove
            | Self::FerryFarawayIslandLilycove
            | Self::FerryBattleFrontierLilycove
            | Self::FerryNavelRockLilycove => GroupTravelDestination::LilycoveHarbor,
            Self::FerryBattleFrontierSlateport => GroupTravelDestination::SlateportHarbor,
            Self::FerrySSTidalSlateportBoard | Self::FerrySSTidalLilycoveBoard => {
                GroupTravelDestination::SSTidalCorridor
            }
            Self::FerrySSTidalLilycoveExit => GroupTravelDestination::LilycoveHarbor,
            Self::FerrySSTidalSlateportExit => GroupTravelDestination::SlateportHarbor,
            Self::FerryBrineyHouseDewford | Self::FerryRoute109Dewford => {
                GroupTravelDestination::HoennDewford
            }
            Self::FerryDewfordBrineyHouse => GroupTravelDestination::HoennBrineyHouse,
            Self::FerryDewfordRoute109 => GroupTravelDestination::HoennRoute109,
            Self::SeagallopOneVermilion
            | Self::SeagallopTwoVermilion
            | Self::SeagallopThreeVermilion
            | Self::SeagallopFourVermilion
            | Self::SeagallopFiveVermilion
            | Self::SeagallopSixVermilion
            | Self::SeagallopSevenVermilion
            | Self::SeagallopNavelVermilion
            | Self::SeagallopBirthVermilion => GroupTravelDestination::SeagallopVermilion,
            Self::SeagallopVermilionOne
            | Self::SeagallopTwoOne
            | Self::SeagallopThreeOne
            | Self::SeagallopFourOne
            | Self::SeagallopFiveOne
            | Self::SeagallopSixOne
            | Self::SeagallopSevenOne => GroupTravelDestination::SeagallopOne,
            Self::SeagallopBillCinnabarOne => GroupTravelDestination::BillOneIslandCenter,
            Self::SeagallopBillOneCinnabar => GroupTravelDestination::BillCinnabar,
            Self::CableCarRoute112MtChimney => GroupTravelDestination::MtChimneyCableCarStation,
            Self::CableCarMtChimneyRoute112 => GroupTravelDestination::Route112CableCarStation,
            Self::SeagallopVermilionTwo
            | Self::SeagallopOneTwo
            | Self::SeagallopThreeTwo
            | Self::SeagallopFourTwo
            | Self::SeagallopFiveTwo
            | Self::SeagallopSixTwo
            | Self::SeagallopSevenTwo => GroupTravelDestination::SeagallopTwo,
            Self::SeagallopVermilionThree
            | Self::SeagallopOneThree
            | Self::SeagallopTwoThree
            | Self::SeagallopFourThree
            | Self::SeagallopFiveThree
            | Self::SeagallopSixThree
            | Self::SeagallopSevenThree => GroupTravelDestination::SeagallopThree,
            Self::SeagallopVermilionFour
            | Self::SeagallopOneFour
            | Self::SeagallopTwoFour
            | Self::SeagallopThreeFour
            | Self::SeagallopFiveFour
            | Self::SeagallopSixFour
            | Self::SeagallopSevenFour => GroupTravelDestination::SeagallopFour,
            Self::SeagallopVermilionFive
            | Self::SeagallopOneFive
            | Self::SeagallopTwoFive
            | Self::SeagallopThreeFive
            | Self::SeagallopFourFive
            | Self::SeagallopSixFive
            | Self::SeagallopSevenFive => GroupTravelDestination::SeagallopFive,
            Self::SeagallopVermilionSix
            | Self::SeagallopOneSix
            | Self::SeagallopTwoSix
            | Self::SeagallopThreeSix
            | Self::SeagallopFourSix
            | Self::SeagallopFiveSix
            | Self::SeagallopSevenSix => GroupTravelDestination::SeagallopSix,
            Self::SeagallopVermilionSeven
            | Self::SeagallopOneSeven
            | Self::SeagallopTwoSeven
            | Self::SeagallopThreeSeven
            | Self::SeagallopFourSeven
            | Self::SeagallopFiveSeven
            | Self::SeagallopSixSeven => GroupTravelDestination::SeagallopSeven,
            Self::SeagallopVermilionNavel => GroupTravelDestination::SeagallopNavel,
            Self::SeagallopVermilionBirth => GroupTravelDestination::SeagallopBirth,
            Self::FlyLittleroot => GroupTravelDestination::HoennLittleroot,
            Self::FlyJohtoNewbark => GroupTravelDestination::JohtoNewbark,
            Self::FlyJohtoCherrygrove => GroupTravelDestination::JohtoCherrygrove,
            Self::FlyJohtoViolet => GroupTravelDestination::JohtoViolet,
            Self::FlyJohtoAzalea => GroupTravelDestination::JohtoAzalea,
            Self::FlyJohtoGoldenrod => GroupTravelDestination::JohtoGoldenrod,
            Self::FlyJohtoEcruteak => GroupTravelDestination::JohtoEcruteak,
            Self::FlyJohtoOlivine => GroupTravelDestination::JohtoOlivine,
            Self::FlyJohtoCianwood => GroupTravelDestination::JohtoCianwood,
            Self::FlyJohtoMahogany => GroupTravelDestination::JohtoMahogany,
            Self::FlyJohtoBlackthorn => GroupTravelDestination::JohtoBlackthorn,
            Self::FlyHoennOldale => GroupTravelDestination::HoennOldale,
            Self::FlyHoennDewford => GroupTravelDestination::HoennDewford,
            Self::FlyHoennLavaridge => GroupTravelDestination::HoennLavaridge,
            Self::FlyHoennFallarbor => GroupTravelDestination::HoennFallarbor,
            Self::FlyHoennVerdanturf => GroupTravelDestination::HoennVerdanturf,
            Self::FlyHoennPacifidlog => GroupTravelDestination::HoennPacifidlog,
            Self::FlyHoennPetalburg => GroupTravelDestination::HoennPetalburg,
            Self::FlyHoennSlateport => GroupTravelDestination::HoennSlateport,
            Self::FlyHoennMauville => GroupTravelDestination::HoennMauville,
            Self::FlyHoennRustboro => GroupTravelDestination::HoennRustboro,
            Self::FlyHoennFortree => GroupTravelDestination::HoennFortree,
            Self::FlyHoennLilycove => GroupTravelDestination::HoennLilycove,
            Self::FlyHoennMossdeep => GroupTravelDestination::HoennMossdeep,
            Self::FlyHoennSootopolis => GroupTravelDestination::HoennSootopolis,
            Self::FlyKantoOriginalPallet => GroupTravelDestination::KantoOriginalPallet,
            Self::FlyKantoOriginalViridian => GroupTravelDestination::KantoOriginalViridian,
            Self::FlyKantoOriginalPewter => GroupTravelDestination::KantoOriginalPewter,
            Self::FlyKantoOriginalCerulean => GroupTravelDestination::KantoOriginalCerulean,
            Self::FlyKantoOriginalLavender => GroupTravelDestination::KantoOriginalLavender,
            Self::FlyKantoOriginalVermilion => GroupTravelDestination::KantoOriginalVermilion,
            Self::FlyKantoOriginalCeladon => GroupTravelDestination::KantoOriginalCeladon,
            Self::FlyKantoOriginalFuchsia => GroupTravelDestination::KantoOriginalFuchsia,
            Self::FlyKantoOriginalCinnabar => GroupTravelDestination::KantoOriginalCinnabar,
            Self::FlyKantoOriginalIndigo => GroupTravelDestination::KantoOriginalIndigo,
            Self::FlyKantoOriginalSaffron => GroupTravelDestination::KantoOriginalSaffron,
            Self::FlyKantoLaterPallet => GroupTravelDestination::KantoLaterPallet,
            Self::FlyKantoLaterViridian => GroupTravelDestination::KantoLaterViridian,
            Self::FlyKantoLaterPewter => GroupTravelDestination::KantoLaterPewter,
            Self::FlyKantoLaterCerulean => GroupTravelDestination::KantoLaterCerulean,
            Self::FlyKantoLaterLavender => GroupTravelDestination::KantoLaterLavender,
            Self::FlyKantoLaterVermilion => GroupTravelDestination::KantoLaterVermilion,
            Self::FlyKantoLaterCeladon => GroupTravelDestination::KantoLaterCeladon,
            Self::FlyKantoLaterFuchsia => GroupTravelDestination::KantoLaterFuchsia,
            Self::FlyKantoLaterSaffron => GroupTravelDestination::KantoLaterSaffron,
            Self::FlyKantoLaterCinnabar => GroupTravelDestination::KantoLaterCinnabar,
            Self::FlySeviiOneIsland => GroupTravelDestination::SeviiOneIsland,
            Self::FlySeviiTwoIsland => GroupTravelDestination::SeviiTwoIsland,
            Self::FlySeviiThreeIsland => GroupTravelDestination::SeviiThreeIsland,
            Self::FlySeviiFourIsland => GroupTravelDestination::SeviiFourIsland,
            Self::FlySeviiFiveIsland => GroupTravelDestination::SeviiFiveIsland,
            Self::FlySeviiSevenIsland => GroupTravelDestination::SeviiSevenIsland,
            Self::FlySeviiSixIsland => GroupTravelDestination::SeviiSixIsland,
            Self::FlyKantoRoute4PokemonCenter => GroupTravelDestination::KantoRoute4PokemonCenter,
            Self::FlyKantoRoute10PokemonCenter => GroupTravelDestination::KantoRoute10PokemonCenter,
            Self::FlyHoennEverGrandeCenter => GroupTravelDestination::HoennEverGrandeCenter,
            Self::FlyHoennEverGrandeLeague => GroupTravelDestination::HoennEverGrandeLeague,
            Self::FlyHoennBattleFrontier => GroupTravelDestination::HoennBattleFrontier,
            Self::Dig | Self::EscapeRope => GroupTravelDestination::Dynamic,
        }
    }

    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            1 => Ok(Self::TrainOriginal),
            2 => Ok(Self::TrainLater),
            3 => Ok(Self::FerryOriginal),
            4 => Ok(Self::FerryLater),
            5 => Ok(Self::GateOriginal),
            6 => Ok(Self::GateLater),
            7 => Ok(Self::FlyLittleroot),
            8 => Ok(Self::FlyJohtoNewbark),
            9 => Ok(Self::FlyJohtoCherrygrove),
            10 => Ok(Self::FlyJohtoViolet),
            11 => Ok(Self::FlyJohtoAzalea),
            12 => Ok(Self::FlyJohtoGoldenrod),
            13 => Ok(Self::FlyJohtoEcruteak),
            14 => Ok(Self::FlyJohtoOlivine),
            15 => Ok(Self::FlyJohtoCianwood),
            16 => Ok(Self::FlyJohtoMahogany),
            17 => Ok(Self::FlyJohtoBlackthorn),
            18 => Ok(Self::FlyHoennOldale),
            19 => Ok(Self::FlyHoennDewford),
            20 => Ok(Self::FlyHoennLavaridge),
            21 => Ok(Self::FlyHoennFallarbor),
            22 => Ok(Self::FlyHoennVerdanturf),
            23 => Ok(Self::FlyHoennPacifidlog),
            24 => Ok(Self::FlyHoennPetalburg),
            25 => Ok(Self::FlyHoennSlateport),
            26 => Ok(Self::FlyHoennMauville),
            27 => Ok(Self::FlyHoennRustboro),
            28 => Ok(Self::FlyHoennFortree),
            29 => Ok(Self::FlyHoennLilycove),
            30 => Ok(Self::FlyHoennMossdeep),
            31 => Ok(Self::FlyHoennSootopolis),
            33 => Ok(Self::FlyKantoOriginalPallet),
            34 => Ok(Self::FlyKantoOriginalViridian),
            35 => Ok(Self::FlyKantoOriginalPewter),
            36 => Ok(Self::FlyKantoOriginalCerulean),
            37 => Ok(Self::FlyKantoOriginalLavender),
            38 => Ok(Self::FlyKantoOriginalVermilion),
            39 => Ok(Self::FlyKantoOriginalCeladon),
            40 => Ok(Self::FlyKantoOriginalFuchsia),
            41 => Ok(Self::FlyKantoOriginalCinnabar),
            42 => Ok(Self::FlyKantoOriginalIndigo),
            43 => Ok(Self::FlyKantoOriginalSaffron),
            44 => Ok(Self::FlyKantoLaterPallet),
            45 => Ok(Self::FlyKantoLaterViridian),
            46 => Ok(Self::FlyKantoLaterPewter),
            47 => Ok(Self::FlyKantoLaterCerulean),
            48 => Ok(Self::FlyKantoLaterLavender),
            49 => Ok(Self::FlyKantoLaterVermilion),
            50 => Ok(Self::FlyKantoLaterCeladon),
            51 => Ok(Self::FlyKantoLaterFuchsia),
            52 => Ok(Self::FlyKantoLaterSaffron),
            53 => Ok(Self::FlyKantoLaterCinnabar),
            54 => Ok(Self::FlySeviiOneIsland),
            55 => Ok(Self::FlySeviiTwoIsland),
            56 => Ok(Self::FlySeviiThreeIsland),
            57 => Ok(Self::FlySeviiFourIsland),
            58 => Ok(Self::FlySeviiFiveIsland),
            59 => Ok(Self::FlySeviiSevenIsland),
            60 => Ok(Self::FlySeviiSixIsland),
            61 => Ok(Self::FlyKantoRoute4PokemonCenter),
            62 => Ok(Self::FlyKantoRoute10PokemonCenter),
            63 => Ok(Self::FlyHoennEverGrandeCenter),
            64 => Ok(Self::FlyHoennEverGrandeLeague),
            65 => Ok(Self::FlyHoennBattleFrontier),
            66 => Ok(Self::ReturnFerryOriginal),
            67 => Ok(Self::ReturnFerryLater),
            68 => Ok(Self::ReturnTrainOriginal),
            69 => Ok(Self::ReturnTrainLater),
            70 => Ok(Self::ReturnGateOriginal),
            71 => Ok(Self::ReturnGateLater),
            72 => Ok(Self::FerryOlivineSouthernIsland),
            73 => Ok(Self::FerryOlivineBirthIsland),
            74 => Ok(Self::FerryOlivineFarawayIsland),
            75 => Ok(Self::FerryOlivineBattleFrontier),
            76 => Ok(Self::FerryVermilionSouthernIsland),
            77 => Ok(Self::FerryVermilionBirthIsland),
            78 => Ok(Self::FerryVermilionFarawayIsland),
            79 => Ok(Self::FerryVermilionBattleFrontier),
            80 => Ok(Self::FerrySouthernIslandLilycove),
            81 => Ok(Self::FerryBirthIslandLilycove),
            82 => Ok(Self::FerryFarawayIslandLilycove),
            83 => Ok(Self::FerryBattleFrontierSlateport),
            84 => Ok(Self::FerryBattleFrontierLilycove),
            85 => Ok(Self::FerryLilycoveSouthernIsland),
            86 => Ok(Self::FerryLilycoveNavelRock),
            87 => Ok(Self::FerryLilycoveBirthIsland),
            88 => Ok(Self::FerryLilycoveFarawayIsland),
            89 => Ok(Self::FerryLilycoveBattleFrontier),
            90 => Ok(Self::FerrySlateportBattleFrontier),
            91 => Ok(Self::FerryNavelRockLilycove),
            92 => Ok(Self::FerrySSTidalSlateportBoard),
            93 => Ok(Self::FerrySSTidalLilycoveBoard),
            94 => Ok(Self::FerrySSTidalLilycoveExit),
            95 => Ok(Self::FerrySSTidalSlateportExit),
            96 => Ok(Self::FerryBrineyHouseDewford),
            97 => Ok(Self::FerryDewfordBrineyHouse),
            98 => Ok(Self::FerryDewfordRoute109),
            99 => Ok(Self::FerryRoute109Dewford),
            100 => Ok(Self::SeagallopVermilionOne),
            101 => Ok(Self::SeagallopVermilionTwo),
            102 => Ok(Self::SeagallopVermilionThree),
            103 => Ok(Self::SeagallopVermilionFour),
            104 => Ok(Self::SeagallopVermilionFive),
            105 => Ok(Self::SeagallopVermilionSix),
            106 => Ok(Self::SeagallopVermilionSeven),
            107 => Ok(Self::SeagallopOneVermilion),
            108 => Ok(Self::SeagallopOneTwo),
            109 => Ok(Self::SeagallopOneThree),
            110 => Ok(Self::SeagallopOneFour),
            111 => Ok(Self::SeagallopOneFive),
            112 => Ok(Self::SeagallopOneSix),
            113 => Ok(Self::SeagallopOneSeven),
            114 => Ok(Self::SeagallopTwoVermilion),
            115 => Ok(Self::SeagallopTwoOne),
            116 => Ok(Self::SeagallopTwoThree),
            117 => Ok(Self::SeagallopTwoFour),
            118 => Ok(Self::SeagallopTwoFive),
            119 => Ok(Self::SeagallopTwoSix),
            120 => Ok(Self::SeagallopTwoSeven),
            121 => Ok(Self::SeagallopThreeVermilion),
            122 => Ok(Self::SeagallopThreeOne),
            123 => Ok(Self::SeagallopThreeTwo),
            124 => Ok(Self::SeagallopThreeFour),
            125 => Ok(Self::SeagallopThreeFive),
            126 => Ok(Self::SeagallopThreeSix),
            127 => Ok(Self::SeagallopThreeSeven),
            128 => Ok(Self::SeagallopFourVermilion),
            129 => Ok(Self::SeagallopFourOne),
            130 => Ok(Self::SeagallopFourTwo),
            131 => Ok(Self::SeagallopFourThree),
            132 => Ok(Self::SeagallopFourFive),
            133 => Ok(Self::SeagallopFourSix),
            134 => Ok(Self::SeagallopFourSeven),
            135 => Ok(Self::SeagallopFiveVermilion),
            136 => Ok(Self::SeagallopFiveOne),
            137 => Ok(Self::SeagallopFiveTwo),
            138 => Ok(Self::SeagallopFiveThree),
            139 => Ok(Self::SeagallopFiveFour),
            140 => Ok(Self::SeagallopFiveSix),
            141 => Ok(Self::SeagallopFiveSeven),
            142 => Ok(Self::SeagallopSixVermilion),
            143 => Ok(Self::SeagallopSixOne),
            144 => Ok(Self::SeagallopSixTwo),
            145 => Ok(Self::SeagallopSixThree),
            146 => Ok(Self::SeagallopSixFour),
            147 => Ok(Self::SeagallopSixFive),
            148 => Ok(Self::SeagallopSixSeven),
            149 => Ok(Self::SeagallopSevenVermilion),
            150 => Ok(Self::SeagallopSevenOne),
            151 => Ok(Self::SeagallopSevenTwo),
            152 => Ok(Self::SeagallopSevenThree),
            153 => Ok(Self::SeagallopSevenFour),
            154 => Ok(Self::SeagallopSevenFive),
            155 => Ok(Self::SeagallopSevenSix),
            156 => Ok(Self::SeagallopVermilionNavel),
            157 => Ok(Self::SeagallopNavelVermilion),
            158 => Ok(Self::SeagallopVermilionBirth),
            159 => Ok(Self::SeagallopBirthVermilion),
            160 => Ok(Self::SeagallopBillCinnabarOne),
            161 => Ok(Self::SeagallopBillOneCinnabar),
            162 => Ok(Self::CableCarRoute112MtChimney),
            163 => Ok(Self::CableCarMtChimneyRoute112),
            164 => Ok(Self::Dig),
            165 => Ok(Self::EscapeRope),
            value => Err(GroupTravelCodecError::InvalidRoute(value)),
        }
    }
}

/// The script-owned departure path that authorized a group-travel request.
///
/// Ferry routes intentionally have two distinct contexts: the normal Olivine
/// ferry and the S.S. Aqua maiden voyage. This value occupies byte six of the
/// fixed-size record so a responder never infers the journey from local flags.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelDeparture {
    Train = 1,
    Ferry = 2,
    SsaquaMaiden = 3,
    Gate = 4,
    Fly = 5,
    CableCar = 6,
    Teleport = 7,
    Dig = 8,
    EscapeRope = 9,
}

impl GroupTravelDeparture {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            1 => Ok(Self::Train),
            2 => Ok(Self::Ferry),
            3 => Ok(Self::SsaquaMaiden),
            4 => Ok(Self::Gate),
            5 => Ok(Self::Fly),
            6 => Ok(Self::CableCar),
            7 => Ok(Self::Teleport),
            8 => Ok(Self::Dig),
            9 => Ok(Self::EscapeRope),
            value => Err(GroupTravelCodecError::InvalidDeparture(value)),
        }
    }

    #[must_use]
    pub const fn matches_route(self, route: GroupTravelRoute) -> bool {
        match self {
            Self::Train => matches!(
                route,
                GroupTravelRoute::TrainOriginal
                    | GroupTravelRoute::TrainLater
                    | GroupTravelRoute::ReturnTrainOriginal
                    | GroupTravelRoute::ReturnTrainLater
            ),
            Self::Ferry => matches!(
                route,
                GroupTravelRoute::FerryOriginal
                    | GroupTravelRoute::FerryLater
                    | GroupTravelRoute::ReturnFerryOriginal
                    | GroupTravelRoute::ReturnFerryLater
                    | GroupTravelRoute::FerryOlivineSouthernIsland
                    | GroupTravelRoute::FerryOlivineBirthIsland
                    | GroupTravelRoute::FerryOlivineFarawayIsland
                    | GroupTravelRoute::FerryOlivineBattleFrontier
                    | GroupTravelRoute::FerryVermilionSouthernIsland
                    | GroupTravelRoute::FerryVermilionBirthIsland
                    | GroupTravelRoute::FerryVermilionFarawayIsland
                    | GroupTravelRoute::FerryVermilionBattleFrontier
                    | GroupTravelRoute::FerrySouthernIslandLilycove
                    | GroupTravelRoute::FerryBirthIslandLilycove
                    | GroupTravelRoute::FerryFarawayIslandLilycove
                    | GroupTravelRoute::FerryBattleFrontierSlateport
                    | GroupTravelRoute::FerryBattleFrontierLilycove
                    | GroupTravelRoute::FerryLilycoveSouthernIsland
                    | GroupTravelRoute::FerryLilycoveNavelRock
                    | GroupTravelRoute::FerryLilycoveBirthIsland
                    | GroupTravelRoute::FerryLilycoveFarawayIsland
                    | GroupTravelRoute::FerryLilycoveBattleFrontier
                    | GroupTravelRoute::FerrySlateportBattleFrontier
                    | GroupTravelRoute::FerryNavelRockLilycove
                    | GroupTravelRoute::FerrySSTidalSlateportBoard
                    | GroupTravelRoute::FerrySSTidalLilycoveBoard
                    | GroupTravelRoute::FerrySSTidalLilycoveExit
                    | GroupTravelRoute::FerrySSTidalSlateportExit
                    | GroupTravelRoute::FerryBrineyHouseDewford
                    | GroupTravelRoute::FerryDewfordBrineyHouse
                    | GroupTravelRoute::FerryDewfordRoute109
                    | GroupTravelRoute::FerryRoute109Dewford
                    | GroupTravelRoute::SeagallopVermilionOne
                    | GroupTravelRoute::SeagallopVermilionTwo
                    | GroupTravelRoute::SeagallopVermilionThree
                    | GroupTravelRoute::SeagallopVermilionFour
                    | GroupTravelRoute::SeagallopVermilionFive
                    | GroupTravelRoute::SeagallopVermilionSix
                    | GroupTravelRoute::SeagallopVermilionSeven
                    | GroupTravelRoute::SeagallopOneVermilion
                    | GroupTravelRoute::SeagallopOneTwo
                    | GroupTravelRoute::SeagallopOneThree
                    | GroupTravelRoute::SeagallopOneFour
                    | GroupTravelRoute::SeagallopOneFive
                    | GroupTravelRoute::SeagallopOneSix
                    | GroupTravelRoute::SeagallopOneSeven
                    | GroupTravelRoute::SeagallopTwoVermilion
                    | GroupTravelRoute::SeagallopTwoOne
                    | GroupTravelRoute::SeagallopTwoThree
                    | GroupTravelRoute::SeagallopTwoFour
                    | GroupTravelRoute::SeagallopTwoFive
                    | GroupTravelRoute::SeagallopTwoSix
                    | GroupTravelRoute::SeagallopTwoSeven
                    | GroupTravelRoute::SeagallopThreeVermilion
                    | GroupTravelRoute::SeagallopThreeOne
                    | GroupTravelRoute::SeagallopThreeTwo
                    | GroupTravelRoute::SeagallopThreeFour
                    | GroupTravelRoute::SeagallopThreeFive
                    | GroupTravelRoute::SeagallopThreeSix
                    | GroupTravelRoute::SeagallopThreeSeven
                    | GroupTravelRoute::SeagallopFourVermilion
                    | GroupTravelRoute::SeagallopFourOne
                    | GroupTravelRoute::SeagallopFourTwo
                    | GroupTravelRoute::SeagallopFourThree
                    | GroupTravelRoute::SeagallopFourFive
                    | GroupTravelRoute::SeagallopFourSix
                    | GroupTravelRoute::SeagallopFourSeven
                    | GroupTravelRoute::SeagallopFiveVermilion
                    | GroupTravelRoute::SeagallopFiveOne
                    | GroupTravelRoute::SeagallopFiveTwo
                    | GroupTravelRoute::SeagallopFiveThree
                    | GroupTravelRoute::SeagallopFiveFour
                    | GroupTravelRoute::SeagallopFiveSix
                    | GroupTravelRoute::SeagallopFiveSeven
                    | GroupTravelRoute::SeagallopSixVermilion
                    | GroupTravelRoute::SeagallopSixOne
                    | GroupTravelRoute::SeagallopSixTwo
                    | GroupTravelRoute::SeagallopSixThree
                    | GroupTravelRoute::SeagallopSixFour
                    | GroupTravelRoute::SeagallopSixFive
                    | GroupTravelRoute::SeagallopSixSeven
                    | GroupTravelRoute::SeagallopSevenVermilion
                    | GroupTravelRoute::SeagallopSevenOne
                    | GroupTravelRoute::SeagallopSevenTwo
                    | GroupTravelRoute::SeagallopSevenThree
                    | GroupTravelRoute::SeagallopSevenFour
                    | GroupTravelRoute::SeagallopSevenFive
                    | GroupTravelRoute::SeagallopSevenSix
                    | GroupTravelRoute::SeagallopVermilionNavel
                    | GroupTravelRoute::SeagallopNavelVermilion
                    | GroupTravelRoute::SeagallopVermilionBirth
                    | GroupTravelRoute::SeagallopBirthVermilion
                    | GroupTravelRoute::SeagallopBillCinnabarOne
                    | GroupTravelRoute::SeagallopBillOneCinnabar
            ),
            Self::SsaquaMaiden => matches!(
                route,
                GroupTravelRoute::FerryOriginal | GroupTravelRoute::FerryLater
            ),
            Self::Gate => matches!(
                route,
                GroupTravelRoute::GateOriginal
                    | GroupTravelRoute::GateLater
                    | GroupTravelRoute::ReturnGateOriginal
                    | GroupTravelRoute::ReturnGateLater
            ),
            Self::Fly => route.is_fly(),
            Self::Teleport => {
                route.is_fly() && (route as u8) > (GroupTravelRoute::FlyLittleroot as u8)
            }
            Self::CableCar => matches!(
                route,
                GroupTravelRoute::CableCarRoute112MtChimney
                    | GroupTravelRoute::CableCarMtChimneyRoute112
            ),
            Self::Dig => matches!(route, GroupTravelRoute::Dig),
            Self::EscapeRope => matches!(route, GroupTravelRoute::EscapeRope),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelEra {
    Original = 1,
    Later = 2,
    Hoenn = 3,
    Johto = 4,
    Sevii = 5,
}

impl GroupTravelEra {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            1 => Ok(Self::Original),
            2 => Ok(Self::Later),
            3 => Ok(Self::Hoenn),
            4 => Ok(Self::Johto),
            5 => Ok(Self::Sevii),
            value => Err(GroupTravelCodecError::InvalidEra(value)),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelDestination {
    OriginalVermilion = 1,
    LaterVermilion = 2,
    OriginalSaffron = 3,
    LaterSaffron = 4,
    OriginalRoute22 = 5,
    LaterRoute22 = 6,
    HoennLittleroot = 7,
    JohtoNewbark = 8,
    JohtoCherrygrove = 9,
    JohtoViolet = 10,
    JohtoAzalea = 11,
    JohtoGoldenrod = 12,
    JohtoEcruteak = 13,
    JohtoOlivine = 14,
    JohtoCianwood = 15,
    JohtoMahogany = 16,
    JohtoBlackthorn = 17,
    HoennOldale = 18,
    HoennDewford = 19,
    HoennLavaridge = 20,
    HoennFallarbor = 21,
    HoennVerdanturf = 22,
    HoennPacifidlog = 23,
    HoennPetalburg = 24,
    HoennSlateport = 25,
    HoennMauville = 26,
    HoennRustboro = 27,
    HoennFortree = 28,
    HoennLilycove = 29,
    HoennMossdeep = 30,
    HoennSootopolis = 31,
    KantoOriginalPallet = 33,
    KantoOriginalViridian = 34,
    KantoOriginalPewter = 35,
    KantoOriginalCerulean = 36,
    KantoOriginalLavender = 37,
    KantoOriginalVermilion = 38,
    KantoOriginalCeladon = 39,
    KantoOriginalFuchsia = 40,
    KantoOriginalCinnabar = 41,
    KantoOriginalIndigo = 42,
    KantoOriginalSaffron = 43,
    KantoLaterPallet = 44,
    KantoLaterViridian = 45,
    KantoLaterPewter = 46,
    KantoLaterCerulean = 47,
    KantoLaterLavender = 48,
    KantoLaterVermilion = 49,
    KantoLaterCeladon = 50,
    KantoLaterFuchsia = 51,
    KantoLaterSaffron = 52,
    KantoLaterCinnabar = 53,
    SeviiOneIsland = 54,
    SeviiTwoIsland = 55,
    SeviiThreeIsland = 56,
    SeviiFourIsland = 57,
    SeviiFiveIsland = 58,
    SeviiSevenIsland = 59,
    SeviiSixIsland = 60,
    KantoRoute4PokemonCenter = 61,
    KantoRoute10PokemonCenter = 62,
    HoennEverGrandeCenter = 63,
    HoennEverGrandeLeague = 64,
    HoennBattleFrontier = 65,
    JohtoReceptionGate = 66,
    SouthernIsland = 72,
    BirthIsland = 73,
    FarawayIsland = 74,
    BattleFrontier = 75,
    NavelRock = 76,
    LilycoveHarbor = 80,
    SlateportHarbor = 81,
    HoennBrineyHouse = 82,
    HoennRoute109 = 83,
    SeagallopVermilion = 84,
    SeagallopOne = 85,
    SeagallopTwo = 86,
    SeagallopThree = 87,
    SeagallopFour = 88,
    SeagallopFive = 89,
    SeagallopSix = 90,
    SeagallopSeven = 91,
    SeagallopNavel = 92,
    SeagallopBirth = 93,
    SSTidalCorridor = 94,
    BillOneIslandCenter = 95,
    BillCinnabar = 96,
    MtChimneyCableCarStation = 97,
    Route112CableCarStation = 98,
    /// Dynamic routes carry their map identity in the endpoint extension.
    Dynamic = 99,
}

impl GroupTravelDestination {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            1 => Ok(Self::OriginalVermilion),
            2 => Ok(Self::LaterVermilion),
            3 => Ok(Self::OriginalSaffron),
            4 => Ok(Self::LaterSaffron),
            5 => Ok(Self::OriginalRoute22),
            6 => Ok(Self::LaterRoute22),
            7 => Ok(Self::HoennLittleroot),
            8 => Ok(Self::JohtoNewbark),
            9 => Ok(Self::JohtoCherrygrove),
            10 => Ok(Self::JohtoViolet),
            11 => Ok(Self::JohtoAzalea),
            12 => Ok(Self::JohtoGoldenrod),
            13 => Ok(Self::JohtoEcruteak),
            14 => Ok(Self::JohtoOlivine),
            15 => Ok(Self::JohtoCianwood),
            16 => Ok(Self::JohtoMahogany),
            17 => Ok(Self::JohtoBlackthorn),
            18 => Ok(Self::HoennOldale),
            19 => Ok(Self::HoennDewford),
            20 => Ok(Self::HoennLavaridge),
            21 => Ok(Self::HoennFallarbor),
            22 => Ok(Self::HoennVerdanturf),
            23 => Ok(Self::HoennPacifidlog),
            24 => Ok(Self::HoennPetalburg),
            25 => Ok(Self::HoennSlateport),
            26 => Ok(Self::HoennMauville),
            27 => Ok(Self::HoennRustboro),
            28 => Ok(Self::HoennFortree),
            29 => Ok(Self::HoennLilycove),
            30 => Ok(Self::HoennMossdeep),
            31 => Ok(Self::HoennSootopolis),
            33 => Ok(Self::KantoOriginalPallet),
            34 => Ok(Self::KantoOriginalViridian),
            35 => Ok(Self::KantoOriginalPewter),
            36 => Ok(Self::KantoOriginalCerulean),
            37 => Ok(Self::KantoOriginalLavender),
            38 => Ok(Self::KantoOriginalVermilion),
            39 => Ok(Self::KantoOriginalCeladon),
            40 => Ok(Self::KantoOriginalFuchsia),
            41 => Ok(Self::KantoOriginalCinnabar),
            42 => Ok(Self::KantoOriginalIndigo),
            43 => Ok(Self::KantoOriginalSaffron),
            44 => Ok(Self::KantoLaterPallet),
            45 => Ok(Self::KantoLaterViridian),
            46 => Ok(Self::KantoLaterPewter),
            47 => Ok(Self::KantoLaterCerulean),
            48 => Ok(Self::KantoLaterLavender),
            49 => Ok(Self::KantoLaterVermilion),
            50 => Ok(Self::KantoLaterCeladon),
            51 => Ok(Self::KantoLaterFuchsia),
            52 => Ok(Self::KantoLaterSaffron),
            53 => Ok(Self::KantoLaterCinnabar),
            54 => Ok(Self::SeviiOneIsland),
            55 => Ok(Self::SeviiTwoIsland),
            56 => Ok(Self::SeviiThreeIsland),
            57 => Ok(Self::SeviiFourIsland),
            58 => Ok(Self::SeviiFiveIsland),
            59 => Ok(Self::SeviiSevenIsland),
            60 => Ok(Self::SeviiSixIsland),
            61 => Ok(Self::KantoRoute4PokemonCenter),
            62 => Ok(Self::KantoRoute10PokemonCenter),
            63 => Ok(Self::HoennEverGrandeCenter),
            64 => Ok(Self::HoennEverGrandeLeague),
            65 => Ok(Self::HoennBattleFrontier),
            66 => Ok(Self::JohtoReceptionGate),
            72 => Ok(Self::SouthernIsland),
            73 => Ok(Self::BirthIsland),
            74 => Ok(Self::FarawayIsland),
            75 => Ok(Self::BattleFrontier),
            76 => Ok(Self::NavelRock),
            80 => Ok(Self::LilycoveHarbor),
            81 => Ok(Self::SlateportHarbor),
            82 => Ok(Self::HoennBrineyHouse),
            83 => Ok(Self::HoennRoute109),
            84 => Ok(Self::SeagallopVermilion),
            85 => Ok(Self::SeagallopOne),
            86 => Ok(Self::SeagallopTwo),
            87 => Ok(Self::SeagallopThree),
            88 => Ok(Self::SeagallopFour),
            89 => Ok(Self::SeagallopFive),
            90 => Ok(Self::SeagallopSix),
            91 => Ok(Self::SeagallopSeven),
            92 => Ok(Self::SeagallopNavel),
            93 => Ok(Self::SeagallopBirth),
            94 => Ok(Self::SSTidalCorridor),
            95 => Ok(Self::BillOneIslandCenter),
            96 => Ok(Self::BillCinnabar),
            97 => Ok(Self::MtChimneyCableCarStation),
            98 => Ok(Self::Route112CableCarStation),
            99 => Ok(Self::Dynamic),
            value => Err(GroupTravelCodecError::InvalidDestination(value)),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelResult {
    None = 0,
    Accepted = 1,
    Declined = 2,
    Applied = 3,
}

impl GroupTravelResult {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Accepted),
            2 => Ok(Self::Declined),
            3 => Ok(Self::Applied),
            value => Err(GroupTravelCodecError::InvalidResult(value)),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelReason {
    None = 0,
    ParticipantDeclined = 1,
    RequesterCanceled = 2,
    Conflict = 3,
    Unsafe = 4,
}

impl GroupTravelReason {
    fn from_wire(value: u8) -> Result<Self, GroupTravelCodecError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::ParticipantDeclined),
            2 => Ok(Self::RequesterCanceled),
            3 => Ok(Self::Conflict),
            4 => Ok(Self::Unsafe),
            value => Err(GroupTravelCodecError::InvalidReason(value)),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelClientKind {
    Request = 1,
    Decision = 2,
    Cancel = 3,
    Applied = 4,
    SceneMarkerRequest = 5,
    SceneComplete = 6,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupTravelServerKind {
    Requesting = 1,
    Offer = 2,
    Commit = 3,
    Abort = 4,
    Complete = 5,
    SceneMarkerAccepted = 6,
    SceneReady = 7,
}

/// The exact dynamic travel endpoint carried by Dig and Escape Rope records.
///
/// The ROM bridge deliberately uses one-byte map coordinates and signed
/// one-byte player coordinates.  Source coordinates identify the map the two
/// players are currently on; target coordinates are immutable proposal data.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelEndpoint {
    pub source_map_group: u8,
    pub source_map_number: u8,
    pub target_map_group: u8,
    pub target_map_number: u8,
    pub target_x: i8,
    pub target_y: i8,
}

impl GroupTravelEndpoint {
    #[must_use]
    pub const fn new(
        source_map_group: u8,
        source_map_number: u8,
        target_map_group: u8,
        target_map_number: u8,
        target_x: i8,
        target_y: i8,
    ) -> Self {
        Self {
            source_map_group,
            source_map_number,
            target_map_group,
            target_map_number,
            target_x,
            target_y,
        }
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.source_map_group == 0
            && self.source_map_number == 0
            && self.target_map_group == 0
            && self.target_map_number == 0
            && self.target_x == 0
            && self.target_y == 0
    }
}

/// Compatibility alias for callers that name the extension explicitly.
pub type DynamicTravelEndpoint = GroupTravelEndpoint;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelClientRecord {
    pub kind: GroupTravelClientKind,
    pub route: GroupTravelRoute,
    pub departure: GroupTravelDeparture,
    pub request_id: u32,
    pub proposal_id: [u8; 16],
    pub result: GroupTravelResult,
    pub reason: GroupTravelReason,
    /// Present only for Dig and Escape Rope routes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<GroupTravelEndpoint>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTravelServerRecord {
    pub kind: GroupTravelServerKind,
    pub route: GroupTravelRoute,
    pub departure: GroupTravelDeparture,
    pub request_id: u32,
    pub proposal_id: [u8; 16],
    pub result: GroupTravelResult,
    pub reason: GroupTravelReason,
    /// Server-derived whole seconds remaining for a pending vote (byte 28).
    pub remaining_seconds: u8,
    /// Present only for Dig and Escape Rope routes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<GroupTravelEndpoint>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum GroupTravelCodecError {
    #[error("group-travel record must be exactly 32 bytes, received {0}")]
    InvalidLength(usize),
    #[error("invalid group-travel client kind {0}")]
    InvalidClientKind(u8),
    #[error("invalid group-travel server kind {0}")]
    InvalidServerKind(u8),
    #[error("invalid group-travel route {0}")]
    InvalidRoute(u8),
    #[error("invalid group-travel departure {0}")]
    InvalidDeparture(u8),
    #[error("invalid group-travel era {0}")]
    InvalidEra(u8),
    #[error("invalid group-travel destination {0}")]
    InvalidDestination(u8),
    #[error("invalid group-travel result {0}")]
    InvalidResult(u8),
    #[error("invalid group-travel reason {0}")]
    InvalidReason(u8),
    #[error("request id zero is reserved")]
    RequestIdZero,
    #[error("proposal id is invalid for this phase")]
    InvalidProposalId,
    #[error("route, era, and destination disagree")]
    RouteMismatch,
    #[error("dynamic route endpoint is missing or static route endpoint is present")]
    EndpointMismatch,
    #[error("result or reason is invalid for this phase")]
    InvalidOutcome,
    #[error("reserved group-travel byte {0} must be zero")]
    NonZeroPadding(usize),
}

type DecodedCommon = (
    GroupTravelRoute,
    GroupTravelDeparture,
    u32,
    [u8; 16],
    GroupTravelResult,
    GroupTravelReason,
    Option<GroupTravelEndpoint>,
);

fn decode_common(bytes: &[u8]) -> Result<DecodedCommon, GroupTravelCodecError> {
    if bytes.len() != GROUP_TRAVEL_RECORD_SIZE {
        return Err(GroupTravelCodecError::InvalidLength(bytes.len()));
    }
    let route = GroupTravelRoute::from_wire(bytes[1])?;
    let endpoint = if matches!(route, GroupTravelRoute::Dig | GroupTravelRoute::EscapeRope) {
        Some(GroupTravelEndpoint::new(
            bytes[2],
            bytes[3],
            bytes[7],
            bytes[29],
            bytes[30] as i8,
            bytes[31] as i8,
        ))
    } else {
        for index in [7_usize, 29, 30, 31] {
            if bytes[index] != 0 {
                return Err(GroupTravelCodecError::NonZeroPadding(index));
            }
        }
        None
    };
    let departure = GroupTravelDeparture::from_wire(bytes[6])?;
    if !departure.matches_route(route) {
        return Err(GroupTravelCodecError::RouteMismatch);
    }
    if endpoint.is_none() {
        let era = GroupTravelEra::from_wire(bytes[2])?;
        let destination = GroupTravelDestination::from_wire(bytes[3])?;
        if era != route.era() || destination != route.destination() {
            return Err(GroupTravelCodecError::RouteMismatch);
        }
    }
    let request_id = u32::from_le_bytes(bytes[8..12].try_into().expect("fixed range"));
    if request_id == 0 {
        return Err(GroupTravelCodecError::RequestIdZero);
    }
    let proposal_id = bytes[12..28].try_into().expect("fixed range");
    Ok((
        route,
        departure,
        request_id,
        proposal_id,
        GroupTravelResult::from_wire(bytes[4])?,
        GroupTravelReason::from_wire(bytes[5])?,
        endpoint,
    ))
}

fn encode_common(
    kind: u8,
    route: GroupTravelRoute,
    departure: GroupTravelDeparture,
    request_id: u32,
    proposal_id: [u8; 16],
    result: GroupTravelResult,
    reason: GroupTravelReason,
    endpoint: Option<GroupTravelEndpoint>,
) -> [u8; GROUP_TRAVEL_RECORD_SIZE] {
    let mut bytes = [0; GROUP_TRAVEL_RECORD_SIZE];
    bytes[0] = kind;
    bytes[1] = route as u8;
    bytes[6] = departure as u8;
    if let Some(endpoint) = endpoint {
        bytes[2] = endpoint.source_map_group;
        bytes[3] = endpoint.source_map_number;
        bytes[7] = endpoint.target_map_group;
        bytes[29] = endpoint.target_map_number;
        bytes[30] = endpoint.target_x as u8;
        bytes[31] = endpoint.target_y as u8;
    } else {
        bytes[2] = route.era() as u8;
        bytes[3] = route.destination() as u8;
    }
    bytes[4] = result as u8;
    bytes[5] = reason as u8;
    bytes[8..12].copy_from_slice(&request_id.to_le_bytes());
    bytes[12..28].copy_from_slice(&proposal_id);
    bytes
}

fn proposal_is_zero(value: &[u8; 16]) -> bool {
    value.iter().all(|byte| *byte == 0)
}

impl GroupTravelClientRecord {
    /// Encodes the canonical fixed-size client record.
    ///
    /// # Errors
    /// Returns an error when phase fields or identifiers are inconsistent.
    pub fn encode(self) -> Result<[u8; GROUP_TRAVEL_RECORD_SIZE], GroupTravelCodecError> {
        self.validate()?;
        Ok(encode_common(
            self.kind as u8,
            self.route,
            self.departure,
            self.request_id,
            self.proposal_id,
            self.result,
            self.reason,
            self.endpoint,
        ))
    }
    /// Decodes and strictly validates a client record.
    ///
    /// # Errors
    /// Returns an error for size, enum, correlation, outcome, or padding violations.
    pub fn decode(bytes: &[u8]) -> Result<Self, GroupTravelCodecError> {
        let (route, departure, request_id, proposal_id, result, reason, endpoint) =
            decode_common(bytes)?;
        if bytes[28] != 0 {
            return Err(GroupTravelCodecError::NonZeroPadding(28));
        }
        let kind = match bytes[0] {
            1 => GroupTravelClientKind::Request,
            2 => GroupTravelClientKind::Decision,
            3 => GroupTravelClientKind::Cancel,
            4 => GroupTravelClientKind::Applied,
            5 => GroupTravelClientKind::SceneMarkerRequest,
            6 => GroupTravelClientKind::SceneComplete,
            value => return Err(GroupTravelCodecError::InvalidClientKind(value)),
        };
        let value = Self {
            kind,
            route,
            departure,
            request_id,
            proposal_id,
            result,
            reason,
            endpoint,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), GroupTravelCodecError> {
        if self.route.is_dynamic() != self.endpoint.is_some() {
            return Err(GroupTravelCodecError::EndpointMismatch);
        }
        if matches!(
            self.route,
            GroupTravelRoute::FerryBrineyHouseDewford
                | GroupTravelRoute::SeagallopBillCinnabarOne
                | GroupTravelRoute::SeagallopBillOneCinnabar
        ) && self.kind == GroupTravelClientKind::Applied
        {
            return Err(GroupTravelCodecError::InvalidOutcome);
        }
        if self.request_id == 0 {
            return Err(GroupTravelCodecError::RequestIdZero);
        }
        let zero = proposal_is_zero(&self.proposal_id);
        let valid = match self.kind {
            GroupTravelClientKind::Request => {
                zero && self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelClientKind::Decision => {
                !zero
                    && matches!(
                        self.result,
                        GroupTravelResult::Accepted | GroupTravelResult::Declined
                    )
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelClientKind::Cancel => {
                self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::RequesterCanceled
            }
            GroupTravelClientKind::Applied => {
                !zero
                    && self.result == GroupTravelResult::Applied
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelClientKind::SceneMarkerRequest => {
                matches!(
                    self.route,
                    GroupTravelRoute::FerryBrineyHouseDewford
                        | GroupTravelRoute::SeagallopBillCinnabarOne
                        | GroupTravelRoute::SeagallopBillOneCinnabar
                ) && !zero
                    && self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelClientKind::SceneComplete => {
                matches!(
                    self.route,
                    GroupTravelRoute::FerryBrineyHouseDewford
                        | GroupTravelRoute::SeagallopBillCinnabarOne
                        | GroupTravelRoute::SeagallopBillOneCinnabar
                ) && !zero
                    && self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::None
            }
        };
        if valid {
            Ok(())
        } else if matches!(
            self.kind,
            GroupTravelClientKind::Decision
                | GroupTravelClientKind::Applied
                | GroupTravelClientKind::SceneMarkerRequest
                | GroupTravelClientKind::SceneComplete
        ) && zero
        {
            Err(GroupTravelCodecError::InvalidProposalId)
        } else {
            Err(GroupTravelCodecError::InvalidOutcome)
        }
    }
}

impl GroupTravelServerRecord {
    /// Encodes the canonical fixed-size server record.
    ///
    /// # Errors
    /// Returns an error when phase fields or identifiers are inconsistent.
    pub fn encode(self) -> Result<[u8; GROUP_TRAVEL_RECORD_SIZE], GroupTravelCodecError> {
        self.validate()?;
        let mut bytes = encode_common(
            self.kind as u8,
            self.route,
            self.departure,
            self.request_id,
            self.proposal_id,
            self.result,
            self.reason,
            self.endpoint,
        );
        bytes[28] = self.remaining_seconds;
        Ok(bytes)
    }
    /// Decodes and strictly validates a server record.
    ///
    /// # Errors
    /// Returns an error for size, enum, correlation, outcome, or padding violations.
    pub fn decode(bytes: &[u8]) -> Result<Self, GroupTravelCodecError> {
        let (route, departure, request_id, proposal_id, result, reason, endpoint) =
            decode_common(bytes)?;
        let kind = match bytes[0] {
            1 => GroupTravelServerKind::Requesting,
            2 => GroupTravelServerKind::Offer,
            3 => GroupTravelServerKind::Commit,
            4 => GroupTravelServerKind::Abort,
            5 => GroupTravelServerKind::Complete,
            6 => GroupTravelServerKind::SceneMarkerAccepted,
            7 => GroupTravelServerKind::SceneReady,
            value => return Err(GroupTravelCodecError::InvalidServerKind(value)),
        };
        let value = Self {
            kind,
            route,
            departure,
            request_id,
            proposal_id,
            result,
            reason,
            remaining_seconds: bytes[28],
            endpoint,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), GroupTravelCodecError> {
        if self.route.is_dynamic() != self.endpoint.is_some() {
            return Err(GroupTravelCodecError::EndpointMismatch);
        }
        if matches!(
            self.route,
            GroupTravelRoute::FerryBrineyHouseDewford
                | GroupTravelRoute::SeagallopBillCinnabarOne
                | GroupTravelRoute::SeagallopBillOneCinnabar
        ) && self.kind == GroupTravelServerKind::Commit
        {
            return Err(GroupTravelCodecError::InvalidOutcome);
        }
        if matches!(
            self.kind,
            GroupTravelServerKind::SceneReady | GroupTravelServerKind::SceneMarkerAccepted
        ) && !matches!(
            self.route,
            GroupTravelRoute::FerryBrineyHouseDewford
                | GroupTravelRoute::SeagallopBillCinnabarOne
                | GroupTravelRoute::SeagallopBillOneCinnabar
        ) {
            return Err(GroupTravelCodecError::InvalidOutcome);
        }
        if self.remaining_seconds > 30
            || (!matches!(
                self.kind,
                GroupTravelServerKind::Requesting | GroupTravelServerKind::Offer
            ) && self.remaining_seconds != 0)
        {
            return Err(GroupTravelCodecError::InvalidOutcome);
        }
        if self.request_id == 0 {
            return Err(GroupTravelCodecError::RequestIdZero);
        }
        let zero = proposal_is_zero(&self.proposal_id);
        let valid = match self.kind {
            GroupTravelServerKind::Requesting => {
                zero && self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelServerKind::Offer
            | GroupTravelServerKind::Commit
            | GroupTravelServerKind::SceneReady
            | GroupTravelServerKind::SceneMarkerAccepted => {
                !zero
                    && self.result == GroupTravelResult::None
                    && self.reason == GroupTravelReason::None
            }
            GroupTravelServerKind::Abort => {
                self.result == GroupTravelResult::None
                    && self.reason != GroupTravelReason::None
                    && (!zero
                        || matches!(
                            self.reason,
                            GroupTravelReason::Conflict | GroupTravelReason::Unsafe
                        ))
            }
            GroupTravelServerKind::Complete => {
                !zero
                    && self.result == GroupTravelResult::Applied
                    && self.reason == GroupTravelReason::None
            }
        };
        if valid {
            Ok(())
        } else if !matches!(
            self.kind,
            GroupTravelServerKind::Requesting | GroupTravelServerKind::Abort
        ) && zero
        {
            Err(GroupTravelCodecError::InvalidProposalId)
        } else {
            Err(GroupTravelCodecError::InvalidOutcome)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn golden_vectors_cover_all_consent_destinations() {
        for (index, route) in [
            GroupTravelRoute::TrainOriginal,
            GroupTravelRoute::TrainLater,
            GroupTravelRoute::FerryOriginal,
            GroupTravelRoute::FerryLater,
            GroupTravelRoute::GateOriginal,
            GroupTravelRoute::GateLater,
            GroupTravelRoute::FlyLittleroot,
            GroupTravelRoute::FlyJohtoNewbark,
            GroupTravelRoute::FlyJohtoCherrygrove,
            GroupTravelRoute::FlyJohtoViolet,
            GroupTravelRoute::FlyJohtoAzalea,
            GroupTravelRoute::FlyJohtoGoldenrod,
            GroupTravelRoute::FlyJohtoEcruteak,
            GroupTravelRoute::FlyJohtoOlivine,
            GroupTravelRoute::FlyJohtoCianwood,
            GroupTravelRoute::FlyJohtoMahogany,
            GroupTravelRoute::FlyJohtoBlackthorn,
            GroupTravelRoute::FlyHoennOldale,
            GroupTravelRoute::FlyHoennDewford,
            GroupTravelRoute::FlyHoennLavaridge,
            GroupTravelRoute::FlyHoennFallarbor,
            GroupTravelRoute::FlyHoennVerdanturf,
            GroupTravelRoute::FlyHoennPacifidlog,
            GroupTravelRoute::FlyHoennPetalburg,
            GroupTravelRoute::FlyHoennSlateport,
            GroupTravelRoute::FlyHoennMauville,
            GroupTravelRoute::FlyHoennRustboro,
            GroupTravelRoute::FlyHoennFortree,
            GroupTravelRoute::FlyHoennLilycove,
            GroupTravelRoute::FlyHoennMossdeep,
            GroupTravelRoute::FlyHoennSootopolis,
            GroupTravelRoute::FlyKantoOriginalPallet,
            GroupTravelRoute::FlyKantoOriginalViridian,
            GroupTravelRoute::FlyKantoOriginalPewter,
            GroupTravelRoute::FlyKantoOriginalCerulean,
            GroupTravelRoute::FlyKantoOriginalLavender,
            GroupTravelRoute::FlyKantoOriginalVermilion,
            GroupTravelRoute::FlyKantoOriginalCeladon,
            GroupTravelRoute::FlyKantoOriginalFuchsia,
            GroupTravelRoute::FlyKantoOriginalCinnabar,
            GroupTravelRoute::FlyKantoOriginalIndigo,
            GroupTravelRoute::FlyKantoOriginalSaffron,
            GroupTravelRoute::FlyKantoLaterPallet,
            GroupTravelRoute::FlyKantoLaterViridian,
            GroupTravelRoute::FlyKantoLaterPewter,
            GroupTravelRoute::FlyKantoLaterCerulean,
            GroupTravelRoute::FlyKantoLaterLavender,
            GroupTravelRoute::FlyKantoLaterVermilion,
            GroupTravelRoute::FlyKantoLaterCeladon,
            GroupTravelRoute::FlyKantoLaterFuchsia,
            GroupTravelRoute::FlyKantoLaterSaffron,
            GroupTravelRoute::FlyKantoLaterCinnabar,
            GroupTravelRoute::FlySeviiOneIsland,
            GroupTravelRoute::FlySeviiTwoIsland,
            GroupTravelRoute::FlySeviiThreeIsland,
            GroupTravelRoute::FlySeviiFourIsland,
            GroupTravelRoute::FlySeviiFiveIsland,
            GroupTravelRoute::FlySeviiSevenIsland,
            GroupTravelRoute::FlySeviiSixIsland,
            GroupTravelRoute::FlyKantoRoute4PokemonCenter,
            GroupTravelRoute::FlyKantoRoute10PokemonCenter,
            GroupTravelRoute::FlyHoennEverGrandeCenter,
            GroupTravelRoute::FlyHoennEverGrandeLeague,
            GroupTravelRoute::FlyHoennBattleFrontier,
            GroupTravelRoute::ReturnFerryOriginal,
            GroupTravelRoute::ReturnFerryLater,
            GroupTravelRoute::ReturnTrainOriginal,
            GroupTravelRoute::ReturnTrainLater,
            GroupTravelRoute::ReturnGateOriginal,
            GroupTravelRoute::ReturnGateLater,
            GroupTravelRoute::FerryOlivineSouthernIsland,
            GroupTravelRoute::FerryOlivineBirthIsland,
            GroupTravelRoute::FerryOlivineFarawayIsland,
            GroupTravelRoute::FerryOlivineBattleFrontier,
            GroupTravelRoute::FerryVermilionSouthernIsland,
            GroupTravelRoute::FerryVermilionBirthIsland,
            GroupTravelRoute::FerryVermilionFarawayIsland,
            GroupTravelRoute::FerryVermilionBattleFrontier,
            GroupTravelRoute::FerrySouthernIslandLilycove,
            GroupTravelRoute::FerryBirthIslandLilycove,
            GroupTravelRoute::FerryFarawayIslandLilycove,
            GroupTravelRoute::FerryBattleFrontierSlateport,
            GroupTravelRoute::FerryBattleFrontierLilycove,
            GroupTravelRoute::FerryLilycoveSouthernIsland,
            GroupTravelRoute::FerryLilycoveNavelRock,
            GroupTravelRoute::FerryLilycoveBirthIsland,
            GroupTravelRoute::FerryLilycoveFarawayIsland,
            GroupTravelRoute::FerryLilycoveBattleFrontier,
            GroupTravelRoute::FerrySlateportBattleFrontier,
            GroupTravelRoute::FerryNavelRockLilycove,
            GroupTravelRoute::FerrySSTidalSlateportBoard,
            GroupTravelRoute::FerrySSTidalLilycoveBoard,
            GroupTravelRoute::FerrySSTidalLilycoveExit,
            GroupTravelRoute::FerrySSTidalSlateportExit,
            GroupTravelRoute::FerryDewfordBrineyHouse,
            GroupTravelRoute::FerryDewfordRoute109,
            GroupTravelRoute::FerryRoute109Dewford,
            GroupTravelRoute::SeagallopVermilionOne,
            GroupTravelRoute::SeagallopVermilionTwo,
            GroupTravelRoute::SeagallopVermilionThree,
            GroupTravelRoute::SeagallopVermilionFour,
            GroupTravelRoute::SeagallopVermilionFive,
            GroupTravelRoute::SeagallopVermilionSix,
            GroupTravelRoute::SeagallopVermilionSeven,
            GroupTravelRoute::SeagallopOneVermilion,
            GroupTravelRoute::SeagallopOneTwo,
            GroupTravelRoute::SeagallopOneThree,
            GroupTravelRoute::SeagallopOneFour,
            GroupTravelRoute::SeagallopOneFive,
            GroupTravelRoute::SeagallopOneSix,
            GroupTravelRoute::SeagallopOneSeven,
            GroupTravelRoute::SeagallopTwoVermilion,
            GroupTravelRoute::SeagallopTwoOne,
            GroupTravelRoute::SeagallopTwoThree,
            GroupTravelRoute::SeagallopTwoFour,
            GroupTravelRoute::SeagallopTwoFive,
            GroupTravelRoute::SeagallopTwoSix,
            GroupTravelRoute::SeagallopTwoSeven,
            GroupTravelRoute::SeagallopThreeVermilion,
            GroupTravelRoute::SeagallopThreeOne,
            GroupTravelRoute::SeagallopThreeTwo,
            GroupTravelRoute::SeagallopThreeFour,
            GroupTravelRoute::SeagallopThreeFive,
            GroupTravelRoute::SeagallopThreeSix,
            GroupTravelRoute::SeagallopThreeSeven,
            GroupTravelRoute::SeagallopFourVermilion,
            GroupTravelRoute::SeagallopFourOne,
            GroupTravelRoute::SeagallopFourTwo,
            GroupTravelRoute::SeagallopFourThree,
            GroupTravelRoute::SeagallopFourFive,
            GroupTravelRoute::SeagallopFourSix,
            GroupTravelRoute::SeagallopFourSeven,
            GroupTravelRoute::SeagallopFiveVermilion,
            GroupTravelRoute::SeagallopFiveOne,
            GroupTravelRoute::SeagallopFiveTwo,
            GroupTravelRoute::SeagallopFiveThree,
            GroupTravelRoute::SeagallopFiveFour,
            GroupTravelRoute::SeagallopFiveSix,
            GroupTravelRoute::SeagallopFiveSeven,
            GroupTravelRoute::SeagallopSixVermilion,
            GroupTravelRoute::SeagallopSixOne,
            GroupTravelRoute::SeagallopSixTwo,
            GroupTravelRoute::SeagallopSixThree,
            GroupTravelRoute::SeagallopSixFour,
            GroupTravelRoute::SeagallopSixFive,
            GroupTravelRoute::SeagallopSixSeven,
            GroupTravelRoute::SeagallopSevenVermilion,
            GroupTravelRoute::SeagallopSevenOne,
            GroupTravelRoute::SeagallopSevenTwo,
            GroupTravelRoute::SeagallopSevenThree,
            GroupTravelRoute::SeagallopSevenFour,
            GroupTravelRoute::SeagallopSevenFive,
            GroupTravelRoute::SeagallopSevenSix,
            GroupTravelRoute::SeagallopVermilionNavel,
            GroupTravelRoute::SeagallopNavelVermilion,
            GroupTravelRoute::SeagallopVermilionBirth,
            GroupTravelRoute::SeagallopBirthVermilion,
            GroupTravelRoute::SeagallopBillCinnabarOne,
            GroupTravelRoute::SeagallopBillOneCinnabar,
        ]
        .into_iter()
        .enumerate()
        {
            let departure = match route {
                GroupTravelRoute::TrainOriginal | GroupTravelRoute::TrainLater => {
                    GroupTravelDeparture::Train
                }
                GroupTravelRoute::ReturnTrainOriginal | GroupTravelRoute::ReturnTrainLater => {
                    GroupTravelDeparture::Train
                }
                GroupTravelRoute::FerryOriginal | GroupTravelRoute::FerryLater => {
                    GroupTravelDeparture::Ferry
                }
                GroupTravelRoute::ReturnFerryOriginal | GroupTravelRoute::ReturnFerryLater => {
                    GroupTravelDeparture::Ferry
                }
                GroupTravelRoute::GateOriginal | GroupTravelRoute::GateLater => {
                    GroupTravelDeparture::Gate
                }
                GroupTravelRoute::ReturnGateOriginal | GroupTravelRoute::ReturnGateLater => {
                    GroupTravelDeparture::Gate
                }
                GroupTravelRoute::FerryOlivineSouthernIsland
                | GroupTravelRoute::FerryOlivineBirthIsland
                | GroupTravelRoute::FerryOlivineFarawayIsland
                | GroupTravelRoute::FerryOlivineBattleFrontier
                | GroupTravelRoute::FerryVermilionSouthernIsland
                | GroupTravelRoute::FerryVermilionBirthIsland
                | GroupTravelRoute::FerryVermilionFarawayIsland
                | GroupTravelRoute::FerryVermilionBattleFrontier
                | GroupTravelRoute::FerrySouthernIslandLilycove
                | GroupTravelRoute::FerryBirthIslandLilycove
                | GroupTravelRoute::FerryFarawayIslandLilycove
                | GroupTravelRoute::FerryBattleFrontierSlateport
                | GroupTravelRoute::FerryBattleFrontierLilycove
                | GroupTravelRoute::FerryLilycoveSouthernIsland
                | GroupTravelRoute::FerryLilycoveNavelRock
                | GroupTravelRoute::FerryLilycoveBirthIsland
                | GroupTravelRoute::FerryLilycoveFarawayIsland
                | GroupTravelRoute::FerryLilycoveBattleFrontier
                | GroupTravelRoute::FerrySlateportBattleFrontier
                | GroupTravelRoute::FerryNavelRockLilycove
                | GroupTravelRoute::FerrySSTidalSlateportBoard
                | GroupTravelRoute::FerrySSTidalLilycoveBoard
                | GroupTravelRoute::FerrySSTidalLilycoveExit
                | GroupTravelRoute::FerrySSTidalSlateportExit
                | GroupTravelRoute::FerryDewfordBrineyHouse
                | GroupTravelRoute::FerryDewfordRoute109
                | GroupTravelRoute::FerryRoute109Dewford
                | GroupTravelRoute::SeagallopVermilionOne
                | GroupTravelRoute::SeagallopVermilionTwo
                | GroupTravelRoute::SeagallopVermilionThree
                | GroupTravelRoute::SeagallopVermilionFour
                | GroupTravelRoute::SeagallopVermilionFive
                | GroupTravelRoute::SeagallopVermilionSix
                | GroupTravelRoute::SeagallopVermilionSeven
                | GroupTravelRoute::SeagallopOneVermilion
                | GroupTravelRoute::SeagallopOneTwo
                | GroupTravelRoute::SeagallopOneThree
                | GroupTravelRoute::SeagallopOneFour
                | GroupTravelRoute::SeagallopOneFive
                | GroupTravelRoute::SeagallopOneSix
                | GroupTravelRoute::SeagallopOneSeven
                | GroupTravelRoute::SeagallopTwoVermilion
                | GroupTravelRoute::SeagallopTwoOne
                | GroupTravelRoute::SeagallopTwoThree
                | GroupTravelRoute::SeagallopTwoFour
                | GroupTravelRoute::SeagallopTwoFive
                | GroupTravelRoute::SeagallopTwoSix
                | GroupTravelRoute::SeagallopTwoSeven
                | GroupTravelRoute::SeagallopThreeVermilion
                | GroupTravelRoute::SeagallopThreeOne
                | GroupTravelRoute::SeagallopThreeTwo
                | GroupTravelRoute::SeagallopThreeFour
                | GroupTravelRoute::SeagallopThreeFive
                | GroupTravelRoute::SeagallopThreeSix
                | GroupTravelRoute::SeagallopThreeSeven
                | GroupTravelRoute::SeagallopFourVermilion
                | GroupTravelRoute::SeagallopFourOne
                | GroupTravelRoute::SeagallopFourTwo
                | GroupTravelRoute::SeagallopFourThree
                | GroupTravelRoute::SeagallopFourFive
                | GroupTravelRoute::SeagallopFourSix
                | GroupTravelRoute::SeagallopFourSeven
                | GroupTravelRoute::SeagallopFiveVermilion
                | GroupTravelRoute::SeagallopFiveOne
                | GroupTravelRoute::SeagallopFiveTwo
                | GroupTravelRoute::SeagallopFiveThree
                | GroupTravelRoute::SeagallopFiveFour
                | GroupTravelRoute::SeagallopFiveSix
                | GroupTravelRoute::SeagallopFiveSeven
                | GroupTravelRoute::SeagallopSixVermilion
                | GroupTravelRoute::SeagallopSixOne
                | GroupTravelRoute::SeagallopSixTwo
                | GroupTravelRoute::SeagallopSixThree
                | GroupTravelRoute::SeagallopSixFour
                | GroupTravelRoute::SeagallopSixFive
                | GroupTravelRoute::SeagallopSixSeven
                | GroupTravelRoute::SeagallopSevenVermilion
                | GroupTravelRoute::SeagallopSevenOne
                | GroupTravelRoute::SeagallopSevenTwo
                | GroupTravelRoute::SeagallopSevenThree
                | GroupTravelRoute::SeagallopSevenFour
                | GroupTravelRoute::SeagallopSevenFive
                | GroupTravelRoute::SeagallopSevenSix
                | GroupTravelRoute::SeagallopVermilionNavel
                | GroupTravelRoute::SeagallopNavelVermilion
                | GroupTravelRoute::SeagallopVermilionBirth
                | GroupTravelRoute::SeagallopBirthVermilion
                | GroupTravelRoute::SeagallopBillCinnabarOne
                | GroupTravelRoute::SeagallopBillOneCinnabar => GroupTravelDeparture::Ferry,
                route if route.is_fly() => GroupTravelDeparture::Fly,
                _ => unreachable!("all fixed travel routes are covered"),
            };
            let record = GroupTravelClientRecord {
                kind: GroupTravelClientKind::Request,
                route,
                departure,
                request_id: u32::try_from(index).unwrap() + 1,
                proposal_id: [0; 16],
                result: GroupTravelResult::None,
                reason: GroupTravelReason::None,
                endpoint: None,
            };
            let encoded = record.encode().unwrap();
            assert_eq!(encoded[1], route as u8);
            assert_eq!(GroupTravelClientRecord::decode(&encoded).unwrap(), record);
        }
    }

    #[test]
    fn ever_grande_wire_slot_remains_reserved() {
        assert_eq!(
            GroupTravelRoute::from_wire(92).unwrap(),
            GroupTravelRoute::FerrySSTidalSlateportBoard
        );
        assert_eq!(
            GroupTravelRoute::from_wire(93).unwrap(),
            GroupTravelRoute::FerrySSTidalLilycoveBoard
        );
        assert_eq!(
            GroupTravelRoute::from_wire(96).unwrap(),
            GroupTravelRoute::FerryBrineyHouseDewford
        );
        assert_eq!(
            GroupTravelRoute::from_wire(160).unwrap(),
            GroupTravelRoute::SeagallopBillCinnabarOne
        );
        assert_eq!(
            GroupTravelRoute::from_wire(161).unwrap(),
            GroupTravelRoute::SeagallopBillOneCinnabar
        );
        let mut encoded = GroupTravelClientRecord {
            kind: GroupTravelClientKind::Request,
            route: GroupTravelRoute::FlyHoennSootopolis,
            departure: GroupTravelDeparture::Fly,
            request_id: 1,
            proposal_id: [0; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            endpoint: None,
        }
        .encode()
        .unwrap();
        encoded[1] = 32;
        assert!(matches!(
            GroupTravelClientRecord::decode(&encoded),
            Err(GroupTravelCodecError::InvalidRoute(32))
        ));
    }

    #[test]
    fn cable_car_routes_have_distinct_fixed_destinations_and_departure() {
        for (wire, route, destination) in [
            (
                162,
                GroupTravelRoute::CableCarRoute112MtChimney,
                GroupTravelDestination::MtChimneyCableCarStation,
            ),
            (
                163,
                GroupTravelRoute::CableCarMtChimneyRoute112,
                GroupTravelDestination::Route112CableCarStation,
            ),
        ] {
            assert_eq!(GroupTravelRoute::from_wire(wire).unwrap(), route);
            assert_eq!(route.era(), GroupTravelEra::Hoenn);
            assert_eq!(route.destination(), destination);
            assert!(GroupTravelDeparture::CableCar.matches_route(route));
            assert!(!GroupTravelDeparture::Ferry.matches_route(route));
            let request = GroupTravelClientRecord {
                kind: GroupTravelClientKind::Request,
                route,
                departure: GroupTravelDeparture::CableCar,
                request_id: 1,
                proposal_id: [0; 16],
                result: GroupTravelResult::None,
                reason: GroupTravelReason::None,
                endpoint: None,
            };
            assert_eq!(
                GroupTravelClientRecord::decode(&request.encode().unwrap()).unwrap(),
                request
            );
        }
    }

    #[test]
    fn dynamic_routes_round_trip_exact_endpoint_without_changing_record_size() {
        let endpoint = GroupTravelEndpoint::new(7, 11, 19, 23, -3, 127);
        let request = GroupTravelClientRecord {
            kind: GroupTravelClientKind::Request,
            route: GroupTravelRoute::Dig,
            departure: GroupTravelDeparture::Dig,
            request_id: 41,
            proposal_id: [0; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            endpoint: Some(endpoint),
        };
        let encoded = request.encode().unwrap();
        assert_eq!(encoded.len(), GROUP_TRAVEL_RECORD_SIZE);
        assert_eq!(encoded[1], 164);
        assert_eq!(encoded[6], 8);
        assert_eq!(encoded[2], 7);
        assert_eq!(encoded[3], 11);
        assert_eq!(encoded[7], 19);
        assert_eq!(encoded[29], 23);
        assert_eq!(encoded[30], (-3_i8) as u8);
        assert_eq!(encoded[31], 127);
        assert_eq!(GroupTravelClientRecord::decode(&encoded).unwrap(), request);

        let offer = GroupTravelServerRecord {
            kind: GroupTravelServerKind::Offer,
            route: GroupTravelRoute::EscapeRope,
            departure: GroupTravelDeparture::EscapeRope,
            request_id: 42,
            proposal_id: [9; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            remaining_seconds: 17,
            endpoint: Some(endpoint),
        };
        let encoded = offer.encode().unwrap();
        assert_eq!(encoded[1], 165);
        assert_eq!(encoded[6], 9);
        assert_eq!(encoded[28], 17);
        assert_eq!(GroupTravelServerRecord::decode(&encoded).unwrap(), offer);

        assert!(
            GroupTravelClientRecord {
                route: GroupTravelRoute::TrainLater,
                departure: GroupTravelDeparture::Train,
                endpoint: Some(endpoint),
                ..request
            }
            .encode()
            .is_err()
        );
    }

    #[test]
    fn teleport_reuses_fly_destinations_but_excludes_littleroot() {
        let route = GroupTravelRoute::FlyJohtoNewbark;
        assert_eq!(
            GroupTravelDeparture::from_wire(7).unwrap(),
            GroupTravelDeparture::Teleport
        );
        assert!(GroupTravelDeparture::Teleport.matches_route(route));
        assert!(!GroupTravelDeparture::Teleport.matches_route(GroupTravelRoute::FlyLittleroot));
        let request = GroupTravelClientRecord {
            kind: GroupTravelClientKind::Request,
            route,
            departure: GroupTravelDeparture::Teleport,
            request_id: 1,
            proposal_id: [0; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            endpoint: None,
        };
        let encoded = request.encode().unwrap();
        assert_eq!(encoded[6], 7);
        assert_eq!(GroupTravelClientRecord::decode(&encoded).unwrap(), request);
    }
    #[test]
    fn story_marker_wire_requires_correlated_first_voyage() {
        let marker = GroupTravelClientRecord {
            kind: GroupTravelClientKind::SceneMarkerRequest,
            route: GroupTravelRoute::FerryBrineyHouseDewford,
            departure: GroupTravelDeparture::Ferry,
            request_id: 42,
            proposal_id: [7; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            endpoint: None,
        };
        assert_eq!(
            GroupTravelClientRecord::decode(&marker.encode().unwrap()).unwrap(),
            marker
        );
        let complete = GroupTravelClientRecord {
            kind: GroupTravelClientKind::SceneComplete,
            ..marker
        };
        assert_eq!(
            GroupTravelClientRecord::decode(&complete.encode().unwrap()).unwrap(),
            complete
        );
        assert!(
            GroupTravelClientRecord {
                proposal_id: [0; 16],
                ..complete
            }
            .encode()
            .is_err()
        );
        assert!(
            GroupTravelClientRecord {
                route: GroupTravelRoute::FerryDewfordBrineyHouse,
                ..marker
            }
            .encode()
            .is_err()
        );
        assert!(
            GroupTravelClientRecord {
                proposal_id: [0; 16],
                ..marker
            }
            .encode()
            .is_err()
        );
        for kind in [
            GroupTravelServerKind::SceneReady,
            GroupTravelServerKind::SceneMarkerAccepted,
        ] {
            let response = GroupTravelServerRecord {
                kind,
                route: marker.route,
                departure: marker.departure,
                request_id: marker.request_id,
                proposal_id: marker.proposal_id,
                result: GroupTravelResult::None,
                reason: GroupTravelReason::None,
                remaining_seconds: 0,
                endpoint: None,
            };
            assert_eq!(
                GroupTravelServerRecord::decode(&response.encode().unwrap()).unwrap(),
                response
            );
            assert!(
                GroupTravelServerRecord {
                    route: GroupTravelRoute::FerryDewfordBrineyHouse,
                    ..response
                }
                .encode()
                .is_err()
            );
        }
    }

    #[test]
    fn story_complete_wire_is_valid_but_premature_commit_is_not() {
        for route in [
            GroupTravelRoute::FerryBrineyHouseDewford,
            GroupTravelRoute::SeagallopBillCinnabarOne,
            GroupTravelRoute::SeagallopBillOneCinnabar,
        ] {
            let complete = GroupTravelServerRecord {
                kind: GroupTravelServerKind::Complete,
                route,
                departure: GroupTravelDeparture::Ferry,
                request_id: 8,
                proposal_id: [1; 16],
                result: GroupTravelResult::Applied,
                reason: GroupTravelReason::None,
                remaining_seconds: 0,
                endpoint: None,
            };
            assert_eq!(
                GroupTravelServerRecord::decode(&complete.encode().unwrap()).unwrap(),
                complete
            );
            assert!(
                GroupTravelServerRecord {
                    kind: GroupTravelServerKind::Commit,
                    result: GroupTravelResult::None,
                    ..complete
                }
                .encode()
                .is_err()
            );
        }
    }
    #[test]
    fn rejects_direction_kind_padding_and_mismatch() {
        let record = GroupTravelServerRecord {
            kind: GroupTravelServerKind::Commit,
            route: GroupTravelRoute::GateLater,
            departure: GroupTravelDeparture::Gate,
            request_id: 7,
            proposal_id: [9; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            remaining_seconds: 0,
            endpoint: None,
        };
        let mut encoded = record.encode().unwrap();
        assert!(GroupTravelClientRecord::decode(&encoded).is_err());
        encoded[28] = 1;
        assert_eq!(
            GroupTravelServerRecord::decode(&encoded),
            Err(GroupTravelCodecError::InvalidOutcome)
        );
        assert_eq!(
            GroupTravelClientRecord::decode(&encoded),
            Err(GroupTravelCodecError::NonZeroPadding(28))
        );
        encoded[28] = 0;
        encoded[6] = GroupTravelDeparture::Train as u8;
        assert_eq!(
            GroupTravelServerRecord::decode(&encoded),
            Err(GroupTravelCodecError::RouteMismatch)
        );
        encoded[6] = GroupTravelDeparture::Gate as u8;
        encoded[2] = 1;
        assert_eq!(
            GroupTravelServerRecord::decode(&encoded),
            Err(GroupTravelCodecError::RouteMismatch)
        );
    }
    #[test]
    fn pending_offer_carries_bounded_vote_seconds_in_reserved_byte() {
        let record = GroupTravelServerRecord {
            kind: GroupTravelServerKind::Offer,
            route: GroupTravelRoute::TrainOriginal,
            departure: GroupTravelDeparture::Train,
            request_id: 4,
            proposal_id: [5; 16],
            result: GroupTravelResult::None,
            reason: GroupTravelReason::None,
            remaining_seconds: 24,
            endpoint: None,
        };
        let encoded = record.encode().unwrap();
        assert_eq!(encoded[28], 24);
        assert_eq!(GroupTravelServerRecord::decode(&encoded), Ok(record));
        assert_eq!(
            GroupTravelClientRecord::decode(&encoded),
            Err(GroupTravelCodecError::NonZeroPadding(28))
        );
        let invalid = GroupTravelServerRecord {
            remaining_seconds: 31,
            ..record
        };
        assert_eq!(invalid.encode(), Err(GroupTravelCodecError::InvalidOutcome));
    }
}
