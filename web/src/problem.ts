import type {
  AnyProblem,
  Donor,
  GoodJson,
  Problem,
  ProblemTarget,
  ProblemV3,
  ProcessJson,
  RecipeDef,
  Requirement,
  TargetDraft,
  TierJson,
} from './types'

/** Pinned to the repoe export the data bundle is built from (contract §3). */
export const BUNDLE_PATCH = '3.29.3.3'

export const DEFAULT_RUNS = 20000
export const DEFAULT_SEED = 7

/** The editable half of a market: everything the UI owns, without derived fields. */
export interface MarketState {
  goods: GoodJson[]
  processes: ProcessJson[]
  root: string | null
  focus: string | null
  prices: Record<string, number>
  donors: Donor[]
  authoredRecipes: RecipeDef[]
}

export function emptyMarketState(): MarketState {
  return { goods: [], processes: [], root: null, focus: null, prices: {}, donors: [], authoredRecipes: [] }
}

export function cloneGood(good: GoodJson): GoodJson {
  return { ...good, target: cloneTarget(good.target) }
}

export function cloneTarget(target: ProblemTarget): ProblemTarget {
  return {
    ...target,
    influences: [...target.influences],
    catalyst: target.catalyst ? { ...target.catalyst } : null,
    requirements: target.requirements.map((requirement) => ({
      ...requirement,
      accepted: [...requirement.accepted],
      sources: requirement.sources ? [...requirement.sources] : undefined,
    })),
  }
}

export function cloneProcess(process: ProcessJson): ProcessJson {
  return {
    ...process,
    kind: 'craft' in process.kind ? { craft: { ...process.kind.craft, allowed: [...process.kind.craft.allowed] } } : { recombine: { ...process.kind.recombine } },
    expanded: process.expanded ? [...process.expanded] : null,
    added: [...process.added],
    disabled: process.disabled.map(([key, action]) => [key, action] as [string, string]),
    buy: process.buy.map(([key, price]) => [key, price] as [string, number]),
    sell: process.sell.map(([key, price]) => [key, price] as [string, number]),
  }
}

/** Human name for a good created from an item target. */
export function goodName(target: ProblemTarget): string {
  const base = `${target.rarity} ${target.base}`
  return target.ilvlMin > 0 ? `${base} ilvl ${target.ilvlMin}` : base
}

/** The normal-rarity base item a recipe starts from (`input: "normal-base"`). */
export function baseGoodName(target: ProblemTarget): string {
  return `${target.base} ilvl ${target.ilvlMin} normal`
}

function priceEntries(prices: Record<string, number>): [string, number][] {
  return Object.keys(prices)
    .sort()
    .map((key) => [key, prices[key]] as [string, number])
}

/** A donor is only sent when it can be resolved by name; half-typed rows stay in the UI. */
function isCompleteDonor(donor: Donor): boolean {
  return (
    donor.label.trim() !== '' &&
    donor.base.trim() !== '' &&
    donor.mod.trim() !== '' &&
    donor.influence.trim() !== '' &&
    Number.isFinite(donor.priceChaos)
  )
}

/**
 * Pure `ProblemV3` builder: no worker, no wasm, no clamping. Prices are forwarded
 * exactly as the user typed them — validating them is the solver's job.
 */
export function buildProblem(state: MarketState): ProblemV3 | null {
  if (state.goods.length === 0 || state.root == null) return null
  return {
    version: 3,
    bundlePatch: BUNDLE_PATCH,
    goods: state.goods.map(cloneGood),
    processes: state.processes.map(cloneProcess),
    root: state.root,
    focus: state.focus,
    prices: { ...state.prices },
    donors: state.donors.filter(isCompleteDonor).map((donor) => ({ ...donor })),
    runs: DEFAULT_RUNS,
    seed: DEFAULT_SEED,
  }
}

/** Turn a shared problem (v1 or v3) back into the editable market state. */
export function stateFromProblem(problem: AnyProblem): MarketState {
  const upgraded = problem.version === 3 ? (problem as ProblemV3) : upgradeV1(problem as Problem)
  return {
    goods: upgraded.goods.map(cloneGood),
    processes: upgraded.processes.map(cloneProcess),
    root: upgraded.root,
    focus: upgraded.focus,
    prices: { ...(upgraded.prices ?? {}) },
    donors: Array.isArray(upgraded.donors) ? upgraded.donors.map((donor) => ({ ...donor })) : [],
    authoredRecipes: [],
  }
}

/**
 * A v1 problem is one target crafted by one tree. The goal prices move onto the
 * `main` good; every other state-keyed entry stays with the `p1` process, whose
 * keys are that same tree's keys.
 */
