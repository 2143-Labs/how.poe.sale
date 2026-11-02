import { create } from 'zustand'
import { DEFAULT_PRICES } from './defaultPrices'
import {
  buyNotePriceKey,
  draftToTarget,
  stateFromProblem,
  type ProblemState,
} from './problem'
import { parseShareHash } from './share'
import type { Donor, ProblemTarget, Solution, TargetDraft } from './types'

export const STORAGE_KEY = 'how.poe.sale:v1'

export type Vec2 = { x: number; y: number }

export interface PersistedState extends ProblemState {
  positions: Record<string, Vec2>
}

export interface AppState extends PersistedState {
  /** decode failure of a shared link, shown as a dismissible banner */
  notice: string | null
  // ephemeral — never persisted
  solution: Solution | null
  busy: boolean
  error: string | null

  setTarget: (target: ProblemTarget | null) => void
  applyDraft: (draft: TargetDraft) => void
  setPrice: (key: string, value: number | null) => void
  resetPrices: () => void
  setDonors: (donors: Donor[]) => void
  setBuy: (key: string, value: number | null) => void
  setSell: (key: string, value: number | null) => void
  addSeed: (key: string) => void
  expand: (key: string) => void
  hide: (pre: string, actionKey: string) => void
  setPosition: (key: string, position: Vec2) => void
  setSolution: (solution: Solution | null) => void
  setBusy: (busy: boolean) => void
  setError: (error: string | null) => void
  clearNotice: () => void
}

export function emptyProblemState(): PersistedState {
  return {
    target: null,
    prices: { ...DEFAULT_PRICES },
    donors: [],
    added: [],
    expanded: ['goal'],
    disabled: [],
    buy: {},
    sell: {},
    positions: {},
  }
}

function unique(items: string[], item: string): string[] {
  return items.includes(item) ? items : [...items, item]
}

function withValue(record: Record<string, number>, key: string, value: number | null): Record<string, number> {
  const next = { ...record }
  if (value == null || !Number.isFinite(value)) delete next[key]
  else next[key] = value
  return next
}

function goalOnly(record: Record<string, number>): Record<string, number> {
  return 'goal' in record ? { goal: record.goal } : {}
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []
}

function asDisabled(value: unknown): [string, string][] {
  if (!Array.isArray(value)) return []
  return value.filter(
    (pair): pair is [string, string] =>
      Array.isArray(pair) && pair.length === 2 && typeof pair[0] === 'string' && typeof pair[1] === 'string',
  )
}

function asPositions(value: unknown): Record<string, Vec2> {
  const out: Record<string, Vec2> = {}
  if (!value || typeof value !== 'object') return out
  for (const [key, position] of Object.entries(value as Record<string, unknown>)) {
    const p = position as Partial<Vec2> | null
    if (p && typeof p.x === 'number' && typeof p.y === 'number') out[key] = { x: p.x, y: p.y }
  }
  return out
}

function fromStorage(): PersistedState | null {
  if (typeof localStorage === 'undefined') return null
  let raw: string | null
  try {
    raw = localStorage.getItem(STORAGE_KEY)
  } catch {
    return null
  }
  if (!raw) return null
  try {
    const parsed = JSON.parse(raw) as Record<string, unknown>
    const target = parsed.target as ProblemTarget | null | undefined
    return {
      target: target && typeof target === 'object' && !Array.isArray(target) ? target : null,
      prices: { ...DEFAULT_PRICES, ...((parsed.prices as Record<string, number> | undefined) ?? {}) },
      donors: Array.isArray(parsed.donors) ? (parsed.donors as Donor[]) : [],
      added: asStringArray(parsed.added),
      expanded: asStringArray(parsed.expanded),
      disabled: asDisabled(parsed.disabled),
      buy: (parsed.buy as Record<string, number> | undefined) ?? {},
      sell: (parsed.sell as Record<string, number> | undefined) ?? {},
      positions: asPositions(parsed.positions),
    }
  } catch {
    return null
  }
}

interface Initial {
  state: PersistedState
  notice: string | null
  /** the problem came from a `#p=` fragment (valid or not) */
  fromLink: boolean
}

/**
 * A shared `#p=` link wins over the saved problem; a link that does not decode
 * falls back to `localStorage` and surfaces a banner.
 */
