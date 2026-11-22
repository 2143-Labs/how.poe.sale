import { createStore, type StoreApi } from 'zustand/vanilla'
import type { AppClient } from './client'
import type { Scope } from './scope'
import { parseSetup, parseShareHash, shareKind } from './share'
import type {
  Accept,
  ApplyView,
  ItemSpec,
  LoopRequest,
  LoopView,
  ModOptionView,
  SelectorSpec,
  Setup,
  Side,
  Start,
} from './types'

export type SelectorGrade = 'desired' | 'good'

export interface AppState {
  /** The engine's start view, or `null` when it could not be built. */
  start: Start | null
  /** What the panels edit, what is saved, and what the loop runs. */
  setup: Setup
  loop: LoopView | null
  applied: ApplyView | null
  error: string | null
  notice: string | null
  /** A loop report is in flight. */
  busy: boolean
  /** A currency application is in flight. */
  applying: boolean
  /** The modifiers the setup's base can name. */
  mods: ModOptionView[]

  /** Runs the loop and loads the modifiers for the setup the page opened with; `mount` calls it once. */
  boot: () => void
  setItem: (patch: Partial<ItemSpec>) => void
  setAccept: (accept: Accept) => void
  setDepth: (depth: number) => void
  /** Sets a cost key's price, or drops the key when `value` is `null`. */
  setPrice: (key: string, value: number | null) => void
  setTradePenalty: (value: number) => void
  /** Marks `id` desired or good on `side`, or clears it from the goal when `grade` is `null`. */
  setMod: (side: Side, id: string, grade: SelectorGrade | null) => void
  /** Whether a chosen modifier stands for its family at that tier or higher. */
  setOrBetter: (id: string, orBetter: boolean) => void
  /** Runs the loop and solves the priced graph; the button's answer. */
  priceLoop: () => void
  applyCurrency: (id: string) => void
  setError: (message: string | null) => void
  dismissError: () => void
  dismissNotice: () => void
}

export type AppStore = StoreApi<AppState>

/** What the store reads and writes: the worker, the page's fragment and its storage. */
export interface StoreEnv {
  client: AppClient
  /** The engine's start view, already fetched by `mount`. */
  start: Start | null
  /** Why `start` is `null`, shown as the page's error. */
  startError?: string | null
  hash: string
  storage: Pick<Storage, 'getItem' | 'setItem'> | null
  /** Shown when the browser has no usable storage. */
  storageNotice?: string | null
  /** Drops a share fragment from the address bar once it is read. */
  dropHash: () => void
  /** The wait before a setup change runs the loop; tests use 0. */
  debounceMs?: number
}

/** The setup a page opens with when nothing was saved: the game's default base, white and empty. */
function seed(scope: Scope, start: Start | null): Setup {
  return {
    item: { base: scope.defaultBase, itemLevel: scope.defaultItemLevel, rarity: 'normal', influences: [] },
    goal: { desired: [], good: [], accept: 'desired-or-good-both-sides' },
    prices: { prices: { ...start?.priceDefaults }, tradePenalty: 0.02 },
    depth: 6,
  }
}

/** The setup a page opens with, and a notice when a link or a saved preset could not be used. */
function initial(scope: Scope, env: StoreEnv): { setup: Setup; notice: string | null } {
  const saved = (): Setup => {
    try {
      const raw = env.storage?.getItem(scope.storageKey)
      return (raw ? parseSetup(JSON.parse(raw)) : null) ?? seed(scope, env.start)
    } catch {
      return seed(scope, env.start)
    }
  }
  const shared = parseShareHash(env.hash)
  if (shared) {
    env.dropHash()
    return { setup: shared, notice: null }
  }
  switch (shareKind(env.hash)) {
    case 'share':
      return { setup: saved(), notice: 'This share link could not be read.' }
    case 'legacy':
      return { setup: saved(), notice: 'This link was made by the previous version of how.poe.sale and cannot be opened.' }
    case null:
      return { setup: saved(), notice: env.storageNotice ?? null }
  }
}

