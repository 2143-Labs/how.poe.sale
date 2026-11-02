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
  /**
   * Display names of the mechanics that can put this mod on the item, in the
   * order the engine reports them (`Native`, `Shaper influence`, `Essence: …`,
   * `Recombine with …`, `Bench`, `Fossil: …`). Drafts always carry it; the
   * solver ignores it on input.
   */
  sources?: string[]
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

// ---------------------------------------------------------------- market (v3)
// Contract v3: a market of goods joined by processes. The v1 `Problem`/`Solution`
// above stay in use for the focus tree and for old share links.

/** One item description the user cares about: a target predicate plus prices. */
export interface GoodJson {
  id: string
  name: string
  target: ProblemTarget
  /** chaos; `null` = cannot be bought */
  buy: number | null
  /** chaos; `null` = not a brick (worth 0 when produced) */
  sell: number | null
}

export type ProcessKindJson =
  | { craft: { from: string | null; allowed: string[] } }
  | { recombine: { a: string; b: string } }

export interface ProcessJson {
  id: string
  output: string
  recipe: string | null
  kind: ProcessKindJson
  /** `null` = the solver builds the forward closure under `allowed` */
  expanded: string[] | null
  added: string[]
  disabled: [string, string][]
  buy: [string, number][]
  sell: [string, number][]
}

export interface ProblemV3 {
  version: 3
  bundlePatch: string
  goods: GoodJson[]
  processes: ProcessJson[]
  root: string
  focus: string | null
  prices: Record<string, number>
  donors: Donor[]
  runs: number
  seed: number
}

/** A problem as written by this UI, or a v1 one read from an old share link. */
export type AnyProblem = Problem | ProblemV3

export interface GoodValueJson {
  id: string
  name: string
  value: number | null
  sd: number | null
  seconds: number | null
  /** `"buy"` | process id | `null` */
  via: string | null
  /** no buy price and no working process, but some process consumes it */
  needed: boolean
  view: NodeView
}

export interface StepJson {
  label: string
  cost: [string, number][]
  changes: string[]
}

export interface ExitJson {
  /** good id the outcome satisfies (output or a brick good); `null` = nothing priced */
  good: string | null
  /** an unpriced good this outcome satisfies (counted at 0) */
  unpriced?: string | null
  p: number
  label: string
  /** the outcome as an item target, for "Make brick a good" */
  target?: ProblemTarget
}

export type ProcessStatus = 'Ready' | 'Unreachable' | 'NeedsInput' | 'Error'

export interface ProcessValueJson {
  id: string
  output: string
  recipe: string | null
  kind: 'craft' | 'recombine'
  inputs: string[]
  name: string
  cost: number | null
  sd: number | null
  seconds: number | null
  pSuccess: number | null
  attempts: number | null
  spend: Record<string, number>
  steps: StepJson[]
  core: ExitJson[]
  exits: ExitJson[]
  status: ProcessStatus
  error: string | null
}

export interface ProcessProposalJson {
  process: ProcessJson
  newGoods: GoodJson[]
  name: string
  cost: number | null
  pSuccess: number | null
  status: 'Ready' | 'NeedsInput' | 'Unreachable' | 'Error'
  reason: string | null
}

export interface SolutionV3 {
  version: 3
  error: SolutionError | null
  warnings: string[]
  market: { goods: GoodValueJson[]; processes: ProcessValueJson[] }
  focus: Solution | null
  proposals: Record<string, ProcessProposalJson[]>
}

export interface ContractJson {
  id: string
  name: string
  requires: string
  effect: string
  cost: [string, number][]
  seconds: number
}

export interface TierJson {
  id: string
  text: string
  ilvl: number
  /** smallest / largest displayed value of this tier on the base */
  min: number
  max: number
}

export interface RecipeDef {
  id: string
  name: string
  kind: 'craft' | 'recombine'
  input?: 'normal-base' | 'same-base'
  rarity: Rarity
  allowed?: string[]
}
