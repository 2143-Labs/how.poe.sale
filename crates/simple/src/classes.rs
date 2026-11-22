//! Classes: the modifiers split by how every filter of a run grades them.
//!
//! A [`Node`] records grades, not modifiers, so it can only answer the questions of the filter it
//! was graded by. A craft whose blocks read different filters needs a grading fine enough for all
//! of them: a modifier's [`Class`] is the tuple of its tiers under every filter, and two modifiers
//! share a class exactly when no filter tells them apart. The law, the nodes and the currencies run
//! over classes ([`crate::law::Law::classed`]); each block reads a node through its own filter's
//! column ([`Lens::view`]).
//!
//! This is as exact as the law over one filter's tiers: the law's derivation holds for any split of
//! the pool, and a block's move depends only on what its own filter sees. With one filter the
//! classes are its three tiers, in the order a law lists them — desired, good, any — so a run over
//! classes takes the same sums, in the same order, as the run over tiers.

use std::cmp::Reverse;
use std::fmt::Debug;
use std::hash::Hash;

use crate::data::Data;
use crate::filter::{ItemSetFilter, Tier};
use crate::node::Node;

/// What a node records about one held modifier: a filter's [`Tier`], or a run's [`Class`].
pub trait Grade: Copy + Eq + Hash + Debug + 'static {
    /// The grade as a node's label prints it.
    fn name(self) -> String;
}

impl Grade for Tier {
    fn name(self) -> String {
        Tier::name(self).to_string()
    }
}

/// One class of modifiers: those every filter of a run grades alike. The number is the class's
/// place in [`Classes::all`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Class(pub u8);

impl Grade for Class {
    fn name(self) -> String {
        format!("class {}", self.0)
    }
}

/// The most classes a run may have: a class is a `u8`.
const MAX_CLASSES: usize = u8::MAX as usize;

/// The modifiers of a data file split by a list of filters.
#[derive(Clone, Debug)]
pub struct Classes {
    /// Each modifier's class, by modifier id.
    of: Vec<Class>,
    /// Each class's tier under every filter, in filter order.
    tiers: Vec<Vec<Tier>>,
}

impl Classes {
    /// The classes of every modifier in the file under these filters.
    ///
    /// The classes are ordered by their tiers' ranks, highest first, the first filter deciding
    /// first; so one filter's classes are desired, good, any. The class no filter names — every
    /// tier [`Tier::Any`] — is always there, because the bench places a modifier outside the pool
    /// ([`Classes::unlisted`]).
    pub fn new(data: &Data, filters: &[&ItemSetFilter]) -> Result<Classes, String> {
        let tuple =
            |m: u16| -> Vec<Tier> { filters.iter().map(|filter| filter.tier_of(m)).collect() };
        let every: Vec<Vec<Tier>> = (0..data.mods.len() as u16).map(tuple).collect();
        let mut tiers: Vec<Vec<Tier>> = every.clone();
        tiers.push(vec![Tier::Any; filters.len()]);
        let rank = |tiers: &Vec<Tier>| -> Reverse<Vec<u8>> {
            Reverse(tiers.iter().map(|tier| tier.rank()).collect())
        };
        tiers.sort_by_key(rank);
        tiers.dedup();
        if tiers.len() > MAX_CLASSES {
            return Err(format!(
                "the filters split the modifiers into more than {MAX_CLASSES} classes"
            ));
        }
        let of = every
            .iter()
            .map(|tuple| {
                let at = tiers
                    .iter()
                    .position(|class| class == tuple)
                    .expect("every modifier's tiers are a class");
                Class(at as u8)
            })
            .collect();
        Ok(Classes { of, tiers })
    }

    /// The class of one modifier.
    pub fn of(&self, m: u16) -> Class {
        self.of[m as usize]
    }

    /// Every class, in order.
    pub fn all(&self) -> Vec<Class> {
        (0..self.tiers.len()).map(|at| Class(at as u8)).collect()
    }

    /// The class of a modifier no filter names.
    pub fn unlisted(&self) -> Class {
        let at = self
            .tiers
            .iter()
            .position(|tiers| tiers.iter().all(|tier| *tier == Tier::Any))
            .expect("the class no filter names is always kept");
        Class(at as u8)
    }

    /// How the filter at this place in the list sees each class.
    pub fn lens(&self, filter: usize) -> Lens {
        Lens {
            tiers: self.tiers.iter().map(|tiers| tiers[filter]).collect(),
        }
    }
}

/// One filter's column of a run's classes: what that filter sees in a node graded by class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lens {
    /// Each class's tier, by class.
    tiers: Vec<Tier>,
}

impl Lens {
    /// The tier this filter gives a class.
    pub fn tier(&self, class: Class) -> Tier {
        self.tiers[usize::from(class.0)]
    }

    /// The node as this filter grades it.
    pub fn view(&self, node: &Node<Class>) -> Node<Tier> {
        node.map(|class| self.tier(class))
    }
}
