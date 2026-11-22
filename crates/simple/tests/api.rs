//! The API the Craft planner page calls, read as the page reads it: JSON in, JSON out.
//!
//! Every built-in craft is run with the built-in filters, the 3.30 prices and the whole built-in
//! library, and must give the numbers its spike pinned (the slam's are at the built-in prices; see
//! `ai_research/craft-planner-plan.md`, Verification).

use serde_json::{Value, json};
use simple::api;
use simple::craft::spec::{self, CraftSpec};

fn catalog() -> Value {
    serde_json::to_value(api::catalog().expect("the catalog builds")).expect("it serializes")
}

/// The entry with this id in a list of presets.
fn by_id(list: &Value, id: &str) -> Value {
    list.as_array()
        .expect("a list")
        .iter()
        .find(|entry| entry["id"] == id)
        .unwrap_or_else(|| panic!("{id} is not in the list"))
        .clone()
}

fn builtin_craft(id: &str) -> Value {
    by_id(&catalog()["builtins"]["crafts"], id)
}

/// A run request for this craft under its own prices, with the built-in filters and library.
fn request(craft: Value) -> Value {
    let catalog = catalog();
    let builtins = &catalog["builtins"];
    json!({
        "filters": builtins["filters"],
        "prices": by_id(&builtins["prices"], craft["prices"].as_str().expect("a prices id")),
        "craft": craft,
        "library": builtins["crafts"],
    })
}

/// The request's filter with this id, to edit.
fn filter_mut<'a>(request: &'a mut Value, id: &str) -> &'a mut Value {
    request["filters"]
        .as_array_mut()
        .expect("filters")
        .iter_mut()
        .find(|filter| filter["id"] == id)
        .unwrap_or_else(|| panic!("{id} is not in the request"))
}

/// The request's goal filter, to edit.
fn goal_mut(request: &mut Value) -> &mut Value {
    let id = request["craft"]["goal"]
        .as_str()
        .expect("a goal id")
        .to_string();
    filter_mut(request, &id)
}

fn run(request: &Value) -> Result<Value, String> {
    api::run(&request.to_string())
        .map(|view| serde_json::to_value(view).expect("the view serializes"))
}

fn run_builtin(id: &str) -> Value {
    run(&request(builtin_craft(id))).unwrap_or_else(|why| panic!("{id}: {why}"))
}

fn error(request: &Value) -> String {
    match run(request) {
        Ok(_) => panic!("the run should fail"),
        Err(why) => why,
    }
}

/// `got` within `1e-7` of `want`, relative.
fn close(got: &Value, want: f64, what: &str) {
    let got = got
        .as_f64()
        .unwrap_or_else(|| panic!("{what}: {got} is not a number"));
    let tolerance = 1e-7 * want.abs().max(1.0);
    assert!(
        (got - want).abs() <= tolerance,
        "{what}: got {got}, want {want}"
    );
}

/// The mean count of one orb in a spend view; zero when it has no row.
fn orb(spend: &Value, id: &str) -> Option<Value> {
    spend["orbs"]
        .as_array()
        .expect("orbs")
        .iter()
        .find(|row| row["id"] == id)
        .map(|row| row["mean"].clone())
}

fn unweighted(spend: &Value, id: &str) -> Value {
    spend["unweighted"]
        .as_array()
        .expect("unweighted")
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("{id} is not unweighted"))["count"]
        .clone()
}

#[test]
fn the_catalog_lists_six_bases_ten_methods_and_nine_crafts() {
    let catalog = catalog();
    assert_eq!(catalog["bases"].as_array().unwrap().len(), 6);
    assert_eq!(catalog["methods"].as_array().unwrap().len(), 10);
    assert_eq!(catalog["currencies"].as_array().unwrap().len(), 11);
    assert_eq!(catalog["builtins"]["crafts"].as_array().unwrap().len(), 9);
    assert_eq!(catalog["builtins"]["filters"].as_array().unwrap().len(), 9);
    assert_eq!(catalog["builtins"]["prices"].as_array().unwrap().len(), 2);
    assert_eq!(catalog["grades"], json!(["desired", "good", "any"]));
}

