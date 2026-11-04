//! Crafting methods as data: what a currency, bench option, essence, fossil, catalyst, harvest
//! craft, beastcraft or unveil needs from an item (`Require`, `PairRequire`) and what it does to
//! it (`Op`, run by `apply`). Hand-authored methods live in `rules/v2/methods.json`; `datagen-v2`
//! generates the bench, essence, catalyst and fossil families.

use serde::{Deserialize, Serialize};

use crate::data::{Category, Eldritch, Influence, Rarity};

fn one() -> u8 {
    1
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Method {
    pub id: String,
    pub name: String,
    pub family: Family,
    #[serde(default = "one")]
    pub inputs: u8,
    #[serde(default)]
    pub cost: Vec<CostSpec>,
    #[serde(default)]
    pub requires: Vec<Require>,
    #[serde(default)]
    pub pair: Vec<PairRequire>,
    pub ops: Vec<Op>,
    #[serde(default)]
    pub claims: Vec<String>,
    #[serde(default)]
    pub game_text: Option<String>,
    /// The fixture the catalog demonstrates the method on.
    #[serde(default)]
    pub example: Option<String>,
}

/// A currency name, or `beast:<name>`; datagen resolves it to a `Cost`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CostSpec {
    pub name: String,
    pub amount: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    Currency,
    Bench,
    Essence,
    Fossil,
    Catalyst,
    Harvest,
    Beast,
    Unveil,
}

impl Family {
    pub const ALL: [Family; 8] = [
        Family::Currency,
        Family::Bench,
        Family::Essence,
        Family::Fossil,
        Family::Catalyst,
        Family::Harvest,
        Family::Beast,
        Family::Unveil,
    ];
}

/// One precondition every input must meet.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Require {
    Rarity {
        any_of: Vec<Rarity>,
    },
    Category {
        any_of: Vec<Category>,
    },
    /// Class ids (`ClassData::id`).
    Class {
        any_of: Vec<String>,
    },
    NotCorrupted,
    Corrupted,
    NotMirrored,
    NotSplit,
    NotSynthesised,
    NotUnique,
    NoFractured,
    NoEnchant,
    NoInfluence,
    HasInfluence {
        #[serde(default)]
        influence: Option<Influence>,
    },
    SingleInfluence,
    Influenceable,
    OpenAffix {
        side: SideReq,
    },
    FullSuffixes,
    MinExplicits {
        n: u8,
    },
    MinInfluencedMods {
        n: u8,
    },
    HasImplicit,
    HasCrafted,
    HasEnchant,
    CraftedRoom,
    GroupFree {
        r#mod: String,
    },
    HasVeiled,
    NoVeiled,
    QualityBelow {
        max: u8,
    },
    CatalystRoom {
        catalyst: String,
    },
    HasImprint,
    NotForeseen,
    EldritchDominant,
    BothEldritch,
    NoEldritch,
    HasSockets,
    CanHaveSockets,
    Intangible,
    HasMemoryStrands,
    HasModType {
        any_of: Vec<String>,
    },
    ItemLevelAtMost {
        n: u8,
    },
    EssenceFits {
        essence: String,
    },
    Fossils {
        min: u8,
        max: u8,
    },
    ImplicitsChangeable,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum SideReq {
    Any,
    Prefix,
    Suffix,
    /// The side of the dominant eldritch implicit: Exarch = prefix, Eater = suffix.
    Dominant,
}

/// A precondition on the two inputs of a two-input method.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PairRequire {
    SameClass,
    DifferentSingleInfluence,
}

/// One step of what a method does; `apply` runs them in order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    SetRarity {
        rarity: Rarity,
    },
    ClearExplicits,
    RerollMagic,
    ClearAll,
    AddRandom {
        pool: Pool,
        side: SideReq,
        #[serde(default)]
        ignore_limits: bool,
    },
    RemoveRandom {
        filter: RemoveFilter,
        #[serde(default)]
        keep_last_added: bool,
        #[serde(default)]
        respects_cannot_roll: bool,
    },
    AddVeiled,
    Unveil,
    AddMod {
        r#mod: String,
    },
    RemoveCrafted,
    SetEnchant {
        pool: EnchantPool,
    },
    SetEnchantMod {
        r#mod: String,
    },
    ClearEnchants,
    RerollValues {
        scope: ValueScope,
    },
    Fracture,
    AddInfluence {
        influence: Influence,
    },
    Corrupt,
    VaalImplicit,
    EldritchImplicit {
        eldritch: Eldritch,
        tier: u8,
    },
    Dominance,
    ChangeResistance {
        from: Element,
        to: Element,
    },
    AddQuality {
        by_rarity: [u8; 4],
        max: u8,
    },
    CatalystQuality {
        catalyst: String,
    },
    Foresee,
    Imprint,
    Restore,
    LinkAll,
    Mirror,
    Destroy,
    RemoveIntangibility,
    Chance {
        branches: Vec<Branch>,
    },
    Reforge {
        spec: ReforgeSpec,
    },
    /// "Add the reforge's remaining random mods"; never run in this pass.
    Fill {
        spec: ReforgeSpec,
    },
    Sockets {
        change: SocketChange,
    },
    Unmodelled {
        reason: PendingReason,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Branch {
    pub p: f64,
    #[serde(default)]
    pub ops: Vec<Op>,
}

/// Where an added random modifier comes from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Pool {
    Natural,
    Influence { influence: Influence },
    Tag { tag: String },
    Ducat { tag: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RemoveFilter {
    Any,
    Side { side: SideReq },
    Influenced,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EnchantPool {
    Lab,
    FlaskEnkindling,
    FlaskInstilling,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ValueScope {
    Explicit,
    Implicit,
    Defences,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Element {
    Fire,
    Cold,
    Lightning,
}

impl Element {
    /// The RePoE mod type of this element's resistance mods.
    pub fn resistance_type(self) -> &'static str {
        match self {
            Element::Fire => "FireResistance",
            Element::Cold => "ColdResistance",
            Element::Lightning => "LightningResistance",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReforgeSpec {
    pub rarity: Rarity,
    #[serde(default)]
    pub side: Option<SideReq>,
    #[serde(default)]
    pub respects_locks: bool,
    #[serde(default)]
    pub guarantee: Option<Guarantee>,
    #[serde(default)]
    pub weighting: Weighting,
}

/// The part of a reforge that is certain.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Guarantee {
    Essence,
    Veiled,
    Tag { tag: String },
    Infamous,
    InfluenceMod,
    InfluenceFromEachInput,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Weighting {
    #[default]
    Normal,
    Fossils,
    MoreLikelySame,
    LessLikelySame,
    Recombine,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SocketChange {
    RandomCount,
    Count,
    RandomLinks,
    Links,
    RandomColours,
    Colours,
    LinkUpToFour,
    TaintedCount,
    TaintedLinks,
    TaintedColours,
    Reforge,
}

/// Why an outcome is not computed in this pass.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PendingReason {
    MultiDraw,
    Sockets,
    Unverified,
    NeedsData { what: String },
    TooLarge,
}

impl PendingReason {
    /// The catalog's text for this reason.
    pub fn text(&self) -> String {
        match self {
            PendingReason::MultiDraw => "multi-draw reforge".into(),
            PendingReason::Sockets => "socket outcome table".into(),
            PendingReason::Unverified => "rule not yet confirmed".into(),
            PendingReason::NeedsData { what } => format!("needs data: {what}"),
            PendingReason::TooLarge => "more than 100000 outcomes".into(),
        }
    }
}
