import { createStore, type StoreApi } from 'zustand/vanilla'
import type { Client } from '../client'
import { parseShareHash } from '../share'
import { applyMerges, autoOrder, foldShape, mergesOf, resolveId } from './merge'
import { rowsDiffer, rowsOf, type Row } from './rows'
import {
  normalizeSpec,
  type AbsSpec,
  type AppliedView,
  type EdgeView,
  type MadeByView,
  type MethodOption,
  type ModOption,
  type NodeView,
  type Merge,
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
  /** The root first, then every node pinned since, as shown: merged cards folded into the card they merged into. */
  nodes: NodeView[]
  /** The pinned steps as shown, between the shown nodes. */
  edges: EdgeView[]
  /** Every node the engine made for this chart, merged or not, in pin order. */
  rawNodes: NodeView[]
  /** Every pinned step between raw nodes. */
  rawEdges: EdgeView[]
  merges: Merge[]
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
  /** Pins each outcome of the method shown for the selected node, in order, keeping the selection. */
  pinAll: (keys: string[]) => Promise<void>
  /** Like `pinAll`, then merges each new card into the most specific card the rule and the guard allow. */
  pinAllMerging: (keys: string[]) => Promise<void>
  /** Pins an input a method could have started from, above the selected node. */
  pinBackward: (method: string, key: string) => void
  /** Takes a node off the chart, with every node only it led to. */
  unpin: (id: string) => void
  /**
   * Folds the card `from` into the card `into` when the cards, and where the fold gives a card a
   * plan to follow the engine, agree; otherwise sets `error`.
   */
  merge: (from: string, into: string) => Promise<void>
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
  merges: Merge[]
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
    return { root, watch: watchOf(value.watch), pins: pinsOf(value.pins), merges: mergesOf(value.merges) }
  } catch {
    return null
  }
}

/** The state a page opens with, and a notice when a share link could not be used. */
function initial(scope: Scope, env: StoreEnv): Saved & { notice: string | null } {
  const saved = (): Saved =>
    load(scope, env.storage) ?? { root: seedRoot(scope), watch: [], pins: [], merges: [] }
  const share = parseShareHash(env.hash)
  switch (share.kind) {
    case 'ok':
      env.dropHash()
      if (share.scope !== scope.id) {
        return { ...saved(), notice: 'This share link is for another page.' }
      }
      return { root: share.root, watch: share.watch, pins: share.pins, merges: share.merges, notice: null }
    case 'legacy':
      return { ...saved(), notice: 'This link was made by the previous version of how.poe.sale and cannot be opened.' }
    case 'invalid':
      return { ...saved(), notice: 'This share link could not be read.' }
    case 'none':
      return { ...saved(), notice: null }
  }
}

/** Every node still joined to the root by pinned steps or merges. */
function reachable(nodes: NodeView[], links: { from: string; to: string }[]): Set<string> {
  const keep = new Set<string>(nodes.length > 0 ? [nodes[0].id] : [])
  let grew = true
  while (grew) {
    grew = false
    for (const edge of links) {
      if (keep.has(edge.to) !== keep.has(edge.from)) {
        keep.add(edge.from)
        keep.add(edge.to)
        grew = true
      }
    }
  }
  return keep
}

/**
 * The pins whose ends both stay on the chart, in an order a replay can follow: each starts from
 * the root or from a node an earlier kept pin made. A pin whose start no kept pin makes is dropped.
 */
function replayable(pins: Pin[], root: string, kept: Set<string>): Pin[] {
  const live = pins.filter((pin) => kept.has(pin.node) && kept.has(pin.key))
  const made = new Set([root])
  const out: Pin[] = []
  let grew = true
  while (grew) {
    grew = false
    for (const pin of live) {
      if (out.includes(pin) || !made.has(pin.node)) continue
      out.push(pin)
      made.add(pin.key)
      grew = true
    }
  }
  return out
}