export function initialState(): Initial {
  const hash = typeof location === 'undefined' ? '' : location.hash
  const shared = parseShareHash(hash)
  if (shared.kind === 'ok') {
    return { state: { ...stateFromProblem(shared.problem), positions: {} }, notice: null, fromLink: true }
  }
  const saved = fromStorage()
  if (shared.kind === 'invalid') {
    return {
      state: saved ?? emptyProblemState(),
      notice: 'Could not read the shared link. Showing your saved problem instead.',
      fromLink: true,
    }
  }
  return { state: saved ?? emptyProblemState(), notice: null, fromLink: false }
}

const initial = initialState()

export const useAppStore = create<AppState>()((set, get) => ({
  ...initial.state,
  notice: initial.notice,
  solution: null,
  busy: false,
  error: null,

  setTarget: (target) => set({ target }),

  applyDraft: (draft) => {
    const target = draftToTarget(draft)
    if (!target) {
      set({ notice: 'No base item was recognised in that text, so the target was left unchanged.' })
      return
    }
    const { prices, buy, sell } = get()
    let nextBuy = buy
    let notice: string | null = null
    if (draft.buyNote) {
      const priceKey = buyNotePriceKey(draft.buyNote.currency)
      const price = priceKey ? prices[priceKey] : undefined
      if (price != null && Number.isFinite(price)) {
        nextBuy = { ...buy, goal: draft.buyNote.amount * price }
      } else {
        notice = `No price is set for “${draft.buyNote.currency}”, so the buy price was not changed.`
      }
    }
    // State keys index the old target's requirements, so everything keyed by them goes with the
    // tree; only the goal's prices carry over.
    set({
      target,
      buy: goalOnly(nextBuy),
      sell: goalOnly(sell),
      positions: {},
      added: [],
      expanded: ['goal'],
      disabled: [],
      notice,
    })
  },

  setPrice: (key, value) => set((state) => ({ prices: withValue(state.prices, key, value) })),
  resetPrices: () => set({ prices: { ...DEFAULT_PRICES } }),
  setDonors: (donors) => set({ donors }),
  setBuy: (key, value) => set((state) => ({ buy: withValue(state.buy, key, value) })),
  setSell: (key, value) => set((state) => ({ sell: withValue(state.sell, key, value) })),

  addSeed: (key) =>
    set((state) => ({ added: unique(state.added, key), expanded: unique(state.expanded, key) })),
  expand: (key) => set((state) => ({ expanded: unique(state.expanded, key) })),
  hide: (pre, actionKey) =>
    set((state) => ({
      disabled: state.disabled.some(([key, action]) => key === pre && action === actionKey)
        ? state.disabled
        : [...state.disabled, [pre, actionKey]],
    })),

  setPosition: (key, position) => set((state) => ({ positions: { ...state.positions, [key]: position } })),
  setSolution: (solution) => set({ solution }),
  setBusy: (busy) => set({ busy }),
  setError: (error) => set({ error }),
  clearNotice: () => set({ notice: null }),
}))

export function persistedState(state: AppState): PersistedState {
  return {
    target: state.target,
    prices: state.prices,
    donors: state.donors,
    added: state.added,
    expanded: state.expanded,
    disabled: state.disabled,
    buy: state.buy,
    sell: state.sell,
    positions: state.positions,
  }
}

function save(state: PersistedState) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
  } catch {
    // quota or private mode: the app keeps working without persistence
  }
}

const PERSISTED_FIELDS = Object.keys(emptyProblemState()) as (keyof PersistedState)[]

if (typeof localStorage !== 'undefined') {
  // Only problem edits are written; solutions and the busy flag change far more often.
  useAppStore.subscribe((state, previous) => {
    if (PERSISTED_FIELDS.some((field) => state[field] !== previous[field])) save(persistedState(state))
  })
  if (initial.fromLink && typeof history !== 'undefined') {
    // An opened link becomes the current problem: keep it and drop the fragment, so a later
    // reload restores the user's edits instead of re-reading the link over them.
    if (initial.notice === null) save(persistedState(useAppStore.getState()))
    history.replaceState(null, '', location.pathname + location.search)
  }
}
