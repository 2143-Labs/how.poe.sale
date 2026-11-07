import { createStore, type StoreApi } from 'zustand/vanilla'
import { parseShareHash } from '../share'
import {
  nodeId,
  normalizeSpec,
  type AbsSpec,
  type AbsView,
  type ChartEdge,
  type ChartNode,
  type ModOption,
  type PreimageView,
  type PreimagesView,
} from '../types'
import { HORROR_HELMET, type Scope } from './scope'

export interface FlowState {
  /** The item the chart is built back from, as the panel edits it. */
  spec: AbsSpec
  /** Root first: the last target the engine accepted. */
  chart: ChartNode[]
  edges: ChartEdge[]
  selected: string | null
  preimages: PreimagesView | null
  /** The node `preimages` answers for. */
  preimagesFor: string | null
  busy: boolean
  error: string | null
  /** A share-link problem, shown as a dismissible banner. */
  notice: string | null
  mods: ModOption[]
  setSpec: (spec: AbsSpec) => void
  /** The engine accepted `spec`: the chart starts over from it. */
  accept: (spec: AbsSpec, result: PreimagesView) => void
  select: (id: string) => void
  setPreimages: (id: string, result: PreimagesView) => void
  /** Puts a step's input on the chart under `parentId` and selects it. */
  pin: (parentId: string, preimage: PreimageView) => void
  /** Takes a node off the chart, with every node only it led to. */
  unpin: (id: string) => void
  /** Keeps only the root. */
  clearChart: () => void
  setError: (message: string | null) => void
  setNotice: (message: string | null) => void
  setBusy: (busy: boolean) => void
  setMods: (mods: ModOption[]) => void
}

export type FlowStore = StoreApi<FlowState>

/** What the store reads at start and writes to: the page's fragment and storage. */
export interface StoreEnv {
  hash: string
  storage: Pick<Storage, 'getItem' | 'setItem'> | null
  /** Drops a share fragment from the address bar once it is read. */
  dropHash: () => void
}

/** The target a page opens with when nothing was saved. */
export function seedSpec(scope: Scope): AbsSpec {
  const spec: AbsSpec = {
    base: scope.base ?? 'Archdemon Crown',
    itemLevel: 87,
    rarity: 'rare',
    influences: [],
    prefixes: [],
    suffixes: [],
    anyPrefixes: 0,
    anySuffixes: 0,
  }
  if (scope.id === 'simple') {
    // The essence outcome: Essence of Horror's suffix and four modifiers it does not name.
    spec.anyPrefixes = 2
    spec.anySuffixes = 2
    spec.suffixes = [{ id: HORROR_HELMET }]
  }
  return spec
}

interface Saved {
  spec: AbsSpec
  chart: ChartNode[]
  edges: ChartEdge[]
  selected: string | null
}

function isView(raw: unknown): raw is AbsView {
  if (!raw || typeof raw !== 'object') return false
  const v = raw as AbsView
  return typeof v.base === 'string' && Array.isArray(v.prefixes) && Array.isArray(v.suffixes)
}

function isEdge(raw: unknown): raw is ChartEdge {
  if (!raw || typeof raw !== 'object') return false
  const e = raw as ChartEdge
  return typeof e.from === 'string' && typeof e.to === 'string' && typeof e.method === 'string' && typeof e.p === 'number'
}

function fits(scope: Scope, spec: AbsSpec): boolean {
  return scope.base === null || spec.base === scope.base
}

/** The saved chart, when it parses and belongs to its own target. */
function load(scope: Scope, storage: StoreEnv['storage']): Saved | null {
  try {
    const raw = storage?.getItem(scope.storageKey)
    if (!raw) return null
    const value = JSON.parse(raw) as Partial<Record<keyof Saved, unknown>> | null
    const spec = normalizeSpec(value?.spec)
    if (!value || !spec || !fits(scope, spec)) return null
    const chart: ChartNode[] = (Array.isArray(value.chart) ? value.chart : []).flatMap((raw: unknown) => {
      const node = raw as Partial<ChartNode> | null
      const nodeSpec = normalizeSpec(node?.spec)
      return nodeSpec && isView(node?.view) ? [{ id: nodeId(nodeSpec), view: node.view, spec: nodeSpec }] : []
    })
    if (chart[0]?.id !== nodeId(spec)) return { spec, chart: [], edges: [], selected: null }
    const ids = new Set(chart.map((node) => node.id))
    const edges = (Array.isArray(value.edges) ? value.edges : []).filter(
      (edge: unknown): edge is ChartEdge => isEdge(edge) && ids.has(edge.from) && ids.has(edge.to),
    )
    const selected = typeof value.selected === 'string' && ids.has(value.selected) ? value.selected : chart[0].id
    return { spec, chart, edges, selected }
  } catch {
    return null
  }
}

