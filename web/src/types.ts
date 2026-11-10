// The JSON the worker answers with, mirroring `crates/poe1/src/api.rs` field for field.

export type Rarity = 'normal' | 'magic' | 'rare' | 'unique'

/** Influence names as the engine writes them in a spec. */
export const INFLUENCES = ['shaper', 'elder', 'crusader', 'redeemer', 'hunter', 'warlord'] as const

/** A tracked modifier: its tier, or its family at that tier or higher. */
export interface SlotSpec {
  id: string
  orBetter?: boolean
}

/**
 * An abstract item (`AbsSpec`): the slots the chart tracks and how many untracked modifiers sit
 * on each side. The engine leaves out empty lists, zero counts and false flags when it writes
 * one, so a spec read from it goes through `normalizeSpec` first.
 */
export interface AbsSpec {
  base: string
  itemLevel: number
  rarity: Rarity
  influences: string[]
  prefixes: SlotSpec[]
  suffixes: SlotSpec[]
  anyPrefixes: number
  anySuffixes: number
  corrupted: boolean
}

/** A class as the page shows it: names and modifier texts. */
export interface ClassView {
  base: string
  itemLevel: number
  rarity: string
  corrupted: boolean
  influences: string[]
  /** The tracked modifiers, each `"… or better"` when it stands for its family. */
  prefixes: string[]
  suffixes: string[]
  anyPrefixes: number
  anySuffixes: number
}

/** A chart node: its class, and what it stands for. */
export interface NodeView {
  id: string
  spec: AbsSpec
  view: ClassView
  /** How many items the node stands for, when it has any of its own. */
  members: number | null
  approx: boolean
  /** The forward step whose outcome gave the node its items: the node it was used on, and the method. */
  source: { from: string; method: string } | null
}

export type OutcomeKind = 'item' | 'destroyed' | 'refused' | 'notModelled'

/** One outcome of a method applied to a node. */
export interface OutcomeView {
  key: string
  kind: OutcomeKind
  p: number
  /** The 95% range when `p` comes from a simulation. */
  interval: [number, number] | null
  view: ClassView | null
  reason: string | null
}

/** The chance a step's result holds a watched modifier, or ("at least one of these") any of them. */
export interface WatchView {
  label: string
  p: number
  /** The 95% range when `p` comes from a simulation. */
  interval: [number, number] | null
}

/** One method's outcomes from a node. */
export interface AppliedView {
  node: string
  method: string
  name: string
  cost: string
  /** The reforges run, when the numbers are simulated. */
  simulated: number | null
  /** The method can be used again on each result with the same odds, so a count of tries means something. */
  repeatable: boolean
  outcomes: OutcomeView[]
  /** Each watched modifier's chance, then the chance of at least one when two or more are watched. */
  watched: WatchView[]
}

/** One method a node can be pushed through. */
export interface MethodOption {
  id: string
  name: string
  family: string
  cost: string
}

/** One input a method could have turned into a node. */
export interface InputView {
  key: string
  method: string
  name: string
  p: number
  interval: [number, number] | null
  cost: string
  view: ClassView
  note: string | null
  /** The method can be used again on the node it made with the same odds. */
  repeatable: boolean
}

/** A method that shows no input, and why. */
export interface UnmetView {
  method: string
  name: string
  reason: string
}

/** The inputs that could have produced a node. */
export interface MadeByView {
  node: string
  inputs: InputView[]
  skipped: UnmetView[]
}

/** A pinned step. Edges always run in crafting order. */
export interface EdgeView {
  from: string
  to: string
  method: string
  name: string
  p: number
  interval: [number, number] | null
  /** The method can be used again on the step's result with the same odds. */
  repeatable: boolean
}

/** A node added to the chart, with the edge that added it. */
export interface PinnedView {
  node: NodeView
  edge: EdgeView
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
    prefixes?: { id: string }[]
    suffixes?: { id: string }[]
    corrupted?: boolean
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

/** A pinned step as it is saved and replayed. */
export interface Pin {
  /** `f`: a forward step from `node`; `b`: a backward input above `node`. */
  dir: 'f' | 'b'
  node: string
  method: string
  key: string
}

/** A card folded into another: `from` is shown as `into` from then on. */
export interface Merge {
  from: string
  into: string
}

const RARITIES: readonly string[] = ['normal', 'magic', 'rare', 'unique']

function isRarity(raw: unknown): raw is Rarity {
  return typeof raw === 'string' && RARITIES.includes(raw)
}

function slots(raw: unknown): SlotSpec[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((entry: unknown) => {
    if (!entry || typeof entry !== 'object' || !('id' in entry) || typeof entry.id !== 'string') return []
    return 'orBetter' in entry && entry.orBetter === true
      ? [{ id: entry.id, orBetter: true }]
      : [{ id: entry.id }]
  })
}

function count(raw: unknown): number {
  return typeof raw === 'number' && Number.isInteger(raw) && raw >= 0 ? raw : 0
}

/** A spec with every field present, or `null` when `raw` is not one. */
export function normalizeSpec(raw: unknown): AbsSpec | null {
  if (!raw || typeof raw !== 'object') return null
  const value: Record<string, unknown> = { ...raw }
  if (typeof value.base !== 'string' || typeof value.itemLevel !== 'number') return null
  return {
    base: value.base,
    itemLevel: value.itemLevel,
    rarity: isRarity(value.rarity) ? value.rarity : 'rare',
    influences: Array.isArray(value.influences)
      ? value.influences.filter((i): i is string => typeof i === 'string')
      : [],
    prefixes: slots(value.prefixes),
    suffixes: slots(value.suffixes),
    anyPrefixes: count(value.anyPrefixes),
    anySuffixes: count(value.anySuffixes),
    corrupted: value.corrupted === true,
  }
}
