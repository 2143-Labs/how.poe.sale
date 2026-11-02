import type { Edge, Node } from '@xyflow/react'
import { formatChaos, formatProbability } from './solution'
import { baseGoodName, cloneTarget } from './problem'
import type {
  GoodJson,
  GoodValueJson,
  ProblemV3,
  ProcessJson,
  ProcessProposalJson,
  RecipeDef,
  SolutionV3,
} from './types'

export type MarketNodeData = {
  good: GoodValueJson
  buy: number | null
  sell: number | null
}

/** React Flow node for one good in the market canvas. */
export type MarketFlowNode = Node<MarketNodeData, 'goodNode'>

export type MarketEdgeData = {
  processId: string
  kind: 'craft' | 'recombine'
  /** the output good's chosen `via` process */
  chosen: boolean
  cost: number | null
  pSuccess: number | null
}

export type MarketFlowEdge = Edge<MarketEdgeData>

/** Goods this process consumes (craft: its start good, recombine: both halves). */
export function processInputs(process: ProcessJson): string[] {
  if ('recombine' in process.kind) return [process.kind.recombine.a, process.kind.recombine.b]
  return process.kind.craft.from == null ? [] : [process.kind.craft.from]
}

function fallbackGood(good: GoodJson): GoodValueJson {
  return {
    id: good.id,
    name: good.name,
    value: null,
    sd: null,
    seconds: null,
    via: null,
    needed: good.buy == null,
    view: {
      title: good.name,
      influences: [...good.target.influences],
      catalyst: good.target.catalyst ? `${good.target.catalyst.name} ${good.target.catalyst.quality}%` : null,
      lock: null,
      prefixes: [],
      suffixes: [],
    },
  }
}

/**
 * The market as a graph: one node per good, one edge per (input good → output
 * good) of every process. The output good's `via` process is drawn solid and
 * every other process dashed, so the chosen policy reads off the canvas.
 */
export function marketToFlow(solution: SolutionV3, problem: ProblemV3): { nodes: MarketFlowNode[]; edges: MarketFlowEdge[] } {
  const values = new Map(solution.market.goods.map((good) => [good.id, good]))
  const goods: GoodValueJson[] =
    solution.market.goods.length > 0
      ? solution.market.goods
      : problem.goods.map(fallbackGood)
  const known = new Set(goods.map((good) => good.id))
  const processes = new Map(solution.market.processes.map((process) => [process.id, process]))

  const nodes: MarketFlowNode[] = goods.map((good) => {
    const from = problem.goods.find((candidate) => candidate.id === good.id)
    return {
      id: good.id,
      type: 'goodNode',
      position: { x: 0, y: 0 },
      data: { good, buy: from?.buy ?? null, sell: from?.sell ?? null },
    }
  })

  const edges: MarketFlowEdge[] = []
  for (const process of problem.processes) {
    const value = processes.get(process.id)
    const kind = 'recombine' in process.kind ? 'recombine' : 'craft'
    const chosen = values.get(process.output)?.via === process.id
    const name = value?.name ?? process.recipe ?? (kind === 'recombine' ? 'Recombine' : 'Craft')
    const label = [
      name,
      value?.pSuccess == null ? null : formatProbability(value.pSuccess),
      value?.cost == null ? null : `${formatChaos(value.cost)} chaos`,
    ]
      .filter((part): part is string => part !== null)
      .join(' · ')

    for (const input of processInputs(process)) {
      if (!known.has(input) || !known.has(process.output) || input === process.output) continue
      edges.push({
        id: `${process.id}:${input}->${process.output}`,
        source: input,
        target: process.output,
        type: 'default',
        label,
        className: `market-edge ${chosen ? 'edge-market-chosen' : 'edge-market-faint'}`,
        data: {
          processId: process.id,
          kind,
          chosen,
          cost: value?.cost ?? null,
          pSuccess: value?.pSuccess ?? null,
        },
      })
    }
  }

  return { nodes, edges }
}

/**
 * An authored recipe turned into a proposal client-side, so it sits in the same
 * list as the engine's proposals. Nothing has been solved yet, hence
 * `NeedsInput` and no cost.
 */
export function authoredProposal(recipe: RecipeDef, good: GoodJson): ProcessProposalJson | null {
  if (recipe.kind !== 'craft') return null
  const id = `${good.id}-base`
  const newGoods: GoodJson[] = []
  if (recipe.input) {
    const target = cloneTarget(good.target)
    target.requirements = []
    target.minHits = 0
    target.catalyst = null
    if (recipe.input === 'normal-base') target.rarity = 'Normal'
    newGoods.push({ id, name: recipe.input === 'normal-base' ? baseGoodName(good.target) : `${good.target.base} base`, target, buy: null, sell: null })
  }
  const process: ProcessJson = {
    id: `authored-${recipe.id}`,
    output: good.id,
    recipe: recipe.id,
    kind: { craft: { from: recipe.input ? id : null, allowed: [...(recipe.allowed ?? [])] } },
    expanded: null,
    added: [],
    disabled: [],
    buy: [],
    sell: [],
  }
  return {
    process,
    newGoods,
    name: recipe.name,
    cost: null,
    pSuccess: null,
    status: 'NeedsInput',
    reason: 'an authored recipe has no value until it is solved',
  }
}