/** The state a page opens with, and a notice when a share link could not be used. */
function initial(scope: Scope, env: StoreEnv): Saved & { notice: string | null } {
  const saved = () => load(scope, env.storage) ?? { spec: seedSpec(scope), chart: [], edges: [], selected: null }
  const share = parseShareHash(env.hash)
  switch (share.kind) {
    case 'ok':
      env.dropHash()
      if (share.scope !== scope.id || !fits(scope, share.target)) {
        return { ...saved(), notice: 'This share link is for another page.' }
      }
      return { spec: share.target, chart: [], edges: [], selected: null, notice: null }
    case 'legacy':
      return { ...saved(), notice: 'This link was made by the previous version of how.poe.sale and cannot be opened.' }
    case 'invalid':
      return { ...saved(), notice: 'This share link could not be read.' }
    case 'none':
      return { ...saved(), notice: null }
  }
}

/** Every node a chain of pinned steps leads from to the root. */
function reachable(chart: ChartNode[], edges: ChartEdge[]): Set<string> {
  const keep = new Set<string>(chart.length > 0 ? [chart[0].id] : [])
  let grew = true
  while (grew) {
    grew = false
    for (const edge of edges) {
      if (keep.has(edge.to) && !keep.has(edge.from)) {
        keep.add(edge.from)
        grew = true
      }
    }
  }
  return keep
}

export function createFlowStore(scope: Scope, env: StoreEnv): FlowStore {
  const start = initial(scope, env)
  const store = createStore<FlowState>()((set, get) => ({
    ...start,
    preimages: null,
    preimagesFor: null,
    busy: false,
    error: null,
    mods: [],
    setSpec: (spec) => set({ spec }),
    accept: (spec, result) => {
      const id = nodeId(spec)
      set({
        chart: [{ id, view: result.target, spec }],
        edges: [],
        selected: id,
        preimages: result,
        preimagesFor: id,
        error: null,
      })
    },
    select: (id) => set({ selected: id }),
    setPreimages: (id, result) => set({ preimages: result, preimagesFor: id }),
    pin: (parentId, preimage) => {
      const spec = normalizeSpec(preimage.inputSpec)
      if (!spec) return
      const id = nodeId(spec)
      const { chart, edges } = get()
      const nextChart = chart.some((node) => node.id === id) ? chart : [...chart, { id, view: preimage.input, spec }]
      const known = edges.some((e) => e.from === id && e.to === parentId && e.method === preimage.method)
      const nextEdges = known
        ? edges
        : [...edges, { from: id, to: parentId, method: preimage.method, name: preimage.name, p: preimage.p }]
      set({ chart: nextChart, edges: nextEdges, selected: id })
    },
    unpin: (id) => {
      const { chart, edges, selected } = get()
      if (chart.length === 0 || chart[0].id === id) return
      const left = edges.filter((edge) => edge.from !== id && edge.to !== id)
      const keep = reachable(
        chart.filter((node) => node.id !== id),
        left,
      )
      set({
        chart: chart.filter((node) => keep.has(node.id)),
        edges: left.filter((edge) => keep.has(edge.from) && keep.has(edge.to)),
        selected: selected && keep.has(selected) ? selected : chart[0].id,
      })
    },
    clearChart: () => {
      const { chart } = get()
      set({ chart: chart.slice(0, 1), edges: [], selected: chart[0]?.id ?? null })
    },
    setError: (error) => set({ error }),
    setNotice: (notice) => set({ notice }),
    setBusy: (busy) => set({ busy }),
    setMods: (mods) => set({ mods }),
  }))

  if (env.storage) {
    const storage = env.storage
    store.subscribe((state, previous) => {
      if (
        state.spec === previous.spec &&
        state.chart === previous.chart &&
        state.edges === previous.edges &&
        state.selected === previous.selected
      ) {
        return
      }
      try {
        const { spec, chart, edges, selected } = state
        storage.setItem(scope.storageKey, JSON.stringify({ spec, chart, edges, selected }))
      } catch {
        // storage full or unavailable: the chart still lives for this visit
      }
    })
  }
  return store
}
