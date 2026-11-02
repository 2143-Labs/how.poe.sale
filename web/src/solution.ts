import type { Edge, Node } from '@xyflow/react'
import { INFLUENCES } from './types'
import type {
  ContractJson,
  ExitJson,
  GoodJson,
  GoodValueJson,
  NodeView,
  ProblemTarget,
  ProcessJson,
  ProcessProposalJson,
  ProcessValueJson,
  RecipeDef,
  Solution,
  SolutionEdge,
  SolutionError,
  SolutionNode,
  SolutionStats,
  SolutionV3,
  StepJson,
  TierJson,
} from './types'

export type StateNodeData = { node: SolutionNode }

export type FlowNode = Node<StateNodeData, 'solutionNode'>

export type FlowEdgeData = {
  kind: SolutionEdge['kind']
  chosen: boolean
  recovery: boolean
  probability: number
}

export type FlowEdge = Edge<FlowEdgeData>

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : {}
}

function normalizeNode(raw: unknown): SolutionNode | null {
  const n = asRecord(raw)
  if (typeof n.key !== 'string') return null
  const kind = n.kind === 'start' || n.kind === 'goal' ? n.kind : 'state'
  return {
    key: n.key,
    kind,
    state: n.state ?? null,
    view: (n.view as SolutionNode['view']) ?? null,
    value: typeof n.value === 'number' && Number.isFinite(n.value) ? n.value : null,
    expanded: n.expanded === true,
    inR: n.inR !== false,
  }
}

function normalizeEdge(raw: unknown): SolutionEdge | null {
  const e = asRecord(raw)
  if (typeof e.from !== 'string') return null
  const kind = e.kind === 'buy' || e.kind === 'sell' ? e.kind : 'craft'
  const outcomes: [string, number][] = Array.isArray(e.outcomes)
    ? e.outcomes
        .filter((outcome): outcome is [unknown, unknown] => Array.isArray(outcome) && outcome.length === 2)
        .filter(
          (outcome): outcome is [string, number] =>
            typeof outcome[0] === 'string' && typeof outcome[1] === 'number' && Number.isFinite(outcome[1]),
        )
    : []
  return {
    from: e.from,
    kind,
    action: e.action ?? null,
    actionKey: typeof e.actionKey === 'string' ? e.actionKey : null,
    label: typeof e.label === 'string' ? e.label : kind,
    cost: typeof e.cost === 'number' && Number.isFinite(e.cost) ? e.cost : 0,
    seconds: typeof e.seconds === 'number' && Number.isFinite(e.seconds) ? e.seconds : 0,
    outcomes,
    q: typeof e.q === 'number' && Number.isFinite(e.q) ? e.q : null,
    chosen: e.chosen === true,
  }
}

function normalizeStats(raw: unknown): SolutionStats | null {
  const s = asRecord(raw)
  if (Object.keys(s).length === 0) return null
  return {
    expectedCost: typeof s.expectedCost === 'number' ? s.expectedCost : 0,
    sdCost: typeof s.sdCost === 'number' ? s.sdCost : 0,
    expectedSeconds: typeof s.expectedSeconds === 'number' ? s.expectedSeconds : 0,
    expectedSpend: (s.expectedSpend as Record<string, number>) ?? {},
    expectedBuyChaos: typeof s.expectedBuyChaos === 'number' ? s.expectedBuyChaos : 0,
    expectedSellChaos: typeof s.expectedSellChaos === 'number' ? s.expectedSellChaos : 0,
    quantiles: (s.quantiles as SolutionStats['quantiles']) ?? null,
  }
}

/**
 * Tolerates the Step-2 `Bridge::solve` stub (`{"version":1}`) and any partially
 * populated answer, so the UI never crashes on a missing field.
 */
export function normalizeSolution(raw: unknown): Solution {
  const s = asRecord(raw)
  const nodes = Array.isArray(s.nodes)
    ? s.nodes.map(normalizeNode).filter((node): node is SolutionNode => node !== null)
    : []
  return {
    version: typeof s.version === 'number' ? s.version : 1,
    error: (s.error as Solution['error']) ?? null,
    warnings: Array.isArray(s.warnings) ? s.warnings.filter((w): w is string => typeof w === 'string') : [],
    nodes,
    edges: Array.isArray(s.edges)
      ? s.edges.map(normalizeEdge).filter((edge): edge is SolutionEdge => edge !== null)
      : [],
    stats: normalizeStats(s.stats),
    proposals: (s.proposals as Solution['proposals']) ?? {},
  }
}