#[test]
fn the_shield_totem_craft_costs_this() {
    let view = run_builtin("builtin:craft:shield-totem");
    let total = &view["total"];
    for (id, want) in [
        ("currency/transmutation", 51.804733254),
        ("currency/augmentation", 25.742475475),
        ("currency/regal", 51.165168646),
        ("currency/scouring", 50.804733254),
    ] {
        close(&orb(total, id).expect("a row"), want, id);
    }
    assert_eq!(orb(total, "currency/alteration"), None);
    assert_eq!(orb(total, "currency/exalted"), None);
    close(&total["value"], 45.668445135, "the chaos");
    let outputs = view["outputs"].as_array().unwrap();
    for (exit, want) in [("magic", 0.1599), ("magic_pair", 0.4797), ("rare", 0.3604)] {
        let share = outputs
            .iter()
            .find(|output| output["exit"] == exit)
            .unwrap_or_else(|| panic!("no output {exit}"))["share"]
            .as_f64()
            .unwrap();
        assert!((share - want).abs() <= 5e-5, "{exit}: {share}");
    }
    assert_eq!(view["checked"]["skipped"], 0);
}

#[test]
fn the_shield_slam_costs_this() {
    let total = &run_builtin("builtin:craft:shield-slam")["total"];
    close(&total["value"], 185.245359110, "the chaos");
    close(&total["sd"], 93.926720101, "its spread");
}

#[test]
fn the_flask_costs_this() {
    let total = &run_builtin("builtin:craft:flask")["total"];
    close(
        &orb(total, "currency/alteration").unwrap(),
        156.7,
        "alteration",
    );
    close(
        &orb(total, "currency/augmentation").unwrap(),
        7.0875,
        "augmentation",
    );
    close(&total["value"], 14.7555625, "the chaos");
}

#[test]
fn the_hunter_boots_cost_this() {
    let total = &run_builtin("builtin:craft:boots-hunter")["total"];
    close(&total["value"], 66.62875, "the chaos");
    close(&total["sd"], 66.43176194929275, "its spread");
    close(&unweighted(total, "currency/alchemy"), 4.0, "alchemy");
}

#[test]
fn the_redeemer_boots_cost_this_and_nest_exactly() {
    let view = run_builtin("builtin:craft:boots-redeemer");
    close(&view["total"]["value"], 28.5512854469249, "the chaos");
    close(&view["total"]["sd"], 26.0253700963052, "its spread");
    let steps = view["steps"].as_array().unwrap();
    let names: Vec<&str> = steps.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["alt_to_mod", "exalt_full"]);
    let blocks: Vec<&str> = steps.iter().map(|s| s["block"].as_str().unwrap()).collect();
    assert_eq!(blocks, ["b1/b1", "b2"]);

    let mut inlined = builtin_craft("builtin:craft:boots-redeemer");
    inlined["id"] = json!("user:craft:inlined");
    inlined["blocks"][0] = json!({
        "id": "b1",
        "method": { "kind": "alt_to_mod", "augment": false },
    });
    let flat = run(&request(inlined)).expect("the inlined craft runs");
    assert_eq!(flat["total"]["value"], view["total"]["value"]);
    assert_eq!(flat["total"]["sd"], view["total"]["sd"]);
}

