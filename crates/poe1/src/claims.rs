//! Game rules as cited data (`rules/poe1-claims.json`, compiled in).
//!
//! Every rule a kernel reads is a claim: what it says, where it comes from, and whether the
//! evidence is weak enough to call the rule approximate. Rule constants in code carry a marker
//! line naming their claim; the tests below check that markers and claims agree.

use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Claim {
    pub id: String,
    /// The rule, in our own words.
    pub statement: String,
    /// One of `wiki`, `datamined`, `coe`, `guide`, `experiment`, `owner`.
    pub source_kind: String,
    /// The public source, or a repo-relative path for checks against our own pinned data.
    pub url: String,
    /// The source's own words, when we read it ourselves; else empty.
    pub quote: String,
    /// The patch the source describes.
    pub patch: String,
    /// Whether we opened the source ourselves.
    pub checked: bool,
    /// How the fact reached us.
    pub via: String,
    #[serde(default)]
    pub approximate: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub value: serde_json::Value,
}

#[derive(Deserialize)]
struct ClaimFile {
    claims: Vec<Claim>,
}

static CLAIMS: LazyLock<Vec<Claim>> = LazyLock::new(|| {
    serde_json::from_str::<ClaimFile>(include_str!("../../../rules/poe1-claims.json"))
        .expect("rules/poe1-claims.json parses")
        .claims
});

/// The compiled-in claims, in file order.
pub fn all() -> &'static [Claim] {
    &CLAIMS
}

/// The claim with this id. Panics on an unknown id: ids are compile-time knowledge.
pub fn claim(id: &str) -> &'static Claim {
    all()
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("unknown claim {id}"))
}

fn number(id: &str, v: &serde_json::Value) -> f64 {
    v.as_f64()
        .unwrap_or_else(|| panic!("claim {id}: {v} is not a number"))
}

/// `[(mod count, chance)]` of a full rare reforge, normalised.
pub fn chaos_count_weights() -> [(u8, f64); 3] {
    let c = claim("affix.chaos_count_ratio");
    let raw = [4u8, 5, 6].map(|k| (k, number(&c.id, &c.value[k.to_string()])));
    let total: f64 = raw.iter().map(|(_, w)| w).sum();
    raw.map(|(k, w)| (k, w / total))
}

/// `[(mod count, chance)]` of a Transmutation or Alteration, normalised.
pub fn magic_mod_count_weights() -> [(u8, f64); 2] {
    let c = claim("magic.mod_count");
    let raw = [1u8, 2].map(|k| (k, number(&c.id, &c.value[k.to_string()])));
    let total: f64 = raw.iter().map(|(_, w)| w).sum();
    raw.map(|(k, w)| (k, w / total))
}

/// A recombine count table: row = pooled count 1..=6 (clamped), column = mods kept 0..=3, each
/// row normalised to sum 1.
pub fn recombine_count_table(id: &str) -> [[f64; 4]; 6] {
    let c = claim(id);
    let rows = c
        .value
        .as_array()
        .filter(|r| r.len() == 6)
        .unwrap_or_else(|| panic!("claim {id}: value is not 6 rows"));
    let mut table = [[0.0; 4]; 6];
    for (row, raw) in table.iter_mut().zip(rows) {
        let cells = raw
            .as_array()
            .filter(|r| r.len() == 4)
            .unwrap_or_else(|| panic!("claim {id}: a row is not 4 cells"));
        for (cell, v) in row.iter_mut().zip(cells) {
            *cell = number(id, v);
        }
        let total: f64 = row.iter().sum();
        assert!(total > 0.0, "claim {id}: a row sums to 0");
        for cell in row.iter_mut() {
            *cell /= total;
        }
    }
    table
}

/// The recombine selection exponent: (plan, low, high).
pub fn recombine_alpha() -> (f64, f64, f64) {
    let c = claim("recombine.selection_alpha");
    let get = |k: &str| number(&c.id, &c.value[k]);
    (get("plan"), get("low"), get("high"))
}

