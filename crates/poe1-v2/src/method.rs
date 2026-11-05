//! Crafting methods as data: what a currency, bench option, essence, fossil, catalyst, harvest
//! craft, beastcraft or unveil needs from an item (`Require`, `PairRequire`), how it treats the
//! metamods that protect modifiers (`Protection`), and what it does to it (`Op`, run by `apply`).
//! A reforge is written as primitives: set the rarity, clear what can change, place the
//! guaranteed modifier, then `Fill` with the typed mod-count table. Hand-authored methods live in
//! `rules/v2/methods.json`; `datagen-v2` generates the bench, essence, catalyst and fossil
//! families. `Method::validate` resolves every id a method names against the bundle, and
//! `Data::from_bundle` runs it on every method, so a bundle with a bad method does not load.

use serde::{Deserialize, Serialize};

use crate::data::{Category, Data, Eldritch, Influence, Rarity};

fn one() -> u8 {
    1
}

/// One crafting step. A two-input method reads `inputs` in the order the game asks for them: an
/// Awakener's Orb takes the item whose influence it takes first and the item it reforges second;
/// a recombinator takes its two items in either order.
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
    /// How the method treats the metamods that protect modifiers. Required exactly when an op
    /// removes or changes existing explicits (`Op::changes_explicits`), or to forbid the method
    /// on an item that carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protect: Option<Protection>,
    pub ops: Vec<Op>,
    #[serde(default)]
    pub claims: Vec<String>,
    #[serde(default)]
    pub game_text: Option<String>,
    /// The fixture the catalog demonstrates the method on.
    #[serde(default)]
    pub example: Option<String>,
    /// Fossil family only: the resonator currency ids for one, two, three and four fossils.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resonators: Vec<String>,
}

/// A currency name, or `beast:<name>`; `apply::cost` resolves it to a `Cost`.
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

/// How a method treats one kind of protecting metamod. The values come from Craft of Exile's
/// metamod table (https://www.craftofexile.com/metamods), checked against maxroll's
/// metacrafting guide and vhpg's Prefixes Cannot Be Changed table.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Policy {
    /// What the metamod protects stays as it is.
    Respect,
    /// The method changes it as if the metamod were not there.
    Ignore,
    /// The method cannot be used while the metamod is on the item.
    Forbid,
}

/// A method's treatment of the protecting metamods; `apply` decides in one place, from this,
/// whether an affix can change. Can Have Up To 3 Crafted Modifiers protects nothing and forbids
/// nothing (owner, Q9).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Protection {
    /// Prefixes Cannot Be Changed and Suffixes Cannot Be Changed: respected, every modifier on
    /// the side they lock stays.
    pub locks: Policy,
    /// Cannot Roll Attack Modifiers and Cannot Roll Caster Modifiers: respected, a modifier with
    /// the tag they name is never removed or changed. Every op that adds a random modifier leaves
    /// that tag out whatever the policy (claim `metamod.cannot-roll`).
    pub cannot_roll: Policy,
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
    /// A mirrored item (Reflective Oil).
    Mirrored,
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
    /// A tag the base itself carries (a Talisman base).
    BaseTag {
        tag: String,
    },
    /// A tag the base must not carry: 3.29 Talismans cannot be anointed (claim
    /// `anoint.talismans`).
    NotBaseTag {
        tag: String,
    },
    /// The item's tag set, domain, item level and open slots leave at least one candidate in
    /// `pool`. A qualified reforge still refuses when no guaranteed modifier can roll at all.
    CanRoll {
        pool: Pool,
    },
    /// The item holds at least `n` sockets (a link craft of `n` sockets).
    MinSockets {
        n: u8,
    },
    EssenceFits {
        essence: String,
    },
    Fossils {
        min: u8,
        max: u8,
    },
    /// The selected fossils' own usage limits (claim `fossil.fractured-limits`): a fossil whose
    /// text fractures a modifier refuses influenced and fractured items.
    FossilLimits,
    ImplicitsChangeable,
}