export function createAppStore(scope: Scope, env: StoreEnv): AppStore {
  const opened = initial(scope, env)
  const debounceMs = env.debounceMs ?? 400
  const modsCache = new Map<string, ModOptionView[]>()
  let loopTimer: ReturnType<typeof setTimeout> | undefined
  let modsTimer: ReturnType<typeof setTimeout> | undefined
  let modsKey: string | null = null

  const store = createStore<AppState>()((set, get) => {
    const fail = (reason: unknown) => set({ error: reason instanceof Error ? reason.message : String(reason) })

    const request = (setup: Setup, price: boolean): LoopRequest => ({
      item: setup.item,
      goal: setup.goal,
      prices: setup.prices,
      depth: setup.depth,
      price,
    })

    const runNow = (price: boolean) => {
      if (!get().start) return
      set({ busy: true })
      env.client
        .loopReport(request(get().setup, price))
        .then((loop) => set({ loop, busy: false, error: null }))
        .catch((reason: unknown) => {
          fail(reason)
          set({ busy: false })
        })
    }

    /** Runs the loop after the wait, unless a newer change replaces it or a priced run pre-empts it. */
    const schedule = () => {
      clearTimeout(loopTimer)
      loopTimer = setTimeout(() => {
        loopTimer = undefined
        runNow(false)
      }, debounceMs)
    }

    /** The modifiers a base and item level can name, fetched once per base, level and influence set. */
    const loadMods = () => {
      const item = get().setup.item
      const key = `${item.base}|${item.itemLevel}|${item.influences.join(',')}`
      if (key === modsKey) return
      modsKey = key
      const cached = modsCache.get(key)
      if (cached) {
        set({ mods: cached })
        return
      }
      clearTimeout(modsTimer)
      modsTimer = setTimeout(() => {
        modsTimer = undefined
        env.client
          .mods(item.base, item.itemLevel, item.influences)
          .then((mods) => {
            modsCache.set(key, mods)
            if (modsKey === key) set({ mods })
          })
          .catch((reason: unknown) => {
            if (modsKey !== key) return
            set({ mods: [] })
            fail(reason)
          })
      }, 200)
    }

    /** The setup after a panel changed it: saved, and the loop re-run for it. */
    const commit = (setup: Setup) => {
      set({ setup })
      loadMods()
      schedule()
    }

    const without = (list: SelectorSpec[], id: string) => list.filter((slot) => slot.id !== id)

    return {
      start: env.start,
      setup: opened.setup,
      loop: null,
      applied: null,
      error: env.startError ?? null,
      notice: opened.notice,
      busy: false,
      applying: false,
      mods: [],

      boot: () => {
        loadMods()
        runNow(false)
      },

      setItem: (patch) => {
        const setup = get().setup
        const item = { ...setup.item, ...patch }
        // The goal's modifiers belong to a base's data: another base or influence set cannot name them.
        const reset =
          (patch.base !== undefined && patch.base !== setup.item.base) ||
          (patch.influences !== undefined && patch.influences.join(',') !== setup.item.influences.join(','))
        const goal = reset ? { ...setup.goal, desired: [], good: [] } : setup.goal
        commit({ ...setup, item, goal })
      },

      setAccept: (accept) => {
        const setup = get().setup
        commit({ ...setup, goal: { ...setup.goal, accept } })
      },

      setDepth: (depth) => {
        const setup = get().setup
        commit({ ...setup, depth: Math.min(20, Math.max(1, Math.trunc(depth))) })
      },

      setPrice: (key, value) => {
        const setup = get().setup
        const prices = { ...setup.prices.prices }
        if (value === null || !Number.isFinite(value)) delete prices[key]
        else prices[key] = value
        commit({ ...setup, prices: { ...setup.prices, prices } })
      },

      setTradePenalty: (value) => {
        const setup = get().setup
        commit({ ...setup, prices: { ...setup.prices, tradePenalty: Number.isFinite(value) ? value : 0 } })
      },

      setMod: (side, id, grade) => {
        const setup = get().setup
        const known = [...setup.goal.desired, ...setup.goal.good].find((slot) => slot.id === id)
        const desired = without(setup.goal.desired, id)
        const good = without(setup.goal.good, id)
        if (grade === 'desired') desired.push({ side, id, orBetter: known?.orBetter ?? false })
        if (grade === 'good') good.push({ side, id, orBetter: known?.orBetter ?? false })
        commit({ ...setup, goal: { ...setup.goal, desired, good } })
      },

      setOrBetter: (id, orBetter) => {
        const setup = get().setup
        const flip = (list: SelectorSpec[]) =>
          list.map((slot) => (slot.id === id ? { ...slot, orBetter } : slot))
        commit({ ...setup, goal: { ...setup.goal, desired: flip(setup.goal.desired), good: flip(setup.goal.good) } })
      },

      priceLoop: () => {
        // A pending unpriced run would supersede the priced one.
        clearTimeout(loopTimer)
        loopTimer = undefined
        runNow(true)
      },

      applyCurrency: (currency) => {
        set({ applying: true })
        env.client
          .applyCurrency({ item: get().setup.item, currency })
          .then((applied) => set({ applied, applying: false, error: null }))
          .catch((reason: unknown) => {
            fail(reason)
            set({ applying: false })
          })
      },

      setError: (error) => set({ error }),
      dismissError: () => set({ error: null }),
      dismissNotice: () => set({ notice: null }),
    }
  })

  if (env.storage) {
    const storage = env.storage
    store.subscribe((state, previous) => {
      if (state.setup === previous.setup) return
      try {
        storage.setItem(scope.storageKey, JSON.stringify({ v: 1, ...state.setup }))
      } catch {
        // Storage full or unavailable: the setup still lives for this visit.
        if (store.getState().notice === null) {
          store.setState({ notice: 'Presets cannot be saved in this browser.' })
        }
      }
    })
  }
  return store
}
