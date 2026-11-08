import { createStore, type StoreApi } from 'zustand/vanilla'
import type { Client } from '../client'
import { parseShareHash } from '../share'
import {
  normalizeSpec,
  type AbsSpec,
  type AppliedView,
  type EdgeView,
  type MadeByView,
  type MethodOption,
  type ModOption,
  type NodeView,
  type Pin,
  type SlotSpec,
} from '../types'
import type { Scope } from './scope'

export type Tab = 'forward' | 'backward'

export interface FlowState {
  /** The item the chart is built from, as the panel edits it. */
  root: AbsSpec
  /** The modifiers tracked on top of the root's own. */
  watch: SlotSpec[]
  /** The root first, then every node pinned since. */
  nodes: NodeView[]
  edges: EdgeView[]
  /** The pinned steps, in order, so a reload can replay them. */
  pins: Pin[]
  selected: string | null
  tab: Tab
  /** The methods the selected node can be pushed through. */
  methods: MethodOption[]
  method: string | null
  applied: AppliedView | null
  madeBy: MadeByView | null
  busy: boolean
  error: string | null
  /** A share-link or replay problem, shown as a dismissible banner. */
  notice: string | null
  mods: ModOption[]

  setRoot: (root: AbsSpec) => void
  setWatch: (watch: SlotSpec[]) => void
  select: (id: string) => void
  setTab: (tab: Tab) => void
  chooseMethod: (id: string) => void
  /** Pins an outcome of the method shown for the selected node. */
  pinForward: (key: string) => void
  /** Pins an input a method could have started from, above the selected node. */
  pinBackward: (method: string, key: string) => void
  /** Takes a node off the chart, with every node only it led to. */
  unpin: (id: string) => void
  clearChart: () => void
  setError: (message: string | null) => void
  setNotice: (message: string | null) => void
  setBusy: (busy: boolean) => void
  setMods: (mods: ModOption[]) => void
}

export type FlowStore = StoreApi<FlowState>

/** What the store reads at start and writes to: the page's fragment and storage. */
export interface StoreEnv {
  client: Client
  hash: string
  storage: Pick<Storage, 'getItem' | 'setItem'> | null
  /** Drops a share fragment from the address bar once it is read. */
  dropHash: () => void
}

/** The root a page opens with when nothing was saved: the game's default base, rare and empty. */
export function seedRoot(scope: Scope): AbsSpec {
  return {
    base: scope.defaultBase,
    itemLevel: 87,
    rarity: 'rare',
    influences: [],
    prefixes: [],
    suffixes: [],
    anyPrefixes: 0,
    anySuffixes: 0,
    corrupted: false,
  }
}

interface Saved {
  root: AbsSpec
  watch: SlotSpec[]
  pins: Pin[]
}

/** A pinned step, when it reads back as one. */
function isPin(raw: unknown): raw is Pin {
  if (!raw || typeof raw !== 'object') return false
  const pin = raw as Pin
  return (
    (pin.dir === 'f' || pin.dir === 'b') &&
    typeof pin.node === 'string' &&
    typeof pin.method === 'string' &&
    typeof pin.key === 'string'
  )
}

function pinsOf(raw: unknown): Pin[] {
  return (Array.isArray(raw) ? raw : []).filter(isPin)
}

function watchOf(raw: unknown): SlotSpec[] {
  return (Array.isArray(raw) ? raw : []).flatMap((entry: unknown) => {
    if (!entry || typeof entry !== 'object' || !('id' in entry) || typeof entry.id !== 'string') return []
    return 'orBetter' in entry && entry.orBetter === true
      ? [{ id: entry.id, orBetter: true }]
      : [{ id: entry.id }]
  })
}

/** The saved chart, when it parses and belongs to its own page. */
function load(scope: Scope, storage: StoreEnv['storage']): Saved | null {
  try {
    const raw = storage?.getItem(scope.storageKey)
    if (!raw) return null
    const value = JSON.parse(raw) as Record<string, unknown> | null
    const root = normalizeSpec(value?.root)
    if (!value || !root) return null
    return { root, watch: watchOf(value.watch), pins: pinsOf(value.pins) }
  } catch {
    return null
  }
}

/** The state a page opens with, and a notice when a share link could not be used. */
function initial(scope: Scope, env: StoreEnv): Saved & { notice: string | null } {
  const saved = (): Saved => load(scope, env.storage) ?? { root: seedRoot(scope), watch: [], pins: [] }
  const share = parseShareHash(env.hash)
  switch (share.kind) {
    case 'ok':
      env.dropHash()
      if (share.scope !== scope.id) {
        return { ...saved(), notice: 'This share link is for another page.' }
      }
      return { root: share.root, watch: share.watch, pins: share.pins, notice: null }
    case 'legacy':
      return { ...saved(), notice: 'This link was made by the previous version of how.poe.sale and cannot be opened.' }
    case 'invalid':
      return { ...saved(), notice: 'This share link could not be read.' }
    case 'none':
      return { ...saved(), notice: null }
  }
}

/** Every node still joined to the root by pinned steps. */
function reachable(nodes: NodeView[], edges: EdgeView[]): Set<string> {
  const keep = new Set<string>(nodes.length > 0 ? [nodes[0].id] : [])
  let grew = true
  while (grew) {
    grew = false
    for (const edge of edges) {
      if (keep.has(edge.to) !== keep.has(edge.from)) {
        keep.add(edge.from)
        keep.add(edge.to)
        grew = true
      }
    }
  }
  return keep
}