#[test]
fn the_jewel_costs_this_and_warns_of_its_shared_family() {
    let view = run_builtin("builtin:craft:jewel");
    close(&view["total"]["value"], 199.47692214285712, "the chaos");
    close(&view["total"]["sd"], 198.4215372744611, "its spread");
    let warnings = view["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].as_str().unwrap().contains("DamageForm"));
}

#[test]
fn the_crown_spends_only_essences() {
    let view = run_builtin("builtin:craft:crown");
    close(&view["total"]["value"], 0.0, "the chaos");
    close(
        &unweighted(&view["total"], "essence/essence-of-horror"),
        34.9927875988717,
        "essences",
    );
    assert!(view["checked"]["skipped"].as_u64().unwrap() > 0);
}

#[test]
fn every_step_has_a_diagram() {
    let view = run_builtin("builtin:craft:boots-hunter");
    let diagrams = view["diagrams"].as_array().unwrap();
    assert_eq!(diagrams.len(), view["steps"].as_array().unwrap().len());
    for diagram in diagrams {
        assert!(!diagram["boxes"].as_array().unwrap().is_empty());
    }
}

#[test]
fn an_unwired_exit_is_named_by_its_block() {
    let mut craft = builtin_craft("builtin:craft:shield-totem");
    let wires: Vec<Value> = craft["wires"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|wire| wire["exit"] != "magic_pair")
        .cloned()
        .collect();
    craft["wires"] = json!(wires);
    assert_eq!(
        error(&request(craft)),
        r#"block "b1": exit "magic_pair" is not wired"#
    );
}

#[test]
fn an_unknown_method_is_refused() {
    let mut craft = builtin_craft("builtin:craft:flask");
    craft["blocks"][0]["method"] = json!({ "kind": "nope" });
    assert_eq!(
        error(&request(craft)),
        r#"block "b1" has an unknown method "nope""#
    );
}

#[test]
fn a_craft_that_contains_itself_is_refused() {
    let mut craft = builtin_craft("builtin:craft:flask");
    craft["id"] = json!("x");
    craft["blocks"][0] = json!({ "id": "b1", "craft": "x" });
    let mut request = request(craft.clone());
    request["library"].as_array_mut().unwrap().push(craft);
    assert_eq!(error(&request), r#"craft "x" contains itself"#);
}

#[test]
fn a_goal_must_name_the_bases_modifiers() {
    let mut request = request(builtin_craft("builtin:craft:shield-totem"));
    goal_mut(&mut request)["desired"] = json!(["NotAModifier"]);
    assert_eq!(
        error(&request),
        r#""NotAModifier" is not a modifier of Supreme Spiked Shield"#
    );
}

#[test]
fn a_goal_cannot_ask_for_more_than_a_rare_holds() {
    let mut request = request(builtin_craft("builtin:craft:jewel"));
    goal_mut(&mut request)["prefixes"] = json!(["desired", "good", "any", "any"]);
    assert_eq!(
        error(&request),
        "the prefix floors ask for 4 modifiers, and a rare Jewel holds 2"
    );
}

#[test]
fn a_wire_to_a_method_that_cannot_take_the_item_is_refused() {
    let mut craft = builtin_craft("builtin:craft:boots-redeemer");
    craft["blocks"] = json!([
        { "id": "b1", "method": { "kind": "alt_to_mod" } },
        { "id": "b2", "method": { "kind": "exalt_to", "modifiers": 4 } },
    ]);
    craft["wires"] = json!([
        { "from": "b1", "exit": "magic", "to": "b2" },
        { "from": "b2", "exit": "slammed", "to": "done" },
        { "from": "b2", "exit": "scrap", "to": "b1" },
    ]);
    assert_eq!(
        error(&request(craft)),
        "step 1 does not take a magic item, and step 0's exit `magic` hands it one"
    );
}

#[test]
fn an_unreachable_block_is_left_out_with_a_warning() {
    let mut craft = builtin_craft("builtin:craft:flask");
    craft["blocks"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "b9", "method": { "kind": "alt_pair" } }));
    let view = run(&request(craft)).expect("the craft still runs");
    close(&view["total"]["value"], 14.7555625, "the chaos");
    assert_eq!(
        view["warnings"],
        json!([r#"block "b9" is not reachable from the start and was left out"#])
    );
}

#[test]
fn the_shields_modifiers_are_its_explicit_ones() {
    let mods = serde_json::to_value(api::mods("shield").expect("the shield's modifiers")).unwrap();
    let mods = mods.as_array().unwrap();
    assert_eq!(mods.len(), 204);
    let totems = mods
        .iter()
        .find(|m| m["id"] == "MaximumTotemUber1")
        .expect("the totem modifier");
    assert_eq!(totems["weightPlain"], 0);
    assert_eq!(totems["weightInfluenced"], 800);
    assert_eq!(
        api::mods("nope").err(),
        Some(r#"unknown base "nope""#.to_string())
    );
}

#[test]
fn outputs_number_a_repeated_exit() {
    let craft: CraftSpec = serde_json::from_value(json!({
        "id": "user:craft:two",
        "name": "two rares",
        "goal": "builtin:goal:jewel",
        "prices": "builtin:prices:330",
        "start": "normal",
        "entry": "b1",
        "blocks": [
            { "id": "b1", "method": { "kind": "regal_to" } },
            { "id": "b2", "method": { "kind": "desired_pair" } },
        ],
        "wires": [
            { "from": "b1", "exit": "rare", "to": "done" },
            { "from": "b1", "exit": "normal", "to": "b2" },
            { "from": "b2", "exit": "rare", "to": "done" },
        ],
    }))
    .expect("the spec parses");
    assert_eq!(
        spec::outputs(&craft, &[]).expect("outputs"),
        ["rare", "rare#2"]
    );
}

#[test]
fn a_block_id_must_be_unique_and_not_done() {
    let mut craft = builtin_craft("builtin:craft:flask");
    craft["blocks"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "b1", "method": { "kind": "alt_pair" } }));
    assert_eq!(error(&request(craft)), r#"block "b1" is defined twice"#);

    let mut craft = builtin_craft("builtin:craft:flask");
    craft["blocks"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "done", "method": { "kind": "alt_pair" } }));
    assert_eq!(
        error(&request(craft)),
        r#"block "done": "done" names the craft's end"#
    );
}

