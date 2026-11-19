//! A Path of Exile crafting model for one base: the Shaper Supreme Spiked Shield (`Totem Shield`).
//!
//! The crate holds the shield's 204 rollable modifiers and its base (`data`), exact chances
//! (`fraction`), a stat's value as an interval (`roll`), the item a craft carries (`item`: base,
//! item level, rarity, influence, quality, flags, implicits and the modifiers with their values),
//! the draw rule (`pool`), what a currency can be used on (`stock`), what one use costs (`cost`),
//! the eight crafting orbs (`currency`), the goal (`filter`, `target`), the abstract item a
//! craft is planned on (`node`) with the law of its next modifier's grade (`law`), the solver that
//! carries chance, cost and spread through a loop (`chain`), the methods a craft is made of,
//! one file each (`method`), the craft that wires them and reports what each step costs (`craft`),
//! the rough worth of one orb (`weights`), and the craft printed as Markdown (`report`).
//!
//! `data/shield.json` was generated from the pinned 3.29 export by `data/gen_base.py`, given
//! `data/shield.spec.json`; the export copy on `main` only carries the toy bases, so the generator
//! was run against the full export. The generated file is compiled in, so nothing here reads the
//! export.

pub mod api;
pub mod bases;
pub mod chain;
pub mod cost;
pub mod craft;
pub mod currency;
pub mod data;
pub mod diagram;
pub mod filter;
pub mod fraction;
pub mod item;
pub mod law;
pub mod method;
pub mod node;
pub mod pool;
pub mod report;
pub mod roll;
pub mod stock;
pub mod target;
pub mod weights;