export function flowEdgeId(edgeIndex: number, target: string): string {
  return `e${edgeIndex}->${target}`
}

/**
 * React Flow edges, one per outcome, labelled `label · p`. Chosen-policy edges
 * are solid, every other edge faint; an edge is `recovery` when it closes a
 * cycle in a depth-first walk from `start`.
 */
export function toFlow(solution: Solution): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const keys = new Set(solution.nodes.map((node) => node.key))
  const nodes: FlowNode[] = solution.nodes.map((node) => ({
    id: node.key,
    type: 'solutionNode',
    position: { x: 0, y: 0 },
    data: { node },
  }))
  // A Sell edge points every node back at `start`; drawing them turns the tree
  // into a hairball, and the option is always present in the node panel.
  const visible = solution.edges.filter((edge) => edge.kind !== 'sell')
  const recovery = recoveryEdgeIds({ nodes: solution.nodes, edges: visible })
  const edges: FlowEdge[] = []
  visible.forEach((edge, index) => {
    for (const [target, probability] of edge.outcomes) {
      if (!keys.has(target)) continue
      const id = flowEdgeId(index, target)
      const isRecovery = recovery.has(id)
      edges.push({
        id,
        source: edge.from,
        target,
        type: 'default',
        label: edge.kind === 'craft' ? `${edge.label} · ${formatPercent(probability)}` : edge.label,
        className: ['flow-edge', edge.chosen ? 'edge-chosen' : 'edge-faint', isRecovery ? 'edge-recovery' : '']
          .filter(Boolean)
          .join(' '),
        data: { kind: edge.kind, chosen: edge.chosen, recovery: isRecovery, probability },
      })
    }
  })
  return { nodes, edges }
}

/** Ids of the `(edge, outcome)` pairs that are back edges of a DFS rooted at `start`. */
export function recoveryEdgeIds(solution: Pick<Solution, 'nodes' | 'edges'>): Set<string> {
  const recovery = new Set<string>()
  const adjacency = new Map<string, { id: string; target: string }[]>()
  for (const node of solution.nodes) adjacency.set(node.key, [])
  solution.edges.forEach((edge, index) => {
    const outgoing = adjacency.get(edge.from)
    if (!outgoing) return
    for (const [target] of edge.outcomes) {
      if (!adjacency.has(target)) continue
      outgoing.push({ id: flowEdgeId(index, target), target })
    }
  })
  if (!adjacency.has('start')) return recovery

  const visited = new Set<string>(['start'])
  const onStack = new Set<string>(['start'])
  const stack: { key: string; next: number }[] = [{ key: 'start', next: 0 }]
  while (stack.length > 0) {
    const frame = stack[stack.length - 1]
    const outgoing = adjacency.get(frame.key) ?? []
    if (frame.next >= outgoing.length) {
      stack.pop()
      onStack.delete(frame.key)
      continue
    }
    const { id, target } = outgoing[frame.next]
    frame.next += 1
    if (onStack.has(target)) {
      recovery.add(id)
    } else if (!visited.has(target)) {
      visited.add(target)
      onStack.add(target)
      stack.push({ key: target, next: 0 })
    }
  }
  return recovery
}

export function formatPercent(p: number): string {
  if (!Number.isFinite(p)) return '—'
  const value = p * 100
  if (value > 0 && value < 0.01) return '<0.01%'
  const text = value.toFixed(2).replace(/\.?0+$/, '')
  return `${text === '' || text === '-0' ? '0' : text}%`
}

/** Whole chaos from 1,000 up; below that keep up to two decimals (58.4 catalysts, 2.72 exalts). */
export function formatChaos(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return '—'
  return value.toLocaleString('en-US', { maximumFractionDigits: Math.abs(value) < 1000 ? 2 : 0 })
}

export function formatDivine(chaos: number | null | undefined, divinePrice: number): string {
  if (chaos == null || !Number.isFinite(chaos) || !Number.isFinite(divinePrice) || divinePrice <= 0) return '—'
  return `${(chaos / divinePrice).toFixed(2)} div`
}

export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds)) return '—'
  if (seconds < 60) return `${seconds.toFixed(1)} s`
  const minutes = seconds / 60
  if (minutes < 60) return `${minutes.toFixed(1)} min`
  return `${(minutes / 60).toFixed(2)} h`
}

