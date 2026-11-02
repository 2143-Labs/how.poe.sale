import { create } from 'zustand'
import { DEFAULT_PRICES } from './defaultPrices'
import {
  BUNDLE_PATCH,
  buyNotePriceKey,
  cloneGood,
  cloneProcess,
  cloneTarget,
  draftToTarget,
  emptyMarketState,
  goodName,
  stateFromProblem,
  type MarketState,
} from './problem'
import { parseShareHash } from './share'
import type {
  Donor,
  GoodJson,
  ProblemTarget,
  ProblemV3,
  ProcessJson,
  ProcessProposalJson,
  RecipeDef,
  SolutionV3,
  TargetDraft,
} from './types'

/** The v1 key described per-tree fields; the market state gets its own namespace. */
export const STORAGE_KEY = 'how.poe.sale:v2'

export type Vec2 = { x: number; y: number }

export type View = 'market' | 'process'

export interface PersistedState extends MarketState {
  selectedGood: string | null
  view: View
  /** good id -> canvas position */
  positions: Record<string, Vec2>
}

export interface AppState extends PersistedState {
  /** decode failure of a shared link, shown as a dismissible banner */
  notice: string | null
  // ephemeral — never persisted
  solution: SolutionV3 | null
  busy: boolean
  error: string | null
  /** per-good proposal lists fetched from `propose_processes` */
  proposals: Record<string, ProcessProposalJson[]>

  applyDraft: (draft: TargetDraft) => void
  /** Add the draft as another, unpriced good next to the current target. */
  addGoodFromDraft: (draft: TargetDraft) => string | null
  setPrice: (key: string, value: number | null) => void
  resetPrices: () => void
  setDonors: (donors: Donor[]) => void
  // --- market ---
  setGoodPrice: (id: string, patch: { buy?: number | null; sell?: number | null }) => void
  updateGoodTarget: (id: string, target: ProblemTarget) => void
  useProcessProposal: (proposal: ProcessProposalJson) => void
  removeProcess: (id: string) => void
  addBrickGood: (fromGoodId: string, name: string, target?: ProblemTarget) => string | null
  setFocus: (processId: string | null) => void
  setView: (view: View) => void
  selectGood: (id: string | null) => void
  addAuthoredRecipe: (recipe: RecipeDef) => void
  setProposals: (goodId: string, proposals: ProcessProposalJson[]) => void
  // --- focus-process node actions ---
  addSeed: (key: string) => void
  expand: (key: string) => void
  hide: (pre: string, actionKey: string) => void
  setBuy: (key: string, value: number | null) => void
  setSell: (key: string, value: number | null) => void
  setPosition: (key: string, position: Vec2) => void
  setSolution: (solution: SolutionV3 | null) => void
  setBusy: (busy: boolean) => void
  setError: (error: string | null) => void
  clearNotice: () => void
}

/** The root good's target, which the paste editor edits. */
export function rootTarget(state: Pick<PersistedState, 'goods' | 'root'>): ProblemTarget | null {
  return state.goods.find((good) => good.id === state.root)?.target ?? null
}

