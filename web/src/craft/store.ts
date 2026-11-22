import { createStore, type StoreApi } from 'zustand/vanilla'
import type { CraftClient } from './client'
import {
  DEFAULTS,
  duplicate as duplicatePreset,
  exportJson,
  find,
  importJson,
  isBuiltin,
  load,
  remove as removePreset,
  save,
  type Kind,
  type Library,
} from './library'
import type { Catalog, CraftSpec, FilterSpec, ModView, PriceSpec, RunView } from './types'

export type Tab = 'craft' | 'filters' | 'prices'

export interface CraftState {
  catalog: Catalog
  /** The built-ins merged with the user's presets. */
  library: Library
  /** The preset each editor shows: a craft, its goal filter and its price list. */
  selected: Record<Kind, string>
  tab: Tab
  result: RunView | null
  error: string | null
  /** Storage banners, and the page's own notices. */
  notice: string | null
  busy: boolean
  /** One base's modifiers, once they have been fetched. */
  mods: Record<string, ModView[]>
  /** Picks a preset. Picking a craft also picks the goal filter and prices it names. */
  select(kind: Kind, id: string): void
  setTab(tab: Tab): void
  /** Replaces the user's filter with this one. A built-in filter is left alone. */
  updateFilter(filter: FilterSpec): void
  updatePrices(prices: PriceSpec): void
  updateCraft(craft: CraftSpec): void
  /** Copies a preset and selects the copy. */
  duplicate(kind: Kind, id: string): void
  /** Removes a preset, falling back to the default when it was selected; returns the refusal text. */
  remove(kind: Kind, id: string): string | null
  exportLibrary(): string
  /** Merges another library in; returns the error text when the text will not read. */
  importLibrary(text: string): string | null
  /** Fetches one base's modifiers, once. */
  loadMods(base: string): void
  setBusy(busy: boolean): void
  setNotice(notice: string | null): void
}

export type CraftStore = StoreApi<CraftState>

/** What a store needs from the page around it. */
export interface CraftDeps {
  catalog: Catalog
  client: CraftClient
  storage: Storage | null
  /** A storage problem the page found before the store existed. */
  notice: string | null
}

/** The wait after an edit before the engine is asked again. */
const WAIT = 300

/** The triple a page opens on: a craft, and the goal filter and prices that craft names. */
function openingSelection(library: Library): Record<Kind, string> {
  const craft = find(library, 'craft', DEFAULTS.craft) ?? library.crafts[0]
  if (!craft) return { ...DEFAULTS }
  return {
    craft: craft.id,
    filter: find(library, 'filter', craft.goal) ? craft.goal : DEFAULTS.filter,
    prices: find(library, 'prices', craft.prices) ? craft.prices : DEFAULTS.prices,
  }
}

/**
 * The craft planner's state: the catalog, the presets, what is selected, and the last run. Every
 * library change is saved and schedules a run; edits are coalesced for {@link WAIT} ms so a fast
 * typist asks the engine once, and only the newest run may write its answer.
 */