// ------------------------------------------------------------------ state keys

type RawSide = {
  wanted?: { req?: number; value_ok?: boolean }[]
  blocked?: unknown[]
  untracked?: number
  untracked_influenced?: number
}

type RawState = {
  influences?: number
  catalyst?: [number, number] | null
  prefixes?: RawSide
  suffixes?: RawSide
  lock?: number
}

function parseStateKey(key: string): RawState | null {
  if (!key.startsWith('{')) return null
  try {
    const state = JSON.parse(key) as RawState | null
    return state && typeof state === 'object' ? state : null
  } catch {
    return null
  }
}

/**
 * Catalyst index → display name, learned from the solution itself (node views
 * and ApplyCatalyst labels); the bundle lives in the worker.
 */
export function catalystNames(solution: Solution): Map<number, string> {
  const names = new Map<number, string>()
  for (const node of solution.nodes) {
    const state = node.state as RawState | null
    const catalyst = state && Array.isArray(state.catalyst) ? state.catalyst : null
    if (catalyst && node.view?.catalyst) names.set(catalyst[0], node.view.catalyst.replace(/\s+\d+%$/, ''))
  }
  const learn = (action: unknown, label: string) => {
    const a = action as { kind?: unknown; catalyst?: unknown } | null
    if (a && a.kind === 'ApplyCatalyst' && typeof a.catalyst === 'number' && !names.has(a.catalyst)) {
      names.set(a.catalyst, label)
    }
  }
  for (const edge of solution.edges) learn(edge.action, edge.label)
  for (const list of Object.values(solution.proposals)) for (const proposal of list) learn(proposal.action, proposal.label)
  return names
}

function influenceNames(bits: number): string[] {
  return INFLUENCES.filter((_, i) => (bits & (1 << i)) !== 0)
}

/**
 * One line describing a state from its key (the `ItemState` JSON), so proposals
 * whose labels are identical — e.g. a reforge and its catalyst-steered twin —
 * can be told apart before the state is in the graph. `null` for `start`/`goal`.
 */
export function summarizeStateKey(
  key: string,
  target: ProblemTarget | null,
  catalysts: Map<number, string> = new Map(),
): string | null {
  const state = parseStateKey(key)
  if (!state) return null
  const side = (name: string, raw: RawSide | undefined) => {
    const parts: string[] = []
    for (const hit of raw?.wanted ?? []) {
      const label = (hit.req != null ? target?.requirements[hit.req]?.label : undefined) ?? 'requirement'
      parts.push(hit.value_ok === false ? `${label} (low roll)` : label)
    }
    const blocked = raw?.blocked?.length ?? 0
    if (blocked > 0) parts.push(`${blocked} other tier`)
    const untracked = raw?.untracked ?? 0
    const influenced = raw?.untracked_influenced ?? 0
    if (untracked - influenced > 0) parts.push(`${untracked - influenced} other`)
    if (influenced > 0) parts.push(`${influenced} other influenced`)
    return `${name}: ${parts.length > 0 ? parts.join(', ') : 'none'}`
  }
  const out = [side('prefixes', state.prefixes), side('suffixes', state.suffixes)]
  const bits = state.influences ?? 0
  const targetBits = (target?.influences ?? []).reduce(
    (acc, name) => acc | (1 << (INFLUENCES as readonly string[]).indexOf(name)),
    0,
  )
  if (target && bits !== targetBits) {
    const influences = influenceNames(bits)
    out.unshift(influences.length > 0 ? influences.join(' + ') : 'no influence')
  }
  if ((state.lock ?? 0) & 1) out.push('Prefixes Cannot Be Changed')
  if ((state.lock ?? 0) & 2) out.push('Suffixes Cannot Be Changed')
  if (Array.isArray(state.catalyst)) {
    out.push(`${catalysts.get(state.catalyst[0]) ?? 'catalyst'} ${state.catalyst[1]}%`)
  }
  return out.join(' · ')
}

// ------------------------------------------------------------------ market v3

function asNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function asString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []
}

function asPairs<T extends string | number>(value: unknown, cast: 'string' | 'number'): [string, T][] {
  if (!Array.isArray(value)) return []
  return value.flatMap((pair) => {
    if (!Array.isArray(pair) || pair.length !== 2) return []
    const [first, second] = pair
    if (typeof first !== 'string') return []
    if (cast === 'number' && (typeof second !== 'number' || !Number.isFinite(second))) return []
    if (cast === 'string' && typeof second !== 'string') return []
    return [[first, second] as unknown as [string, T]]
  })
}

