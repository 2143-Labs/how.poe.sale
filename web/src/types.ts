// The JSON the worker answers with, mirroring `crates/poe1/src/api.rs` field for field.

export type Rarity = 'normal' | 'magic' | 'rare' | 'unique'

/** Influence names as the engine writes them in a spec. */
export const INFLUENCES = ['shaper', 'elder', 'crusader', 'redeemer', 'hunter', 'warlord'] as const

/** A tracked modifier by id; `values` only when a paste rolled it. */
export interface ModSpec {
  id: string
  values?: number[]
}

/**
 * An abstract item (`AbsSpec`): the tracked modifiers by id and how many untracked ones sit on
 * each side. The engine leaves out empty lists and zero counts when it writes one, so a spec
 * read from it goes through `normalizeSpec` first.
 */
export interface AbsSpec {
  base: string
  itemLevel: number
  rarity: Rarity
  influences: string[]
  prefixes: ModSpec[]
  suffixes: ModSpec[]
  anyPrefixes: number
  anySuffixes: number
}

/** An abstract item as the page shows it: names and modifier texts. */
export interface AbsView {
  base: string
  itemLevel: number
  rarity: string
  influences: string[]
  prefixes: string[]
  suffixes: string[]
  anyPrefixes: number
  anySuffixes: number
}

/** One input that could have produced the item. */
export interface PreimageView {
  method: string
  name: string
  /** The chance the method turns `input` into the item, conditional on `input`. */
  p: number
  cost: string
  input: AbsView
  /** `input` as a spec, in the engine's compact form. */
  inputSpec: Partial<AbsSpec> & { base: string; itemLevel: number; rarity: Rarity }
  note: string | null
}

/** A method that shows no input, and why. */
export interface UnmetView {
  method: string
  name: string
  reason: string
}

export interface PreimagesView {
  target: AbsView
  steps: PreimageView[]
  skipped: UnmetView[]
}

/** A modifier a target can name. */
export interface ModOption {
  id: string
  text: string
  side: 'prefix' | 'suffix'
  level: number
  influence: string | null
  /** `natural`, or the essence that adds it. */
  source: string
}

/** A pasted item as the engine read it. */
export interface ItemView {
  /** The item in the engine's own form (`ItemSpec`). */
  item: {
    base: string
    itemLevel: number
    rarity: Rarity
    influences?: string[]
    prefixes?: ModSpec[]
    suffixes?: ModSpec[]
  } & Record<string, unknown>
  /** The item printed back as game text. */
  text: string
  /** Every line the reader could not explain. */
  warnings: string[]
}

/** An example paste. */
export interface Example {
  id: string
  label: string
  text: string
}

/** What the worker answers `start` with. */
export interface StartInfo {
  version: string
  examples: Example[]
  bases: string[]
}

/** One item on the chart. */
export interface ChartNode {
  id: string
  view: AbsView
  spec: AbsSpec
}

/** A pinned step: `from` is the input, `to` the item it produced. */
export interface ChartEdge {
  from: string
  to: string
  method: string
  name: string
  p: number
}

const RARITIES: readonly string[] = ['normal', 'magic', 'rare', 'unique']

function isRarity(raw: unknown): raw is Rarity {
  return typeof raw === 'string' && RARITIES.includes(raw)
}

function mods(raw: unknown): ModSpec[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((entry: unknown) => {
    if (!entry || typeof entry !== 'object' || !('id' in entry) || typeof entry.id !== 'string') return []
    const values = 'values' in entry && Array.isArray(entry.values) ? entry.values : null
    return values
      ? [{ id: entry.id, values: values.filter((v: unknown): v is number => typeof v === 'number') }]
      : [{ id: entry.id }]
  })
}

function count(raw: unknown): number {
  return typeof raw === 'number' && Number.isInteger(raw) && raw >= 0 ? raw : 0
}

/** A spec with every field present, or `null` when `raw` is not one. */
export function normalizeSpec(raw: unknown): AbsSpec | null {
  if (!raw || typeof raw !== 'object') return null
  // Every field is read as `unknown` and checked before use.
  const value: Record<string, unknown> = { ...raw }
  if (typeof value.base !== 'string' || typeof value.itemLevel !== 'number') return null
  const rarity = isRarity(value.rarity) ? value.rarity : 'rare'
  return {
    base: value.base,
    itemLevel: value.itemLevel,
    rarity,
    influences: Array.isArray(value.influences)
      ? value.influences.filter((i): i is string => typeof i === 'string')
      : [],
    prefixes: mods(value.prefixes),
    suffixes: mods(value.suffixes),
    anyPrefixes: count(value.anyPrefixes),
    anySuffixes: count(value.anySuffixes),
  }
}

/**
 * A node's identity: the same item pinned twice is one node. Modifier order, values and
 * influence order do not count.
 */
export function nodeId(spec: AbsSpec): string {
  return [
    spec.base,
    spec.itemLevel,
    spec.rarity,
    [...spec.influences].sort().join('+'),
    spec.prefixes.map((m) => m.id).sort().join(','),
    spec.suffixes.map((m) => m.id).sort().join(','),
    spec.anyPrefixes,
    spec.anySuffixes,
  ].join('|')
}
