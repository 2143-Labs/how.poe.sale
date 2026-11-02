import type { Donor, Problem, ProblemTarget, Requirement, TargetDraft } from './types'

/** Pinned to the repoe export the data bundle is built from (contract §3). */
export const BUNDLE_PATCH = '3.29.3.3'

export const DEFAULT_RUNS = 20000
export const DEFAULT_SEED = 7

/** The editable half of a `Problem`: everything the UI owns, without derived fields. */
export interface ProblemState {
  target: ProblemTarget | null
  prices: Record<string, number>
  donors: Donor[]
  added: string[]
  expanded: string[]
  disabled: [string, string][]
  buy: Record<string, number>
  sell: Record<string, number>
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
 * Pure `Problem` builder: no worker, no wasm, no clamping. Prices are forwarded
 * exactly as the user typed them — validating them is the solver's job.
 */
export function buildProblem(state: ProblemState): Problem | null {
  if (!state.target) return null
  return {
    version: 1,
    bundlePatch: BUNDLE_PATCH,
    target: state.target,
    prices: { ...state.prices },
    donors: state.donors.filter(isCompleteDonor).map((donor) => ({ ...donor })),
    added: [...state.added],
    expanded: [...state.expanded],
    disabled: state.disabled.map(([key, actionKey]) => [key, actionKey]),
    buy: priceEntries(state.buy),
    sell: priceEntries(state.sell),
    runs: DEFAULT_RUNS,
    seed: DEFAULT_SEED,
  }
}

/** Turn a `Problem` (a shared link) back into the editable state. */
export function stateFromProblem(problem: Problem): ProblemState {
  return {
    target: problem.target ?? null,
    prices: { ...(problem.prices ?? {}) },
    donors: Array.isArray(problem.donors) ? problem.donors : [],
    added: Array.isArray(problem.added) ? problem.added : [],
    expanded: Array.isArray(problem.expanded) ? problem.expanded : [],
    disabled: Array.isArray(problem.disabled) ? problem.disabled : [],
    buy: Object.fromEntries(problem.buy ?? []),
    sell: Object.fromEntries(problem.sell ?? []),
  }
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
    requirements: draft.requirements.map((requirement) => ({ ...requirement, accepted: [...requirement.accepted] })),
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
    requirements: target.requirements.map((requirement) => ({ ...requirement, accepted: [...requirement.accepted] })),
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
  return {
    group: r.group,
    side: r.side === 'Suffix' ? 'Suffix' : 'Prefix',
    accepted: asStringArray(r.accepted),
    minValue: typeof r.minValue === 'number' ? r.minValue : 0,
    label: typeof r.label === 'string' ? r.label : r.group,
  }
}

/**
 * The parser bridge may answer with the Step-2 stub (`{}`) until Step 8 lands;
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