export function upgradeV1(problem: Problem): ProblemV3 {
  const buy = Object.fromEntries(problem.buy ?? [])
  const sell = Object.fromEntries(problem.sell ?? [])
  const { goal: buyPrice, ...stateBuy } = buy
  const { goal: sellPrice, ...stateSell } = sell
  const good: GoodJson = {
    id: 'main',
    name: goodName(problem.target),
    target: cloneTarget(problem.target),
    buy: buyPrice ?? null,
    sell: sellPrice ?? null,
  }
  const process: ProcessJson = {
    id: 'p1',
    output: 'main',
    recipe: null,
    kind: { craft: { from: null, allowed: [] } },
    expanded: Array.isArray(problem.expanded) ? [...problem.expanded] : null,
    added: Array.isArray(problem.added) ? [...problem.added] : [],
    disabled: Array.isArray(problem.disabled) ? problem.disabled.map(([k, a]) => [k, a] as [string, string]) : [],
    buy: priceEntries(stateBuy),
    sell: priceEntries(stateSell),
  }
  return {
    version: 3,
    bundlePatch: problem.bundlePatch ?? BUNDLE_PATCH,
    goods: [good],
    processes: [process],
    root: 'main',
    focus: 'p1',
    prices: { ...(problem.prices ?? {}) },
    donors: Array.isArray(problem.donors) ? problem.donors : [],
    runs: problem.runs ?? DEFAULT_RUNS,
    seed: problem.seed ?? DEFAULT_SEED,
  }
}

export type TierMode = 'any' | 'perfect'

/** The `minValue` a tier picker writes: the tier's floor for any roll, its ceiling for a perfect one. */
export function minValueFor(tier: Pick<TierJson, 'min' | 'max'>, mode: TierMode): number {
  return mode === 'perfect' ? tier.max : tier.min
}

/** Contract §3: `Problem.target` is the draft minus the parser-only fields. */
export function draftToTarget(draft: TargetDraft): ProblemTarget | null {
  if (!draft.base) return null
  return {
    base: draft.base,
    ilvlMin: draft.ilvl ?? 1,
    influences: [...draft.influences],
    rarity: draft.rarity,
    catalyst: draft.catalyst,
    minHits: draft.minHits,
    requirements: draft.requirements.map((requirement) => ({
      ...requirement,
      accepted: [...requirement.accepted],
      sources: requirement.sources ? [...requirement.sources] : undefined,
    })),
  }
}

/** The applied target as an editable draft, so its rows stay editable after a reload. */
export function targetToDraft(target: ProblemTarget): TargetDraft {
  return {
    itemClass: null,
    base: target.base,
    rarity: target.rarity,
    ilvl: target.ilvlMin,
    influences: [...target.influences],
    catalyst: target.catalyst,
    requirements: target.requirements.map((requirement) => ({
      ...requirement,
      accepted: [...requirement.accepted],
      sources: requirement.sources ? [...requirement.sources] : undefined,
    })),
    minHits: target.minHits,
    buyNote: null,
    unmatched: [],
    warnings: [],
  }
}

/** Key-order-independent identity of a target, to tell whether applying a draft changes anything. */
export function targetSignature(target: ProblemTarget | null): string {
  if (!target) return 'null'
  return JSON.stringify([
    target.base,
    target.ilvlMin,
    target.influences,
    target.rarity,
    target.catalyst ? [target.catalyst.name, target.catalyst.quality] : null,
    target.minHits,
    target.requirements.map((r) => [r.group, r.side, r.accepted, r.minValue, r.label]),
  ])
}

const RARITIES = ['Normal', 'Magic', 'Rare'] as const

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []
}

function normalizeRequirement(raw: unknown): Requirement | null {
  if (!raw || typeof raw !== 'object') return null
  const r = raw as Partial<Requirement>
  if (typeof r.group !== 'string') return null
  const sources = Array.isArray(r.sources)
    ? r.sources.filter((item): item is string => typeof item === 'string')
    : undefined
  return {
    group: r.group,
    side: r.side === 'Suffix' ? 'Suffix' : 'Prefix',
    accepted: asStringArray(r.accepted),
    minValue: typeof r.minValue === 'number' ? r.minValue : 0,
    label: typeof r.label === 'string' ? r.label : r.group,
    ...(sources && sources.length > 0 ? { sources } : {}),
  }
}

/**
 * The parser bridge may answer with a Stub (`{}`) until the Rust side lands;
 * fill every field so the editor can render whatever arrives.
 */
export function normalizeDraft(raw: unknown): TargetDraft {
  const d = (raw && typeof raw === 'object' ? raw : {}) as Partial<TargetDraft>
  const rarity = RARITIES.find((value) => value === d.rarity) ?? 'Rare'
  return {
    itemClass: typeof d.itemClass === 'string' ? d.itemClass : null,
    base: typeof d.base === 'string' ? d.base : null,
    rarity,
    ilvl: typeof d.ilvl === 'number' ? d.ilvl : null,
    influences: asStringArray(d.influences),
    catalyst: d.catalyst && typeof d.catalyst.name === 'string' ? d.catalyst : null,
    requirements: Array.isArray(d.requirements)
      ? d.requirements.map(normalizeRequirement).filter((r): r is Requirement => r !== null)
      : [],
    minHits: typeof d.minHits === 'number' ? d.minHits : 0,
    buyNote:
      d.buyNote && typeof d.buyNote.amount === 'number' && typeof d.buyNote.currency === 'string'
        ? { amount: d.buyNote.amount, currency: d.buyNote.currency }
        : null,
    unmatched: asStringArray(d.unmatched),
    warnings: asStringArray(d.warnings),
  }
}

/** Price keys for the currencies the parser understands; unknown ones keep the buy price unset. */
const BUY_NOTE_PRICE_KEYS: Record<string, string> = {
  chaos: 'chaos',
  divine: 'divine',
  exalted: 'exalted',
}

export function buyNotePriceKey(currency: string): string | null {
  return BUY_NOTE_PRICE_KEYS[currency.trim().toLowerCase()] ?? null
}