#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "kebab-case")]
pub enum SideReq {
    #[default]
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

/// One step of what a method does; `apply` runs them in order over a distribution of items. An
/// op with nothing to do outside a `Chance` branch refuses the method; inside one it leaves that
/// branch's item unchanged.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    SetRarity {
        rarity: Rarity,
    },
    /// Removes every explicit on `side` that can change under the method's protection; the
    /// rarity stays (an Orb of Alteration, the clear before a reforge).
    Clear {
        #[serde(default)]
        side: SideReq,
    },
    /// An Orb of Scouring: removes every explicit that can change. With nothing kept the item
    /// turns normal; with only fractured modifiers left it turns magic (one) or rare (two or
    /// more); a side a metamod kept leaves the rarity as it was.
    Scour,
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
        /// With nothing it can remove, remove nothing and carry on instead of refusing the
        /// method (the Farric beastcrafts still add, claim `beast.farric`).
        #[serde(default)]
        optional: bool,
    },
    AddVeiled,
    Unveil,
    AddMod {
        r#mod: String,
    },
    /// The essence's modifier for the item's class.
    AddEssence {
        essence: String,
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
    /// Adds every influence input `input` carries.
    AddInfluences {
        input: u8,
    },
    /// Carries on with input `input` instead of the first one.
    TakeInput {
        input: u8,
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
    Imprint,
    Restore,
    LinkAll,
    Mirror,
    Destroy,
    RemoveIntangibility,
    Chance {
        branches: Vec<Branch>,
    },
    /// The rest of a reforge: its random modifiers. Not run in this pass; `apply` stops here.
    Fill {
        fill: Fill,
    },
    Sockets {
        change: SocketChange,
    },
    Unmodelled {
        reason: PendingReason,
    },
}

impl Op {
    /// Whether the op removes or changes explicits the item already has; a method with such an
    /// op must state its `Protection`.
    pub fn changes_explicits(&self) -> bool {
        matches!(
            self,
            Op::Clear { .. }
                | Op::Scour
                | Op::RemoveRandom { .. }
                | Op::RerollValues {
                    scope: ValueScope::Explicit
                }
                | Op::Dominance
                | Op::ChangeResistance { .. }
        )
    }
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
    /// Natural modifiers of one influence.
    Influence {
        influence: Influence,
    },
    /// Natural modifiers of any influence the item has.
    Influenced,
    Tag {
        tag: String,
    },
    Ducat {
        tag: String,
    },
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

/// The random rest of a reforge, carried for the pass that enumerates it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fill {
    /// (explicit total the reforge ends with, relative weight), by total: `[[4, 8], [5, 3],
    /// [6, 1]]`, which datagen checks against claim `chaos.count`. The counts are what the
    /// reforge *adds*: modifiers a metamod keeps are not part of them, and the additions are
    /// limited by the open slots (claim `reforge.counts`, owner Q7). Empty when the count rule
    /// is not known, or when the weighting brings its own (a recombine).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counts: Vec<(u8, u32)>,
    #[serde(default)]
    pub weighting: Weighting,
    /// The sides it draws on.
    #[serde(default)]
    pub side: SideReq,
    /// The highest modifier level it draws: an essence's "Properties restricted to level N and
    /// below" (claim `essence.ilvl`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_level_max: Option<u8>,
    /// The drawn set must hold at least one modifier from this pool: a Harvest reforge with a
    /// tag repeats until it rolls at least one modifier carrying the tag (claim
    /// `reforge.counts`, owner Q7). Not a guaranteed first draw, and no fixed placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<Pool>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Weighting {
    #[default]
    Normal,
    /// The fossils in `apply::Params::fossils`.
    Fossils,
    /// More likely to roll the modifiers the first input had.
    MoreLikelySame,
    LessLikelySame,
    /// Drawn from both inputs' modifiers (the `recombine.*` claims).
    Recombine,
}

/// What a socket op does; a bench option's count and colours are part of it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum SocketChange {
    RandomCount,
    Count {
        n: u8,
    },
    RandomLinks,
    Links {
        n: u8,
    },
    RandomColours,
    /// The bench's colour spec. RePoE writes the colours it needs (`"RRG"`); since 3.29 the only
    /// colour options are the non-white ones (`"NN"`, `"NNN"`, `"NNNN"`), where `N` means "not
    /// white", which `Socket` has no value for; every socket op returns `Pending` until the
    /// socket pass adds one.
    Colours {
        spec: String,
    },
    LinkUpToFour,
    TaintedCount,
    TaintedLinks,
    TaintedColours,
    /// The 3.29 corruption outcome: a random white socket turns non-white.
    Dewhiten,
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
    NeedsData {
        what: String,
    },
    /// The rule is known but not modelled yet.
    NotModelled {
        why: String,
    },
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
            PendingReason::NotModelled { why } => format!("not modelled yet: {why}"),
            PendingReason::TooLarge => format!("more than {} outcomes", crate::apply::MAX_BRANCHES),
        }
    }
}

