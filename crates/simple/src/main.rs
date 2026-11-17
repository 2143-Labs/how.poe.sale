//! The shield's craft, as a report: what each step costs, with diagrams.
//!
//! Every number is computed from `data/shield.json` and `data/weights.json`; nothing here is
//! hard-coded. The output is Markdown with Mermaid diagrams, and `craft.md` beside this crate is
//! that output, pinned by `tests/report.rs`.

use std::process::ExitCode;

use simple::craft::shield::shield;
use simple::currency::ON_NODES;
use simple::data::data;
use simple::filter::ItemSetFilter;
use simple::item::{Influence, Item, Rarity};
use simple::law::Law;
use simple::method::Kit;
use simple::report::render;
use simple::target::Target;
use simple::weights::weights;

/// The item level the craft runs at: high enough for every modifier this base can roll.
const ITEM_LEVEL: u8 = 87;

fn main() -> ExitCode {
    let data = data();
    let start = Item::of_base(
        data,
        data.base_of("Supreme Spiked Shield"),
        ITEM_LEVEL,
        Rarity::Normal,
    )
    .with_influence(Influence::Shaper);
    let filter = ItemSetFilter::from_data(data);
    let target = Target::from_data(data);
    let law = match Law::new(data, &filter, &start) {
        Ok(law) => law,
        Err(why) => {
            eprintln!("{why}");
            return ExitCode::FAILURE;
        }
    };
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    let craft = shield();
    match craft.report(&kit, weights()) {
        Ok(report) => {
            print!("{}", render(data, &start, &craft, &kit, weights(), &report));
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}
