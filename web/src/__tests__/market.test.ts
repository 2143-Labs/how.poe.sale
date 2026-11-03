import { beforeEach, describe, expect, it } from 'vitest'
import draftJson from '../__fixtures__/simplexDraft.json'
import rawSolution from '../__fixtures__/marketSolution.json'
import { DEFAULT_PRICES } from '../defaultPrices'
import { authoredProposal, marketToFlow, processInputs } from '../market'
import { buildProblem, draftToTarget, minValueFor, normalizeDraft } from '../problem'
import { normalizeSolutionV3 } from '../solution'
import { emptyProblemState, useAppStore } from '../store'
import type { GoodJson, ProblemV3, ProcessJson, RecipeDef } from '../types'

const target = draftToTarget(normalizeDraft(draftJson))!
const solution = normalizeSolutionV3(rawSolution)

function good(id: string, name: string, buy: number | null = null): GoodJson {
  return { id, name, target, buy, sell: null }
}

const processes: ProcessJson[] = [
  {
    id: 'p1',
    output: 'half-a',
    recipe: 'alt-spam',
    kind: { craft: { from: 'base', allowed: ['transmute', 'alteration'] } },
    expanded: null,
    added: [],
    disabled: [],
    buy: [],
    sell: [],
  },
  {
    id: 'p2',
    output: 'main',
    recipe: 'recombine-split',
    kind: { recombine: { a: 'half-a', b: 'base' } },
    expanded: null,
    added: [],
    disabled: [],
    buy: [],
    sell: [],
  },
]

const problem: ProblemV3 = {
  version: 3,
  bundlePatch: '3.29.3.3',
  goods: [good('base', 'Lapis Amulet ilvl 84 normal', 1), good('half-a', 'Magic Lapis Amulet'), good('main', 'Lightning amulet')],
  processes,
  root: 'main',
  focus: null,
  prices: { ...DEFAULT_PRICES },
  donors: [],
  runs: 20000,
  seed: 7,
}

describe('marketToFlow', () => {
  const flow = marketToFlow(solution, problem)

  it('draws one node per good', () => {
    expect(flow.nodes.map((node) => node.id)).toEqual(['base', 'half-a', 'main'])
    expect(flow.nodes.map((node) => node.data.good.name)).toEqual([
      'Lapis Amulet ilvl 84 normal',
      'Magic Lapis Amulet ilvl 84',
      'Lightning amulet',
    ])
  })

  it('draws one edge per input -> output of every process', () => {
    expect(flow.edges).toHaveLength(3)
    expect(flow.edges.map((edge) => edge.id).sort()).toEqual([
      'p1:base->half-a',
      'p2:base->main',
      'p2:half-a->main',
    ])
  })

  it('draws the chosen `via` process solid and the rest dashed', () => {
    const chosen = flow.edges.filter((edge) => edge.data?.chosen)
    expect(chosen.map((edge) => edge.id).sort()).toEqual(['p2:base->main', 'p2:half-a->main'])
    expect(chosen.every((edge) => edge.className?.includes('edge-market-chosen'))).toBe(true)

    const other = flow.edges.find((edge) => edge.id === 'p1:base->half-a')!
    expect(other.className).toContain('edge-market-faint')
    expect(other.data?.chosen).toBe(false)
  })

  it('labels an edge with the recipe name, its probability and its cost', () => {
    const alt = flow.edges.find((edge) => edge.id === 'p1:base->half-a')!
    expect(alt.label).toBe('Transmute, then alteration · 0.0945% · 40 chaos')
    const recombine = flow.edges.find((edge) => edge.id === 'p2:half-a->main')!
    expect(recombine.label).toBe('Recombine two halves · 33.33% · 122.4 chaos')
  })

  it('still draws every good when the solution has no values yet', () => {
    const empty = { ...solution, market: { goods: [], processes: [] } }
    const fallback = marketToFlow(empty, problem)
    expect(fallback.nodes.map((node) => node.id)).toEqual(['base', 'half-a', 'main'])
    expect(fallback.edges).toHaveLength(3)
    expect(fallback.edges.every((edge) => edge.data?.chosen === false)).toBe(true)
  })
})

describe('minValueFor', () => {
  const tier = { min: 12, max: 16 }

  it('takes the tier floor for any roll', () => {
    expect(minValueFor(tier, 'any')).toBe(12)
  })

  it('takes the tier ceiling for a perfect roll', () => {
    expect(minValueFor(tier, 'perfect')).toBe(16)
  })
})

describe('market store', () => {
  beforeEach(() => useAppStore.setState({ ...emptyProblemState(), notice: null }))

  it('writes a typed brick price into the problem it sends to solve', () => {
    useAppStore.getState().applyDraft(normalizeDraft(draftJson))
    const brickId = useAppStore.getState().addBrickGood('main', '+1 all Skill Gems')!
    expect(brickId).toBe('brick-1')

    const before = buildProblem(useAppStore.getState())!
    expect(before.version).toBe(3)
    const brickBefore = before.goods.find((entry) => entry.id === brickId)!
    expect(brickBefore.sell).toBeNull()

    useAppStore.getState().setGoodPrice(brickId, { sell: 250 })
    const after = buildProblem(useAppStore.getState())!
    expect(after.goods.find((entry) => entry.id === brickId)?.sell).toBe(250)
    expect(after.root).toBe('main')
  })

  it('round-trips an authored recipe through the store into the proposal list', () => {
    useAppStore.getState().applyDraft(normalizeDraft(draftJson))
    const recipe: RecipeDef = {
      id: 'authored-chaos',
      name: 'Chaos until it lands',
      kind: 'craft',
      rarity: 'Rare',
      allowed: ['chaos'],
      input: 'normal-base',
    }
    useAppStore.getState().addAuthoredRecipe(recipe)
    expect(useAppStore.getState().authoredRecipes).toEqual([recipe])

    const stored = useAppStore.getState().authoredRecipes[0]
    const mainGood = useAppStore.getState().goods.find((entry) => entry.id === 'main')!
    const proposal = authoredProposal(stored, mainGood)!
    expect(proposal.name).toBe('Chaos until it lands')
    expect(proposal.status).toBe('NeedsInput')
    expect(proposal.cost).toBeNull()
    expect(proposal.newGoods.map((entry) => entry.id)).toEqual(['main-base'])

    // The proposal list helper ranks it in until it has a cost.
    useAppStore.getState().useProcessProposal(proposal)
    const state = useAppStore.getState()
    expect(state.processes.map((entry) => entry.id)).toEqual(['authored-authored-chaos'])
    expect(state.goods.some((entry) => entry.id === 'main-base')).toBe(true)
    expect(state.focus).toBe('authored-authored-chaos')
  })

  it('rejects a target change without a base', () => {
    useAppStore.getState().applyDraft(normalizeDraft(draftJson))
    useAppStore.getState().applyDraft({ ...normalizeDraft(draftJson), base: null })
    expect(useAppStore.getState().goods[0]?.target.base).toBe('Simplex Amulet')
    expect(useAppStore.getState().notice).toMatch(/No base item/)
  })
})

describe('processInputs', () => {
  it('reads a custom craft input', () => {
    const custom = (from: string | null): ProcessJson => ({
      id: 'c',
      output: 'main',
      recipe: null,
      kind: { custom: { from, name: 'x', chance: 0.5, cost: 1, keepsInput: false } },
      expanded: null,
      added: [],
      disabled: [],
      buy: [],
      sell: [],
    })
    expect(processInputs(custom(null))).toEqual([])
    expect(processInputs(custom('x'))).toEqual(['x'])
  })
})