/// Calls `f` on every op, Chance branches included, with whether more ops run after it.
fn walk<'a>(ops: &'a [Op], followed: bool, f: &mut dyn FnMut(&'a Op, bool)) {
    for (i, op) in ops.iter().enumerate() {
        let after = followed || i + 1 < ops.len();
        f(op, after);
        if let Op::Chance { branches } = op {
            for b in branches {
                walk(&b.ops, after, f);
            }
        }
    }
}

impl Method {
    /// Every op, Chance branches included.
    pub fn all_ops(&self) -> Vec<&Op> {
        let mut out = Vec::new();
        walk(&self.ops, false, &mut |op, _| out.push(op));
        out
    }

    /// Whether an op removes or changes explicits the item already has.
    pub fn changes_explicits(&self) -> bool {
        self.all_ops().iter().any(|op| op.changes_explicits())
    }

    /// Whether an op adds past the affix limits (Merrick's Ducat); its results may hold more
    /// prefixes or suffixes than the item's limits (`Item::validate_over_limit`).
    pub fn ignores_limits(&self) -> bool {
        self.all_ops().iter().any(|op| {
            matches!(
                op,
                Op::AddRandom {
                    ignore_limits: true,
                    ..
                }
            )
        })
    }

    /// Every id the method names resolves against the bundle, and its shape is one `apply` can
    /// run: Chance masses sum to 1, Fill tables are well formed, nothing follows a Mirror or a
    /// Destroy, input numbers exist, and the protection is stated where an op needs it.
    pub fn validate(&self, data: &Data) -> Result<(), String> {
        if !(1..=2).contains(&self.inputs) {
            return Err(format!("takes {} inputs; methods take 1 or 2", self.inputs));
        }
        if !self.pair.is_empty() && self.inputs != 2 {
            return Err("pair rules need two inputs".into());
        }
        if self.ops.is_empty() {
            return Err("has no ops".into());
        }
        for c in &self.claims {
            if data.claim(c).is_none() {
                return Err(format!("unknown claim {c}"));
            }
        }
        for c in &self.cost {
            if c.amount.is_nan() || c.amount <= 0.0 {
                return Err(format!("cost {:?} has amount {}", c.name, c.amount));
            }
            if c.name.starts_with("beast:") {
                continue;
            }
            match data.currencies.iter().filter(|x| x.name == c.name).count() {
                0 => return Err(format!("cost names unknown currency {:?}", c.name)),
                1 => {}
                n => {
                    return Err(format!(
                        "cost {:?} names {n} currencies; use one with a unique name",
                        c.name
                    ));
                }
            }
        }
        match (self.family == Family::Fossil, self.resonators.len()) {
            (true, 4) | (false, 0) => {}
            (true, _) => {
                return Err(
                    "a fossil method lists four resonators, for one to four fossils".into(),
                );
            }
            (false, _) => return Err("only a fossil method lists resonators".into()),
        }
        for r in &self.resonators {
            if data.currency_by_id(r).is_none() {
                return Err(format!("unknown resonator currency {r}"));
            }
        }
        match (self.changes_explicits(), self.protect) {
            (true, None) => {
                return Err("changes existing modifiers but states no protection policy".into());
            }
            (false, Some(p)) if p.locks != Policy::Forbid && p.cannot_roll != Policy::Forbid => {
                return Err("states a protection policy but changes no existing modifier".into());
            }
            _ => {}
        }
        for r in &self.requires {
            validate_require(data, r)?;
        }
        let mut problem = None;
        walk(&self.ops, false, &mut |op, followed| {
            if problem.is_none() {
                problem = validate_op(data, self, op, followed).err();
            }
        });
        problem.map_or(Ok(()), Err)
    }
}

fn known_mod(data: &Data, id: &str) -> Result<u32, String> {
    data.mod_by_id(id)
        .ok_or_else(|| format!("unknown mod {id}"))
}

