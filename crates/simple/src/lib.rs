//! A Path of Exile crafting model for one base: the Shaper Supreme Spiked Shield (`Totem Shield`).
//!
//! The crate holds the shield's 204 rollable modifiers and its base (`data`), exact chances
//! (`fraction`), a stat's value as an interval (`roll`), the item a craft carries (`item`: base,
//! item level, rarity, influence, quality, flags, implicits and the modifiers with their values),
//! the draw rule (`pool`), what a currency can be used on (`stock`), what one use costs (`cost`),
//! the eight crafting orbs (`currency`), and the goal (`filter`, `target`).
//!
//! `data/shield.json` was generated from the pinned 3.29 export by `data/gen_shield.py`; the export
//! copy on `main` only carries the toy bases, so the generator was run against the copy that has the
//! shield. The generated file is compiled in, so nothing here reads the export.

pub mod cost;
pub mod currency;
pub mod data;
pub mod filter;
pub mod fraction;
pub mod item;
pub mod pool;
pub mod roll;
pub mod stock;
pub mod target;