#[test]
fn outputs_never_repeat_a_name() {
    let inner: CraftSpec = serde_json::from_value(json!({
        "id": "user:craft:inner", "name": "inner", "goal": "builtin:goal:jewel",
        "prices": "builtin:prices:330", "start": "normal", "entry": "z1",
        "blocks": [
            { "id": "z1", "method": { "kind": "regal_to" } },
            { "id": "z2", "method": { "kind": "desired_pair" } },
        ],
        "wires": [
            { "from": "z1", "exit": "rare", "to": "done" },
            { "from": "z1", "exit": "normal", "to": "z2" },
            { "from": "z2", "exit": "rare", "to": "done" },
        ],
    }))
    .unwrap();
    let outer: CraftSpec = serde_json::from_value(json!({
        "id": "user:craft:outer", "name": "outer", "goal": "builtin:goal:jewel",
        "prices": "builtin:prices:330", "start": "normal", "entry": "y1",
        "blocks": [
            { "id": "y1", "method": { "kind": "regal_to" } },
            { "id": "y2", "craft": "user:craft:inner" },
        ],
        "wires": [
            { "from": "y1", "exit": "rare", "to": "done" },
            { "from": "y1", "exit": "normal", "to": "y2" },
            { "from": "y2", "exit": "rare", "to": "done" },
            { "from": "y2", "exit": "rare#2", "to": "done" },
        ],
    }))
    .unwrap();
    let library = [inner, outer.clone()];
    assert_eq!(
        spec::outputs(&outer, &library).expect("outputs"),
        ["rare", "rare#2", "rare#2#2"]
    );
}

/// The crown's goal is read at the end: of the crowns the essence hands over, the share that are
/// the whole natural goal is the spike's pin.
#[test]
fn the_goal_share_is_the_finished_items_that_meet_the_goal() {
    let view = run_builtin("builtin:craft:crown");
    assert_eq!(view["goal"]["filter"], "builtin:goal:crown");
    close(
        &view["goal"]["share"],
        0.00141161680898771,
        "the natural goal",
    );
    let view = run_builtin("builtin:craft:boots-redeemer");
    close(
        &view["goal"]["share"],
        1.0,
        "every finished boot is the goal",
    );
}