export function emptyProblemState(): PersistedState {
  return {
    ...emptyMarketState(),
    prices: { ...DEFAULT_PRICES },
    selectedGood: null,
    view: 'market',
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

function withPair(
  pairs: [string, number][],
  key: string,
  value: number | null,
): [string, number][] {
  const rest = pairs.filter(([entry]) => entry !== key)
  if (value == null || !Number.isFinite(value)) return rest
  return [...rest, [key, value]]
}

function patchProcess(
  processes: ProcessJson[],
  id: string | null,
  change: (process: ProcessJson) => ProcessJson,
): ProcessJson[] {
  if (id == null) return processes
  return processes.map((process) => (process.id === id ? change(process) : process))
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

function storedProblem(parsed: Record<string, unknown>): ProblemV3 | null {
  if (!Array.isArray(parsed.goods) || parsed.goods.length === 0) return null
  if (typeof parsed.root !== 'string') return null
  return {
    version: 3,
    bundlePatch: typeof parsed.bundlePatch === 'string' ? parsed.bundlePatch : BUNDLE_PATCH,
    goods: parsed.goods as GoodJson[],
    processes: Array.isArray(parsed.processes) ? (parsed.processes as ProcessJson[]) : [],
    root: parsed.root,
    focus: typeof parsed.focus === 'string' ? parsed.focus : null,
    prices: (parsed.prices as Record<string, number> | undefined) ?? {},
    donors: Array.isArray(parsed.donors) ? (parsed.donors as Donor[]) : [],
    runs: 0,
    seed: 0,
  }
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
    const problem = storedProblem(parsed)
    if (!problem) return null
    const market = stateFromProblem(problem)
    const selected = typeof parsed.selectedGood === 'string' ? parsed.selectedGood : null
    return {
      ...market,
      prices: { ...DEFAULT_PRICES, ...market.prices },
      selectedGood: market.goods.some((good) => good.id === selected) ? selected : (market.root ?? null),
      view: parsed.view === 'process' ? 'process' : 'market',
      authoredRecipes: Array.isArray(parsed.authoredRecipes) ? (parsed.authoredRecipes as RecipeDef[]) : [],
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
    const market = stateFromProblem(shared.problem)
    return {
      state: {
        ...emptyProblemState(),
        ...market,
        selectedGood: market.root,
        positions: {},
      },
      notice: null,
      fromLink: true,
    }
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
  proposals: {},

  applyDraft: (draft) => {
    const target = draftToTarget(draft)
    if (!target) {
      set({ notice: 'No base item was recognised in that text, so the target was left unchanged.' })
      return
    }
    const previous = get().goods.find((good) => good.id === 'main')
    let buy = previous?.buy ?? null
    let notice: string | null = null
    if (draft.buyNote) {
      const priceKey = buyNotePriceKey(draft.buyNote.currency)
      const price = priceKey ? get().prices[priceKey] : undefined
      if (price != null && Number.isFinite(price)) buy = draft.buyNote.amount * price
      else notice = `No price is set for “${draft.buyNote.currency}”, so the buy price was not changed.`
    }
    // Process keys index the old target's requirements, so the market goes with the target;
    // the root good's prices carry over.
    set({
      goods: [{ id: 'main', name: goodName(target), target, buy, sell: previous?.sell ?? null }],
      processes: [],
      root: 'main',
      focus: null,
      selectedGood: 'main',
      view: 'market',
      positions: {},
      proposals: {},
      notice,
    })
  },

  addGoodFromDraft: (draft) => {
    const target = draftToTarget(draft)
    if (!target) {
      set({ notice: 'No base item was recognised in that text, so no good was added.' })
      return null
    }
    const used = new Set(get().goods.map((good) => good.id))
    let n = 1
    while (used.has(`good-${n}`)) n += 1
    const id = `good-${n}`
    const label = target.requirements.map((r) => r.label).join(', ')
    const good: GoodJson = { id, name: label || goodName(target), target, buy: null, sell: null }
    set((state) => ({ goods: [...state.goods, good], selectedGood: id, view: 'market' }))
    return id
  },

  setPrice: (key, value) => set((state) => ({ prices: withValue(state.prices, key, value) })),
  resetPrices: () => set({ prices: { ...DEFAULT_PRICES } }),
  setDonors: (donors) => set({ donors }),

  setGoodPrice: (id, patch) =>
    set((state) => ({
      goods: state.goods.map((good) =>
        good.id === id
          ? {
              ...good,
              buy: patch.buy !== undefined ? patch.buy : good.buy,
              sell: patch.sell !== undefined ? patch.sell : good.sell,
            }
          : good,
      ),
    })),

  updateGoodTarget: (id, target) =>
    set((state) => ({
      goods: state.goods.map((good) => (good.id === id ? { ...good, name: goodName(target), target } : good)),
      // State keys index the old target's requirements: let the solver rebuild this good's closure.
      processes: state.processes.map((process) =>
        process.output === id ? { ...process, expanded: null } : process,
      ),
    })),

  useProcessProposal: (proposal) =>
    set((state) => {
      const known = new Set(state.goods.map((good) => good.id))
      const newGoods = proposal.newGoods.filter((good) => !known.has(good.id)).map(cloneGood)
      const exists = state.processes.some((process) => process.id === proposal.process.id)
      const processes = exists
        ? state.processes.map((process) =>
            process.id === proposal.process.id ? cloneProcess(proposal.process) : process,
          )
        : [...state.processes, cloneProcess(proposal.process)]
      const isCraft = 'craft' in proposal.process.kind
      return {
        goods: [...state.goods, ...newGoods],
        processes,
        root: state.root ?? proposal.process.output,
        focus: isCraft ? proposal.process.id : state.focus,
        view: isCraft ? 'process' : 'market',
        selectedGood: proposal.process.output,
      }
    }),

  removeProcess: (id) =>
    set((state) => ({
      processes: state.processes.filter((process) => process.id !== id),
      focus: state.focus === id ? null : state.focus,
      view: state.focus === id ? 'market' : state.view,
    })),

  addBrickGood: (fromGoodId, name, target) => {
    const source = get().goods.find((good) => good.id === fromGoodId)
    if (!source) return null
    const used = new Set(get().goods.map((good) => good.id))
    let n = 1
    while (used.has(`brick-${n}`)) n += 1
    const id = `brick-${n}`
    const good: GoodJson = {
      id,
      name: name.trim() !== '' ? name.trim() : `Brick ${n}`,
      target: cloneTarget(target ?? source.target),
      buy: null,
      sell: null,
    }
    set((state) => ({ goods: [...state.goods, good], selectedGood: id }))
    return id
  },

  setFocus: (processId) =>
    set((state) => ({
      focus: processId,
      view: processId != null && state.processes.some((process) => process.id === processId) ? 'process' : 'market',
    })),

  setView: (view) => set({ view }),
  selectGood: (id) => set({ selectedGood: id }),
  addAuthoredRecipe: (recipe) =>
    set((state) => ({
      authoredRecipes: state.authoredRecipes.some((existing) => existing.id === recipe.id)
        ? state.authoredRecipes.map((existing) => (existing.id === recipe.id ? recipe : existing))
        : [...state.authoredRecipes, recipe],
    })),
  setProposals: (goodId, proposals) =>
    set((state) => ({ proposals: { ...state.proposals, [goodId]: proposals } })),

  addSeed: (key) =>
    set((state) => ({
      processes: patchProcess(state.processes, state.focus, (process) => ({
        ...process,
        added: unique(process.added, key),
        expanded: expandedWith(state, key),
      })),
    })),
  expand: (key) =>
    set((state) => ({
      processes: patchProcess(state.processes, state.focus, (process) => ({
        ...process,
        expanded: expandedWith(state, key),
      })),
    })),
  hide: (pre, actionKey) =>
    set((state) => ({
      processes: patchProcess(state.processes, state.focus, (process) => ({
        ...process,
        disabled: process.disabled.some(([key, action]) => key === pre && action === actionKey)
          ? process.disabled
          : [...process.disabled, [pre, actionKey]],
      })),
    })),
  setBuy: (key, value) =>
    set((state) => ({
      processes: patchProcess(state.processes, state.focus, (process) => ({
        ...process,
        buy: withPair(process.buy, key, value),
      })),
    })),
  setSell: (key, value) =>
    set((state) => ({
      processes: patchProcess(state.processes, state.focus, (process) => ({
        ...process,
        sell: withPair(process.sell, key, value),
      })),
    })),

  setPosition: (key, position) => set((state) => ({ positions: { ...state.positions, [key]: position } })),
  setSolution: (solution) => set({ solution }),
  setBusy: (busy) => set({ busy }),
  setError: (error) => set({ error }),
  clearNotice: () => set({ notice: null }),
}))

/**
 * The focus process's `expanded` set. `null` means "the solver builds the
 * closure", so the first edit materialises the closure it last returned and
 * adds the node being opened.
 */
function expandedWith(state: AppState, key: string): string[] {
  const focus = state.processes.find((process) => process.id === state.focus)
  const known = focus?.expanded ?? state.solution?.focus?.nodes.map((node) => node.key) ?? ['goal']
  return known.includes(key) ? known : [...known, key]
}

export function persistedState(state: AppState): PersistedState {
  return {
    goods: state.goods,
    processes: state.processes,
    root: state.root,
    focus: state.focus,
    prices: state.prices,
    donors: state.donors,
    authoredRecipes: state.authoredRecipes,
    selectedGood: state.selectedGood,
    view: state.view,
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