fn validate_require(data: &Data, r: &Require) -> Result<(), String> {
    match r {
        Require::Rarity { any_of } if any_of.is_empty() => Err("empty rarity list".into()),
        Require::Category { any_of } if any_of.is_empty() => Err("empty category list".into()),
        Require::Class { any_of } => {
            if any_of.is_empty() {
                return Err("empty class list".into());
            }
            for c in any_of {
                if data.class_by_id(c).is_none() {
                    return Err(format!("unknown class {c}"));
                }
            }
            Ok(())
        }
        Require::GroupFree { r#mod } => known_mod(data, r#mod).map(|_| ()),
        Require::BaseTag { tag } | Require::NotBaseTag { tag }
            if data.tag_by_name(tag).is_none() =>
        {
            Err(format!("unknown tag {tag}"))
        }
        Require::CanRoll { pool } => match pool {
            Pool::Tag { tag } | Pool::Ducat { tag } if data.tag_by_name(tag).is_none() => {
                Err(format!("unknown tag {tag}"))
            }
            _ => Ok(()),
        },
        Require::HasModType { any_of } => {
            for t in any_of {
                if data.mod_type_by_name(t).is_none() {
                    return Err(format!("unknown mod type {t}"));
                }
            }
            Ok(())
        }
        Require::CatalystRoom { catalyst } if data.catalyst_by_name(catalyst).is_none() => {
            Err(format!("unknown catalyst {catalyst}"))
        }
        Require::EssenceFits { essence } if data.essence_by_name(essence).is_none() => {
            Err(format!("unknown essence {essence}"))
        }
        Require::Fossils { min, max } if !(1 <= *min && min <= max && *max <= 4) => {
            Err(format!("fossil count {min} to {max} is not within 1 to 4"))
        }
        _ => Ok(()),
    }
}

fn validate_op(data: &Data, m: &Method, op: &Op, followed: bool) -> Result<(), String> {
    match op {
        Op::Mirror | Op::Destroy if followed => {
            Err("an op follows a Mirror or a Destroy, which end the method".into())
        }
        Op::AddRandom { pool, .. } => match pool {
            Pool::Tag { tag } | Pool::Ducat { tag } if data.tag_by_name(tag).is_none() => {
                Err(format!("unknown tag {tag}"))
            }
            _ => Ok(()),
        },
        Op::AddMod { r#mod } => {
            let md = data.mod_at(known_mod(data, r#mod)?);
            if md.kind.is_explicit() && md.side.is_some() {
                Ok(())
            } else {
                Err(format!("{mod} is not a prefix or suffix", mod = r#mod))
            }
        }
        Op::SetEnchantMod { r#mod } => {
            if data.mod_at(known_mod(data, r#mod)?).kind.is_enchant() {
                Ok(())
            } else {
                Err(format!("{mod} is not an enchantment", mod = r#mod))
            }
        }
        Op::AddEssence { essence } => match data.essence_by_name(essence) {
            Some(e) if !data.essences[e as usize].mods.is_empty() => Ok(()),
            Some(_) => Err(format!("{essence} has no modifier for any class")),
            None => Err(format!("unknown essence {essence}")),
        },
        Op::AddVeiled
            if crate::apply::VEILED
                .iter()
                .any(|id| data.mod_by_id(id).is_none()) =>
        {
            Err("the bundle lacks a veiled placeholder modifier".into())
        }
        Op::CatalystQuality { catalyst } if data.catalyst_by_name(catalyst).is_none() => {
            Err(format!("unknown catalyst {catalyst}"))
        }
        Op::ChangeResistance { from, to } => {
            if from == to {
                return Err("a resistance change needs two elements".into());
            }
            for t in [from.resistance_type(), to.resistance_type()] {
                if data.mod_type_by_name(t).is_none() {
                    return Err(format!("unknown mod type {t}"));
                }
            }
            Ok(())
        }
        Op::AddInfluences { input } | Op::TakeInput { input } if *input >= m.inputs => Err(
            format!("names input {input} of a method that takes {}", m.inputs),
        ),
        Op::Chance { branches } => {
            if branches.is_empty() {
                return Err("a Chance op has no branches".into());
            }
            if branches
                .iter()
                .any(|b| b.p.is_nan() || b.p <= 0.0 || b.p > 1.0)
            {
                return Err("a Chance branch has a probability outside (0, 1]".into());
            }
            let total: f64 = branches.iter().map(|b| b.p).sum();
            if (total - 1.0).abs() > 1e-9 {
                return Err(format!("Chance branches sum to {total}, not 1"));
            }
            Ok(())
        }
        Op::Fill { fill } => {
            for &(n, w) in &fill.counts {
                if !(1..=6).contains(&n) || w == 0 {
                    return Err(format!(
                        "Fill count {n} with weight {w}: counts run 1 to 6 and weigh more than 0"
                    ));
                }
            }
            if fill.counts.windows(2).any(|w| w[0].0 >= w[1].0) {
                return Err("Fill counts must rise, each total once".into());
            }
            if fill.mod_level_max == Some(0) {
                return Err("Fill caps modifier level at 0".into());
            }
            if let Some(Pool::Tag { tag } | Pool::Ducat { tag }) = &fill.include
                && data.tag_by_name(tag).is_none()
            {
                return Err(format!("Fill include names unknown tag {tag}"));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