/// The Redeemer boots with two filters: alteration reads Onslaught alone, the exalts read Onslaught
/// and movement speed. The first step is the one-filter alteration, to the digit; reading the goal
/// there instead changes the craft.
#[test]
fn a_block_reads_its_own_filter() {
    let view = run_builtin("builtin:craft:boots-redeemer-speed");
    close(&view["total"]["value"], 69.07008101489289, "total");
    close(&view["total"]["sd"], 66.64768520727074, "total sd");
    close(
        &view["goal"]["share"],
        1.0,
        "every finished boot is the goal",
    );
    assert_eq!(
        view["steps"][0]["filter"],
        "Two-Toned Boots (Redeemer): Onslaught"
    );
    assert_eq!(
        view["steps"][1]["filter"],
        "Two-Toned Boots (Redeemer): Onslaught and 30% movement speed"
    );
    let alone = run_builtin("builtin:craft:alt-until");
    close(
        &view["steps"][0]["reach"]["value"],
        alone["total"]["value"].as_f64().unwrap(),
        "alteration until Onslaught",
    );

    let mut craft = builtin_craft("builtin:craft:boots-redeemer-speed");
    craft["blocks"][0]["filter"] = Value::Null;
    let goal_only = run(&request(craft)).expect("the craft runs on its goal alone");
    let (two, one) = (
        view["total"]["value"].as_f64().unwrap(),
        goal_only["total"]["value"].as_f64().unwrap(),
    );
    assert!((two - one).abs() > 0.01 * two, "{two} against {one}");
}

/// A goal no block reads splits the classes finer and changes nothing the blocks see.
#[test]
fn a_goal_no_block_reads_leaves_the_cost_alone() {
    let mut craft = builtin_craft("builtin:craft:boots-redeemer");
    craft["goal"] = json!("builtin:filter:boots-redeemer-speed");
    craft["blocks"][1]["filter"] = json!("builtin:goal:boots-redeemer");
    let view = run(&request(craft)).expect("the craft runs");
    close(&view["total"]["value"], 28.5512854469249, "total");
    close(&view["total"]["sd"], 26.0253700963052, "total sd");
    let share = view["goal"]["share"].as_f64().unwrap();
    assert!(
        share > 0.0 && share < 1.0,
        "some finished boots have the speed: {share}"
    );
}

#[test]
fn a_blocks_filter_must_be_known_and_for_the_crafts_base() {
    let mut craft = builtin_craft("builtin:craft:boots-redeemer-speed");
    craft["blocks"][0]["filter"] = json!("user:filter:gone");
    assert_eq!(
        error(&request(craft)),
        r#"block "b1" uses an unknown filter "user:filter:gone""#
    );

    let mut craft = builtin_craft("builtin:craft:boots-redeemer-speed");
    craft["blocks"][0]["filter"] = json!("builtin:goal:shield");
    assert_eq!(
        error(&request(craft)),
        "filter \"Supreme Spiked Shield: the file's goal\" is for Supreme Spiked Shield, and the \
         craft is for Two-Toned Boots (Redeemer)"
    );

    let mut craft = builtin_craft("builtin:craft:boots-redeemer");
    craft["blocks"][0]["filter"] = json!("builtin:goal:boots-redeemer");
    assert_eq!(
        error(&request(craft)),
        r#"block "b1": a craft block uses its own filters"#
    );

    let mut craft = builtin_craft("builtin:craft:flask");
    craft["goal"] = json!("user:filter:gone");
    assert_eq!(
        error(&request(craft)),
        r#"filter "user:filter:gone" is not in the request"#
    );
}

/// A craft that can never finish names the item as its step's filter reads it.
#[test]
fn a_craft_that_cannot_finish_names_the_item_by_its_grades() {
    let mut request = request(builtin_craft("builtin:craft:jewel"));
    goal_mut(&mut request)["desired"] = json!(["PercentIncreasedLifeJewel"]);
    assert_eq!(
        error(&request),
        "desired_pair at normal - / -: no exit can be reached from here"
    );
}