export function createCraftStore(deps: CraftDeps): CraftStore {
  const loaded = load(deps.storage, deps.catalog.builtins, deps.catalog.bases)
  /** The bases whose modifier table is being fetched, so each is asked for once. */
  const fetching = new Set<string>()

  let timer: ReturnType<typeof setTimeout> | undefined
  /** The run that may write its answer: an older one is dropped. */
  let attempt = 0

  const store = createStore<CraftState>()((set, get) => {
    const persist = (library: Library) => {
      save(deps.storage, library)
      return library
    }

    /** Replaces one preset in place, saves, and asks for a fresh run. */
    const replace = (library: Library, kind: Kind, entry: FilterSpec | PriceSpec | CraftSpec) => {
      const next: Library =
        kind === 'filter'
          ? {
              ...library,
              filters: library.filters.map((filter) => (filter.id === entry.id ? (entry as FilterSpec) : filter)),
            }
          : kind === 'prices'
            ? {
                ...library,
                prices: library.prices.map((prices) => (prices.id === entry.id ? (entry as PriceSpec) : prices)),
              }
            : {
                ...library,
                crafts: library.crafts.map((craft) => (craft.id === entry.id ? (entry as CraftSpec) : craft)),
              }
      set({ library: persist(next) })
      schedule()
    }

    return {
      catalog: deps.catalog,
      library: loaded.library,
      selected: openingSelection(loaded.library),
      tab: 'craft',
      result: null,
      error: null,
      notice: deps.notice ?? loaded.notice,
      busy: false,
      mods: {},

      select(kind, id) {
        const state = get()
        const selected: Record<Kind, string> = { ...state.selected }
        selected[kind] = id
        if (kind === 'craft') {
          const craft = find(state.library, 'craft', id)
          if (craft) {
            selected.filter = find(state.library, 'filter', craft.goal) ? craft.goal : DEFAULTS.filter
            selected.prices = find(state.library, 'prices', craft.prices) ? craft.prices : DEFAULTS.prices
          }
        }
        set({ selected })
        schedule()
      },

      setTab(tab) {
        set({ tab })
      },

      updateFilter(filter) {
        if (isBuiltin(filter.id)) return
        replace(get().library, 'filter', filter)
      },

      updatePrices(prices) {
        if (isBuiltin(prices.id)) return
        replace(get().library, 'prices', prices)
      },

      updateCraft(craft) {
        if (isBuiltin(craft.id)) return
        const state = get()
        if (state.selected.craft === craft.id) {
          // The craft names its own goal filter and prices, so the other two tabs follow its header.
          const selected: Record<Kind, string> = { ...state.selected }
          if (find(state.library, 'filter', craft.goal)) selected.filter = craft.goal
          if (find(state.library, 'prices', craft.prices)) selected.prices = craft.prices
          set({ selected })
        }
        replace(state.library, 'craft', craft)
      },

      duplicate(kind, id) {
        const result = duplicatePreset(get().library, kind, id)
        if (result.id === id) return
        set({ library: persist(result.library) })
        get().select(kind, result.id)
      },

      remove(kind, id) {
        const result = removePreset(get().library, kind, id)
        if ('error' in result) return result.error
        const selected: Record<Kind, string> = { ...get().selected }
        if (selected[kind] === id) {
          selected[kind] = DEFAULTS[kind]
          if (kind === 'craft') {
            const craft = find(result.library, 'craft', DEFAULTS.craft)
            selected.filter = craft?.goal ?? DEFAULTS.filter
            selected.prices = craft?.prices ?? DEFAULTS.prices
          }
        }
        set({ library: persist(result.library), selected })
        schedule()
        return null
      },

      exportLibrary() {
        return exportJson(get().library)
      },

      importLibrary(text) {
        try {
          const library = importJson(text, get().library, deps.catalog.bases)
          set({ library: persist(library) })
          schedule()
          return null
        } catch (reason: unknown) {
          return reason instanceof Error ? reason.message : String(reason)
        }
      },

      loadMods(base) {
        if (get().mods[base] || fetching.has(base)) return
        fetching.add(base)
        deps.client
          .mods(base)
          .then((mods) => {
            fetching.delete(base)
            set({ mods: { ...get().mods, [base]: mods } })
          })
          .catch(() => {
            // The table stays empty; the editor asks again the next time the base is picked.
            fetching.delete(base)
          })
      },

      setBusy(busy) {
        set({ busy })
      },

      setNotice(notice) {
        set({ notice })
      },
    }
  })

  /** Runs the selected craft, under every filter the library knows and the prices it is costed at. */
  async function run(token: number) {
    const state = store.getState()
    const craft = find(state.library, 'craft', state.selected.craft)
    const prices = find(state.library, 'prices', state.selected.prices) ?? (craft && find(state.library, 'prices', craft.prices))
    if (!craft || !prices) {
      store.setState({ result: null, error: 'the selected craft or price list is missing' })
      return
    }
    try {
      const result = await deps.client.run({
        filters: state.library.filters,
        prices,
        craft,
        library: state.library.crafts,
      })
      if (token === attempt) store.setState({ result, error: null })
    } catch (reason: unknown) {
      if (token !== attempt) return
      store.setState({ result: null, error: reason instanceof Error ? reason.message : String(reason) })
    }
  }

  function schedule() {
    clearTimeout(timer)
    const token = ++attempt
    timer = setTimeout(() => {
      timer = undefined
      void run(token)
    }, WAIT)
  }

  // The page opens on a run, so its numbers are there when the panels first draw.
  schedule()

  return store
}