export function createFlowStore(scope: Scope, env: StoreEnv): FlowStore {
  const start = initial(scope, env)
  const store = createStore<FlowState>()((set, get) => {
    const fail = (reason: unknown) =>
      set({ error: reason instanceof Error ? reason.message : String(reason) })

    /** Adds a pinned node and edge, when they are new. */
    const keep = (node: NodeView, edge: EdgeView) => {
      const { nodes, edges } = get()
      set({
        nodes: nodes.some((known) => known.id === node.id) ? nodes : [...nodes, node],
        edges: edges.some(
          (known) => known.from === edge.from && known.to === edge.to && known.method === edge.method,
        )
          ? edges
          : [...edges, edge],
        selected: node.id,
      })
    }

    /** The methods the selected node can be pushed through, and its inputs when it shows them. */
    const refresh = async (id: string) => {
      const { tab } = get()
      try {
        const methods = await env.client.chartMethods(id)
        if (get().selected !== id) return
        set({ methods, method: null, applied: null })
        if (tab === 'backward') {
          const madeBy = await env.client.chartMadeBy(id, [])
          if (get().selected === id) set({ madeBy })
        }
      } catch (reason) {
        fail(reason)
      }
    }

    /** Starts the engine's chart afresh from `root` and `watch`, then replays `pins`. */
    const restart = async (root: AbsSpec, watch: SlotSpec[], pins: Pin[]) => {
      set({ root, watch, pins: [], nodes: [], edges: [], selected: null, methods: [], method: null, applied: null, madeBy: null })
      try {
        const node = await env.client.chartStart(root, watch)
        set({ nodes: [node], selected: node.id })
        for (const pin of pins) {
          const pinned =
            pin.dir === 'f'
              ? await env.client.chartPinForward(pin.node, pin.method, pin.key)
              : await env.client.chartPinBackward(pin.node, pin.method, pin.key)
          keep(pinned.node, pinned.edge)
          set({ pins: [...get().pins, pin] })
        }
      } catch (reason) {
        set({ notice: `The saved chart no longer matches the engine; it was cut at step ${get().pins.length}.` })
        fail(reason)
        return
      }
      const selected = get().selected
      if (selected) await refresh(selected)
    }

    void restart(start.root, start.watch, start.pins)

    return {
      root: start.root,
      watch: start.watch,
      nodes: [],
      edges: [],
      pins: [],
      selected: null,
      tab: 'forward',
      methods: [],
      method: null,
      applied: null,
      madeBy: null,
      busy: false,
      error: null,
      notice: start.notice,
      mods: [],

      setRoot: (root) => {
        void restart(root, get().watch, [])
      },
      setWatch: (watch) => {
        void restart(get().root, watch, [])
      },
      select: (id) => {
        set({ selected: id, applied: null, madeBy: null, method: null })
        void refresh(id)
      },
      setTab: (tab) => {
        set({ tab })
        const id = get().selected
        if (!id) return
        if (tab === 'backward' && !get().madeBy) {
          env.client
            .chartMadeBy(id, [])
            .then((madeBy) => {
              if (get().selected === id) set({ madeBy })
            })
            .catch(fail)
        }
      },
      chooseMethod: (id) => {
        const node = get().selected
        if (!node) return
        set({ method: id, applied: null })
        env.client
          .chartForward(node, id)
          .then((applied) => {
            if (get().selected === node && get().method === id) set({ applied })
          })
          .catch(fail)
      },
      pinForward: (key) => {
        const { selected, method, pins } = get()
        if (!selected || !method) return
        env.client
          .chartPinForward(selected, method, key)
          .then((pinned) => {
            keep(pinned.node, pinned.edge)
            set({ pins: [...pins, { dir: 'f', node: selected, method, key }] })
            void refresh(pinned.node.id)
          })
          .catch(fail)
      },
      pinBackward: (method, key) => {
        const { selected, pins } = get()
        if (!selected) return
        env.client
          .chartPinBackward(selected, method, key)
          .then((pinned) => {
            keep(pinned.node, pinned.edge)
            set({ pins: [...pins, { dir: 'b', node: selected, method, key }] })
            void refresh(pinned.node.id)
          })
          .catch(fail)
      },
      unpin: (id) => {
        const { nodes, edges, selected, pins } = get()
        if (nodes.length === 0 || nodes[0].id === id) return
        const left = edges.filter((edge) => edge.from !== id && edge.to !== id)
        const rest = nodes.filter((node) => node.id !== id)
        const helped = reachable(rest, left)
        set({
          nodes: rest.filter((node) => helped.has(node.id)),
          edges: left.filter((edge) => helped.has(edge.from) && helped.has(edge.to)),
          pins: pins.filter((pin) => pin.key !== id),
          selected: selected && helped.has(selected) ? selected : nodes[0].id,
        })
      },
      clearChart: () => {
        const { nodes } = get()
        if (nodes.length === 0) return
        set({ nodes: nodes.slice(0, 1), edges: [], pins: [], selected: nodes[0].id })
        void refresh(nodes[0].id)
      },
      setError: (error) => set({ error }),
      setNotice: (notice) => set({ notice }),
      setBusy: (busy) => set({ busy }),
      setMods: (mods) => set({ mods }),
    }
  })

  if (env.storage) {
    const storage = env.storage
    store.subscribe((state, previous) => {
      if (state.root === previous.root && state.watch === previous.watch && state.pins === previous.pins) {
        return
      }
      try {
        storage.setItem(scope.storageKey, JSON.stringify({ root: state.root, watch: state.watch, pins: state.pins }))
      } catch {
        // storage full or unavailable: the chart still lives for this visit
      }
    })
  }
  return store
}