export function createFlowStore(scope: Scope, env: StoreEnv): FlowStore {
  const start = initial(scope, env)
  const store = createStore<FlowState>()((set, get) => {
    const fail = (reason: unknown) =>
      set({ error: reason instanceof Error ? reason.message : String(reason) })

    /** The state for selecting `id`: a different node starts with no methods, chosen method or answers until `refresh` fills them. */
    const moveTo = (id: string): Partial<FlowState> =>
      get().selected === id ? { selected: id } : { selected: id, methods: [], method: null, applied: null, madeBy: null }

    /** The chart lists after a change: the raw ones as given, and the shown ones made from them. */
    const shown = (
      rawNodes: NodeView[],
      rawEdges: EdgeView[],
      merges: Merge[],
    ): Pick<FlowState, 'rawNodes' | 'rawEdges' | 'merges' | 'nodes' | 'edges'> => ({
      rawNodes,
      rawEdges,
      merges,
      ...applyMerges(rawNodes, rawEdges, merges),
    })

    /** Adds a pinned node and edge, when they are new, and selects the card the node is shown as unless told not to. */
    const keep = (node: NodeView, edge: EdgeView, select = true) => {
      const { rawNodes, rawEdges, merges } = get()
      set({
        ...shown(
          rawNodes.some((known) => known.id === node.id) ? rawNodes : [...rawNodes, node],
          rawEdges.some((known) => known.from === edge.from && known.to === edge.to && known.method === edge.method)
            ? rawEdges
            : [...rawEdges, edge],
          merges,
        ),
        ...(select ? moveTo(resolveId(merges, node.id)) : {}),
      })
    }

    /** The cards the chart still has steps pinned from: the ones a craft goes on from. */
    const planned = (id: string) => get().rawEdges.some((edge) => edge.from === id)

    /** Why the chart cannot fold `from` into `into` at all, from the cards alone. */
    const shapeError = (from: string, into: string): string | null => {
      const { nodes } = get()
      if (nodes.length > 0 && nodes[0].id === from) return 'The root cannot be merged into another item.'
      const a = nodes.find((node) => node.id === from)
      const b = nodes.find((node) => node.id === into)
      if (!a || !b || from === into) return 'Only two different items on the chart can be merged.'
      const shape = foldShape(a, b, planned)
      return shape ? `These items cannot be merged: ${shape}.` : null
    }

    /** Folds `from` into `into`. Only called once the rules allow it. */
    const foldNow = (from: string, into: string) => {
      const { rawNodes, rawEdges, merges } = get()
      set(shown(rawNodes, rawEdges, [...merges, { from, into }]))
    }

    /** The rows a method leaves from a card, fetched once per card and method. */
    const fetched = new Map<string, Promise<Row>>()
    const rowsFor = (node: string, method: string) => {
      const key = `${node} ${method}`
      let pending = fetched.get(key)
      if (!pending) {
        pending = env.client.chartForward(node, method).then(rowsOf)
        pending.catch(() => fetched.delete(key))
        fetched.set(key, pending)
      }
      return pending
    }

    /**
     * Why the engine says the two cards differ, or `null` when their odds agree under every method
     * the chart uses on either: the test that makes a fold exact rather than a claim.
     */
    const rowsError = async (from: string, into: string): Promise<string | null> => {
      const { rawEdges } = get()
      const methods = [...new Set(rawEdges.filter((e) => e.from === from || e.from === into).map((e) => e.method))]
      for (const method of methods) {
        const name = rawEdges.find((edge) => edge.method === method)?.name ?? method
        try {
          // One at a time: the page's client answers only the newest forward, so a pair in flight
          // together would cancel each other.
          const left = await rowsFor(from, method)
          const right = await rowsFor(into, method)
          const diff = rowsDiffer(left, right)
          if (diff) {
            return `These items cannot be merged: under ${name} their odds differ by ${(diff.gap * 100).toFixed(1)} points at ${diff.at}.`
          }
        } catch {
          return `These items cannot be merged: ${name} could not be compared.`
        }
      }
      return null
    }

    /** Why the fold would drop a modifier the chart tracks, or `null` when it drops none. */
    const trackedError = (from: string, into: string): string | null => {
      const { nodes, root, watch } = get()
      const tracked = new Set([...root.prefixes, ...root.suffixes, ...watch].map((slot) => slot.id))
      if (tracked.size === 0) return null
      const own = normalizeSpec(nodes.find((node) => node.id === from)?.spec)
      const target = normalizeSpec(nodes.find((node) => node.id === into)?.spec)
      if (!own || !target) return null
      const on = (side: SlotSpec[], slot: SlotSpec) => side.some((other) => other.id === slot.id)
      const lost =
        own.prefixes.some((slot) => tracked.has(slot.id) && !on(target.prefixes, slot)) ||
        own.suffixes.some((slot) => tracked.has(slot.id) && !on(target.suffixes, slot))
      return lost ? 'These items cannot be merged: it would drop a modifier the chart tracks.' : null
    }

    /** Folds `from` into `into` once the cards and, where it matters, the engine agree. */
    const tryMerge = async (from: string, into: string): Promise<string | null> => {
      const shape = shapeError(from, into)
      if (shape) return shape
      const lost = trackedError(from, into)
      if (lost) return lost
      // Two cards nothing is pinned from are destinations: they must be the same item, and no odds
      // are involved. Anything else gives a card a plan to follow, and that plan has to be the same.
      if (planned(from) || planned(into)) {
        const differ = await rowsError(from, into)
        if (differ) return differ
      }
      foldNow(from, into)
      return null
    }

    /**
     * Pins each outcome of `method` from `node` that is not pinned yet, in order, keeping the
     * selection. Gives the nodes that were new to the chart, or `null` when a pin failed.
     */
    const pinEach = async (node: string, method: string, keys: string[]): Promise<string[] | null> => {
      const fresh: string[] = []
      for (const key of keys) {
        const pinned = get().rawEdges.some((e) => e.from === node && e.to === key && e.method === method)
        if (pinned) continue
        try {
          const step = await env.client.chartPinForward(node, method, key)
          if (!get().rawNodes.some((known) => known.id === step.node.id)) fresh.push(step.node.id)
          keep(step.node, step.edge, false)
          set({ pins: [...get().pins, { dir: 'f', node, method, key }] })
        } catch (reason) {
          fail(reason)
          return null
        }
      }
      return fresh
    }

    /** The methods the selected node can be pushed through, and its inputs when it shows them. */
    const refresh = async (id: string) => {
      const { tab } = get()
      try {
        const methods = await env.client.chartMethods(id)
        if (get().selected !== id) return
        set({ methods })
        if (tab === 'backward') {
          const madeBy = await env.client.chartMadeBy(id, [])
          if (get().selected === id) set({ madeBy })
        }
      } catch (reason) {
        fail(reason)
      }
    }

    /** Starts the engine's chart afresh from `root` and `watch`, then replays `pins` and `merges`. */
    const restart = async (root: AbsSpec, watch: SlotSpec[], pins: Pin[], merges: Merge[]) => {
      set({
        root,
        watch,
        pins: [],
        ...shown([], [], []),
        selected: null,
        methods: [],
        method: null,
        applied: null,
        madeBy: null,
      })
      try {
        const node = await env.client.chartStart(root, watch)
        set({ ...shown([node], [], []), selected: node.id })
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
      // Saved merges were checked against the engine when they were made; a forward costs 60 to
      // 300 ms, so a reload only checks the cards, not the odds. A link from an older build keeps
      // its folds until one is made again.
      const dropped = merges.filter((merge) => {
        const shape = shapeError(merge.from, merge.into)
        if (shape) return true
        foldNow(merge.from, merge.into)
        return false
      }).length
      if (dropped > 0) {
        set({
          notice: `${dropped} saved ${dropped === 1 ? 'merge no longer applies and was' : 'merges no longer apply and were'} dropped.`,
        })
      }
      const selected = get().selected
      if (selected) {
        const at = resolveId(get().merges, selected)
        if (at !== selected) set(moveTo(at))
        await refresh(at)
      }
    }

    void restart(start.root, start.watch, start.pins, start.merges)

    return {
      root: start.root,
      watch: start.watch,
      nodes: [],
      edges: [],
      rawNodes: [],
      rawEdges: [],
      merges: [],
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
        void restart(root, get().watch, [], [])
      },
      setWatch: (watch) => {
        void restart(get().root, watch, [], [])
      },
      select: (id) => {
        set({ ...moveTo(id), method: null, applied: null, madeBy: null })
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
        const { selected, method } = get()
        if (!selected || !method) return
        env.client
          .chartPinForward(selected, method, key)
          .then((pinned) => {
            keep(pinned.node, pinned.edge)
            set({ pins: [...get().pins, { dir: 'f', node: selected, method, key }] })
            void refresh(resolveId(get().merges, pinned.node.id))
          })
          .catch(fail)
      },
      pinAll: async (keys) => {
        const { selected, method } = get()
        if (!selected || !method) return
        await pinEach(selected, method, keys)
      },
      pinAllMerging: async (keys) => {
        const { selected, method, root, watch } = get()
        if (!selected || !method) return
        const tracked = new Set([...root.prefixes, ...root.suffixes, ...watch].map((slot) => slot.id))
        const fresh = await pinEach(selected, method, keys)
        for (const id of fresh ?? []) {
          const node = get().nodes.find((known) => known.id === id)
          if (!node) continue
          for (const target of autoOrder(node, get().nodes, tracked)) {
            if ((await tryMerge(node.id, target.id)) === null) break
          }
        }
      },
      pinBackward: (method, key) => {
        const { selected } = get()
        if (!selected) return
        env.client
          .chartPinBackward(selected, method, key)
          .then((pinned) => {
            keep(pinned.node, pinned.edge)
            set({ pins: [...get().pins, { dir: 'b', node: selected, method, key }] })
            void refresh(resolveId(get().merges, pinned.node.id))
          })
          .catch(fail)
      },
      unpin: (id) => {
        const { rawNodes, rawEdges, merges, selected, pins } = get()
        if (rawNodes.length === 0 || rawNodes[0].id === id) return
        const root = rawNodes[0].id
        const gone = new Set([id, ...rawNodes.filter((node) => resolveId(merges, node.id) === id).map((node) => node.id)])
        const rest = rawNodes.filter((node) => !gone.has(node.id))
        const left = rawEdges.filter((edge) => !gone.has(edge.from) && !gone.has(edge.to))
        const folds = merges.filter((merge) => !gone.has(merge.from) && !gone.has(merge.into))
        // a merge joins its two cards as surely as a pinned step does
        const joined = reachable(rest, [...left, ...folds.map((merge) => ({ from: merge.from, to: merge.into }))])
        const kept = replayable(pins, root, joined)
        const made = new Set([root, ...kept.map((pin) => pin.key)])
        const next = shown(
          rest.filter((node) => made.has(node.id)),
          left.filter((edge) => made.has(edge.from) && made.has(edge.to)),
          folds.filter((merge) => made.has(merge.from) && made.has(merge.into)),
        )
        const at = selected ? resolveId(next.merges, selected) : root
        const stay = next.nodes.some((node) => node.id === at) ? at : root
        set({ ...next, pins: kept, ...moveTo(stay) })
        if (stay !== selected) void refresh(stay)
      },
      merge: async (from, into) => {
        const error = await tryMerge(from, into)
        if (error) {
          set({ error })
          return
        }
        if (get().selected === from) {
          set(moveTo(into))
          void refresh(into)
        }
      },
      clearChart: () => {
        const { rawNodes } = get()
        if (rawNodes.length === 0) return
        set({
          ...shown(rawNodes.slice(0, 1), [], []),
          pins: [],
          ...moveTo(rawNodes[0].id),
          method: null,
          applied: null,
        })
        void refresh(rawNodes[0].id)
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
      if (
        state.root === previous.root &&
        state.watch === previous.watch &&
        state.pins === previous.pins &&
        state.merges === previous.merges
      ) {
        return
      }
      try {
        storage.setItem(
          scope.storageKey,
          JSON.stringify({ root: state.root, watch: state.watch, pins: state.pins, merges: state.merges }),
        )
      } catch {
        // storage full or unavailable: the chart still lives for this visit
      }
    })
  }
  return store
}