/// The selection weight of a keepable pooled mod that has no spawn weight on the chosen base.
pub fn exclusive_weight() -> f64 {
    let c = claim("recombine.exclusive_weight");
    number(&c.id, &c.value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Code that may name a claim: by its quoted id (an accessor call) or by a marker line.
    const SOURCES: [&str; 8] = [
        include_str!("claims.rs"),
        include_str!("kernels.rs"),
        include_str!("registry.rs"),
        include_str!("weights.rs"),
        include_str!("market.rs"),
        include_str!("recipe.rs"),
        include_str!("api.rs"),
        include_str!("../../datagen/src/main.rs"),
    ];

    const MARKER: &str = "Claim: ";

    /// Words no claim may contain, as (byte length, FNV-1a 64 of the lowercase word), so this
    /// file never spells them.
    const FORBIDDEN: [(usize, u64); 4] = [
        (15, 0xd1e1829a628dfe57),
        (13, 0x0dba1f5f84e80439),
        (12, 0x1d131dc9f7d67b0d),
        (8, 0x8819d4330c1bdbf4),
    ];

    /// The sources with this test module cut off, so ids written here never count as uses.
    fn sources() -> Vec<&'static str> {
        SOURCES
            .iter()
            .map(|s| s.split("#[cfg(test)]").next().unwrap_or(s))
            .collect()
    }

    fn fnv1a(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    fn forbidden_in(text: &str) -> bool {
        let lower = text.to_lowercase();
        let bytes = lower.as_bytes();
        FORBIDDEN.iter().any(|(len, hash)| {
            bytes.len() >= *len && bytes.windows(*len).any(|w| fnv1a(w) == *hash)
        })
    }

    fn markers() -> Vec<String> {
        let mut out = Vec::new();
        for src in sources() {
            for (i, _) in src.match_indices(MARKER) {
                let id: String = src[i + MARKER.len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
                    .collect();
                out.push(id);
            }
        }
        out
    }

    #[test]
    fn claims_are_well_formed() {
        let kinds = ["wiki", "datamined", "coe", "guide", "experiment", "owner"];
        let mut ids: Vec<&str> = all().iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "claim ids are unique");
        for c in all() {
            assert!(
                kinds.contains(&c.source_kind.as_str()),
                "{}: unknown source_kind {}",
                c.id,
                c.source_kind
            );
            assert!(
                !c.url.is_empty() || c.source_kind == "owner",
                "{}: url is empty",
                c.id
            );
            assert!(!c.statement.is_empty(), "{}: statement is empty", c.id);
            assert!(!c.patch.is_empty(), "{}: patch is empty", c.id);
            if c.approximate {
                assert!(
                    c.reason.as_deref().is_some_and(|r| !r.is_empty()),
                    "{}: approximate without a reason",
                    c.id
                );
            }
        }
    }

    #[test]
    fn every_claim_is_used_and_every_marker_is_a_claim() {
        let srcs = sources();
        for c in all() {
            let quoted = format!("\"{}\"", c.id);
            let marker = format!("{MARKER}{}", c.id);
            assert!(
                srcs.iter()
                    .any(|s| s.contains(&quoted) || s.contains(&marker)),
                "claim {} is not read or marked anywhere in code",
                c.id
            );
        }
        for id in markers() {
            assert!(
                all().iter().any(|c| c.id == id),
                "marker names an unknown claim {id:?}"
            );
        }
    }

    #[test]
    fn accessors_parse_their_values() {
        let total: f64 = chaos_count_weights().iter().map(|(_, w)| w).sum();
        assert!((total - 1.0).abs() < 1e-12);
        let total: f64 = magic_mod_count_weights().iter().map(|(_, w)| w).sum();
        assert!((total - 1.0).abs() < 1e-12);
        let (plan, low, high) = recombine_alpha();
        assert!(
            0.0 <= low && low <= plan && plan <= high,
            "{low} {plan} {high}"
        );
        assert!(exclusive_weight() > 0.0);
        let tables: Vec<&Claim> = all()
            .iter()
            .filter(|c| c.value.as_array().is_some_and(|r| r.len() == 6))
            .collect();
        assert!(!tables.is_empty());
        for c in tables {
            for row in recombine_count_table(&c.id) {
                assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-12, "{}", c.id);
            }
        }
    }

    #[test]
    fn claims_carry_no_attribution() {
        // FNV-1a 64 test vector, so the hash the forbidden list relies on is the published one.
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
        for c in all() {
            let mut fields = vec![
                c.statement.as_str(),
                c.via.as_str(),
                c.url.as_str(),
                c.quote.as_str(),
            ];
            if let Some(r) = &c.reason {
                fields.push(r);
            }
            for f in fields {
                assert!(!forbidden_in(f), "{}: a field names a private source", c.id);
            }
        }
    }
}
