// Shapes exchanged with the solver (`crates/poe1/src/api.rs`) and the tooltip
// parser (`Bridge::parse_target`). See contract.md §2–3; the solver owns the
// semantics, the UI never re-serialises `action`.

export type Side = 'Prefix' | 'Suffix'

export type Rarity = 'Normal' | 'Magic' | 'Rare'

export const INFLUENCES = ['Shaper', 'Elder', 'Crusader', 'Redeemer', 'Hunter', 'Warlord'] as const

export type Influence = (typeof INFLUENCES)[number]

export interface Requirement {
  group: string
  side: Side
  accepted: string[]
  minValue: number
  label: string
}

export interface CatalystRef {
  name: string
  quality: number
}

/** `Problem.target` — identical to the target half of a `TargetDraft` (contract §3). */
export interface ProblemTarget {
  base: string
  ilvlMin: number
  influences: string[]
  rarity: Rarity
  catalyst: CatalystRef | null
  minHits: number
  requirements: Requirement[]
}

export interface Donor {
  label: string
  base: string
  ilvl: number
  influence: string
  mod: string
  priceChaos: number
}

export interface Problem {
  version: number
  bundlePatch: string
  target: ProblemTarget
  prices: Record<string, number>
  donors: Donor[]
  added: string[]
  expanded: string[]
  disabled: [string, string][]
  buy: [string, number][]
  sell: [string, number][]
  runs: number
  seed: number
}

/** `Bridge::parse_target` JSON (contract §3). */
export interface TargetDraft {
  itemClass: string | null
  base: string | null
  rarity: Rarity
  ilvl: number | null
  influences: string[]
  catalyst: CatalystRef | null
  requirements: Requirement[]
  minHits: number
  buyNote: { amount: number; currency: string } | null
  unmatched: string[]
  warnings: string[]
}

export interface InfluenceModEntry {
  id: string
  text: string
  side: Side
}

export type ChipKind = 'wanted' | 'blocked' | 'other' | 'otherInfluenced'

export interface NodeChip {
  kind: ChipKind
  text: string
  ok: boolean
}

/** Per-node display data (contract §2 `view`). */
export interface NodeView {
  title: string
  influences: string[]
  catalyst: string | null
  lock: string | null
  prefixes: NodeChip[]
  suffixes: NodeChip[]
}

export type SolutionNodeKind = 'start' | 'goal' | 'state'

export interface SolutionNode {
  key: string
  kind: SolutionNodeKind
  state: unknown
  view: NodeView | null
  value: number | null
  expanded: boolean
  inR: boolean
}

export type SolutionEdgeKind = 'craft' | 'buy' | 'sell'

export interface SolutionEdge {
  from: string
  kind: SolutionEdgeKind
  action: unknown
  actionKey: string | null
  label: string
  cost: number
  seconds: number
  outcomes: [string, number][]
  q: number | null
  chosen: boolean
}

export interface Q4 {
  p50: number
  p90: number
  p95: number
  p99: number
}

export interface SolutionStats {
  expectedCost: number
  sdCost: number
  expectedSeconds: number
  expectedSpend: Record<string, number>
  /** chaos spent on Buy edges / recovered on Sell edges, per completed run (both positive) */
  expectedBuyChaos: number
  expectedSellChaos: number
  quantiles: { cost: Q4; seconds: Q4 } | null
}

export type ProposalStatus = 'Ready' | 'NeedsInput' | 'Disabled'

export interface Proposal {
  action: unknown
  actionKey: string
  label: string
  pre: string
  pReach: number
  pMods: number
  cost: number
  q: number | null
  status: ProposalStatus
}

export interface SolutionError {
  kind: string
  message: string
}

export interface Solution {
  version: number
  error: SolutionError | null
  warnings: string[]
  nodes: SolutionNode[]
  edges: SolutionEdge[]
  stats: SolutionStats | null
  proposals: Record<string, Proposal[]>
}