function emptyView(title: string): NodeView {
  return { title, influences: [], catalyst: null, lock: null, prefixes: [], suffixes: [] }
}

function normalizeGoodValue(raw: unknown): GoodValueJson | null {
  const g = asRecord(raw)
  if (typeof g.id !== 'string') return null
  const name = asString(g.name, g.id)
  return {
    id: g.id,
    name,
    value: asNumber(g.value),
    sd: asNumber(g.sd),
    seconds: asNumber(g.seconds),
    via: typeof g.via === 'string' ? g.via : null,
    needed: g.needed === true,
    view: (g.view as GoodValueJson['view']) ?? emptyView(name),
  }
}

function normalizeExit(raw: unknown): ExitJson | null {
  const e = asRecord(raw)
  const p = asNumber(e.p)
  if (p == null) return null
  const target = asRecord(e.target)
  return {
    good: typeof e.good === 'string' ? e.good : null,
    unpriced: typeof e.unpriced === 'string' ? e.unpriced : null,
    p,
    label: asString(e.label, 'other outcome'),
    // The engine's TargetJson for this outcome; trusted as-is (it comes from the same wasm).
    target: Array.isArray(target.requirements) ? (target as unknown as ProblemTarget) : undefined,
  }
}

function normalizeStep(raw: unknown): StepJson | null {
  const s = asRecord(raw)
  if (typeof s.label !== 'string') return null
  return { label: s.label, cost: asPairs<number>(s.cost, 'number'), changes: asStringArray(s.changes) }
}

function normalizeExits(value: unknown): ExitJson[] {
  return Array.isArray(value) ? value.map(normalizeExit).filter((e): e is ExitJson => e !== null) : []
}

function normalizeProcessValue(raw: unknown): ProcessValueJson | null {
  const p = asRecord(raw)
  if (typeof p.id !== 'string') return null
  const status = p.status === 'Unreachable' || p.status === 'NeedsInput' || p.status === 'Error' ? p.status : 'Ready'
  const spend = asRecord(p.spend)
  return {
    id: p.id,
    output: asString(p.output),
    recipe: typeof p.recipe === 'string' ? p.recipe : null,
    kind: p.kind === 'recombine' ? 'recombine' : 'craft',
    inputs: asStringArray(p.inputs),
    name: asString(p.name, p.kind === 'recombine' ? 'Recombine' : 'Craft'),
    cost: asNumber(p.cost),
    sd: asNumber(p.sd),
    seconds: asNumber(p.seconds),
    pSuccess: asNumber(p.pSuccess),
    attempts: asNumber(p.attempts),
    spend: Object.fromEntries(
      Object.entries(spend).flatMap(([key, value]) => {
        const n = asNumber(value)
        return n == null ? [] : [[key, n] as [string, number]]
      }),
    ),
    steps: Array.isArray(p.steps) ? p.steps.map(normalizeStep).filter((s): s is StepJson => s !== null) : [],
    core: normalizeExits(p.core),
    exits: normalizeExits(p.exits),
    status,
    error: typeof p.error === 'string' ? p.error : null,
  }
}

function normalizeProcessJson(raw: unknown): ProcessJson | null {
  const p = asRecord(raw)
  if (typeof p.id !== 'string' || typeof p.output !== 'string') return null
  const kindRaw = asRecord(p.kind)
  let kind: ProcessJson['kind']
  if (kindRaw.recombine) {
    const r = asRecord(kindRaw.recombine)
    if (typeof r.a !== 'string' || typeof r.b !== 'string') return null
    kind = { recombine: { a: r.a, b: r.b } }
  } else {
    const c = asRecord(kindRaw.craft ?? kindRaw)
    kind = {
      craft: { from: typeof c.from === 'string' ? c.from : null, allowed: asStringArray(c.allowed) },
    }
  }
  return {
    id: p.id,
    output: p.output,
    recipe: typeof p.recipe === 'string' ? p.recipe : null,
    kind,
    expanded: Array.isArray(p.expanded) ? asStringArray(p.expanded) : null,
    added: asStringArray(p.added),
    disabled: asPairs<string>(p.disabled, 'string'),
    buy: asPairs<number>(p.buy, 'number'),
    sell: asPairs<number>(p.sell, 'number'),
  }
}

function normalizeGoodJson(raw: unknown): GoodJson | null {
  const g = asRecord(raw)
  if (typeof g.id !== 'string' || !g.target || typeof g.target !== 'object') return null
  return {
    id: g.id,
    name: asString(g.name, g.id),
    target: g.target as GoodJson['target'],
    buy: asNumber(g.buy),
    sell: asNumber(g.sell),
  }
}

function normalizeProposal(raw: unknown): ProcessProposalJson | null {
  const p = asRecord(raw)
  const process = normalizeProcessJson(p.process)
  if (!process) return null
  const status =
    p.status === 'NeedsInput' || p.status === 'Unreachable' ? p.status : ('Ready' as ProcessProposalJson['status'])
  return {
    process,
    newGoods: Array.isArray(p.newGoods)
      ? p.newGoods.map(normalizeGoodJson).filter((g): g is GoodJson => g !== null)
      : [],
    name: asString(p.name, process.id),
    cost: asNumber(p.cost),
    pSuccess: asNumber(p.pSuccess),
    status,
    reason: typeof p.reason === 'string' ? p.reason : null,
  }
}

/** Tolerates a partially populated market answer, so the canvas never crashes on a missing field. */
export function normalizeSolutionV3(raw: unknown): SolutionV3 {
  const s = asRecord(raw)
  const market = asRecord(s.market)
  const proposals: Record<string, ProcessProposalJson[]> = {}
  for (const [goodId, list] of Object.entries(asRecord(s.proposals))) {
    if (!Array.isArray(list)) continue
    proposals[goodId] = list.map(normalizeProposal).filter((p): p is ProcessProposalJson => p !== null)
  }
  return {
    version: 3,
    error: (s.error as SolutionError | null) ?? null,
    warnings: asStringArray(s.warnings),
    market: {
      goods: Array.isArray(market.goods)
        ? market.goods.map(normalizeGoodValue).filter((g): g is GoodValueJson => g !== null)
        : [],
      processes: Array.isArray(market.processes)
        ? market.processes.map(normalizeProcessValue).filter((p): p is ProcessValueJson => p !== null)
        : [],
    },
    focus: s.focus ? normalizeSolution(s.focus) : null,
    proposals,
  }
}

export function normalizeProposalList(raw: unknown): ProcessProposalJson[] {
  return Array.isArray(raw) ? raw.map(normalizeProposal).filter((p): p is ProcessProposalJson => p !== null) : []
}

export function normalizeContracts(raw: unknown): ContractJson[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((entry) => {
    const c = asRecord(entry)
    if (typeof c.id !== 'string') return []
    return [
      {
        id: c.id,
        name: asString(c.name, c.id),
        requires: asString(c.requires),
        effect: asString(c.effect),
        cost: asPairs<number>(c.cost, 'number'),
        seconds: asNumber(c.seconds) ?? 0,
      } satisfies ContractJson,
    ]
  })
}

export function normalizeRecipes(raw: unknown): RecipeDef[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((entry) => {
    const r = asRecord(entry)
    if (typeof r.id !== 'string' || typeof r.name !== 'string') return []
    const kind = r.kind === 'recombine' ? 'recombine' : 'craft'
    const input = r.input === 'normal-base' || r.input === 'same-base' ? r.input : undefined
    const rarity = r.rarity === 'Normal' || r.rarity === 'Magic' || r.rarity === 'Rare' ? r.rarity : 'Rare'
    return [
      {
        id: r.id,
        name: r.name,
        kind,
        rarity,
        ...(input ? { input } : {}),
        ...(Array.isArray(r.allowed) ? { allowed: asStringArray(r.allowed) } : {}),
      } satisfies RecipeDef,
    ]
  })
}

export function normalizeTiers(raw: unknown): TierJson[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((entry) => {
    const t = asRecord(entry)
    if (typeof t.id !== 'string') return []
    return [
      {
        id: t.id,
        text: asString(t.text, t.id),
        ilvl: asNumber(t.ilvl) ?? 0,
        min: asNumber(t.min) ?? 0,
        max: asNumber(t.max) ?? (asNumber(t.min) ?? 0),
      } satisfies TierJson,
    ]
  })
}

/** Four significant digits, as the market tables quote probabilities. */
export function formatProbability(p: number | null | undefined): string {
  if (p == null || !Number.isFinite(p)) return '—'
  const value = p * 100
  if (value > 0 && value < 0.0001) return '<0.0001%'
  return `${Number(value.toPrecision(4))}%`
}
