import { describe, expect, it } from 'vitest'
import draftJson from '../__fixtures__/simplexDraft.json'
import { DEFAULT_PRICES, DEFAULT_PRICES_DATE } from '../defaultPrices'
import pricesJson from '../defaultPrices.json'
import { buildProblem, draftToTarget, normalizeDraft, type MarketState } from '../problem'
import type { GoodJson } from '../types'

/** The Allflame snapshot from the plan's Verified facts (2026-11-02). */
const VERIFIED: Record<string, number> = {
  chaos: 1,
  divine: 389.5,
  exalted: 2.72,
  annul: 15.55,
  alt: 0.1151,
  aug: 0.007589,
  transmute: 0.0021,
  regal: 0.3541,
  scour: 0.5296,
  'awakeners-orb': 879.5,
  'warlords-exalted-orb': 545.7,
  'hunters-exalted-orb': 540,
  'crusaders-exalted-orb': 268.8,
  'redeemers-exalted-orb': 119.7,
  'shapers-exalted-orb': 168.2,
  'elders-exalted-orb': 81.66,
  'wild-lifeforce': 0.01887,
  'vivid-lifeforce': 0.1811,
  'primal-lifeforce': 0.04359,
  'sacred-lifeforce': 55.51,
  'abrasive-catalyst': 1.36,
  'accelerating-catalyst': 3.03,
  'fertile-catalyst': 31.63,
  'imbued-catalyst': 1.16,
  'intrinsic-catalyst': 2.92,
  'noxious-catalyst': 1.49,
  'prismatic-catalyst': 72.81,
  'tempering-catalyst': 6.45,
  'turbulent-catalyst': 1.51,
  'unstable-catalyst': 4.6,
}

const target = draftToTarget(normalizeDraft(draftJson))!

const main: GoodJson = { id: 'main', name: 'Rare Simplex Amulet ilvl 84', target, buy: null, sell: null }

function stateWith(overrides: Partial<MarketState> = {}): MarketState {
  return {
    goods: [main],
    processes: [],
    root: 'main',
    focus: null,
    prices: { ...DEFAULT_PRICES },
    donors: [],
    authoredRecipes: [],
    ...overrides,
  }
}

describe('default prices', () => {
  it('keeps the verified 2026-11-02 snapshot for the M1 currencies', () => {
    expect(DEFAULT_PRICES_DATE).toBe('2026-11-02')
    // Later milestones add essence, fossil and typed keys: the verified rows must not move.
    expect(DEFAULT_PRICES).toMatchObject(VERIFIED)
  })

  it('is the same table the Rust tests read', () => {
    expect(pricesJson.prices).toEqual(DEFAULT_PRICES)
    expect(pricesJson.prices).toMatchObject(VERIFIED)
  })
})

describe('buildProblem', () => {
  it('forwards edited prices unchanged (clamping is the solver\'s job)', () => {
    const prices = { chaos: 1, divine: 500, 'primal-lifeforce': 0.037 }
    const problem = buildProblem(stateWith({ prices }))!
    expect(problem.prices).toEqual(prices)
  })

  it('is null without a good', () => {
    expect(buildProblem(stateWith({ goods: [] }))).toBeNull()
    expect(buildProblem(stateWith({ root: null }))).toBeNull()
  })

  it('emits a v3 problem with the goods, the root and the ignored node keys', () => {
    const problem = buildProblem(
      stateWith({
        goods: [main, { id: 'brick-1', name: 'brick', target, buy: null, sell: 30 }],
        processes: [
          {
            id: 'p1',
            output: 'main',
            recipe: null,
            kind: { craft: { from: null, allowed: [] } },
            expanded: ['goal', 'state:x'],
            added: [],
            disabled: [['state:x', '{"kind":"Chaos"}']],
            buy: [['goal', 311600]],
            sell: [],
          },
        ],
        focus: 'p1',
      }),
    )!
    expect(problem.version).toBe(3)
    expect(problem.bundlePatch).toBe('3.29.3.3')
    expect(problem.root).toBe('main')
    expect(problem.focus).toBe('p1')
    expect(problem.goods.map((good) => good.id)).toEqual(['main', 'brick-1'])
    expect(problem.goods[1].sell).toBe(30)
    expect(problem.processes[0].expanded).toEqual(['goal', 'state:x'])
    expect(problem.processes[0].disabled).toEqual([['state:x', '{"kind":"Chaos"}']])
    expect(problem.processes[0].buy).toEqual([['goal', 311600]])
  })

  it('drops donors that cannot be resolved by name', () => {
    const problem = buildProblem(
      stateWith({
        donors: [
          { label: 'complete', base: 'Simplex Amulet', ilvl: 84, influence: 'Warlord', mod: 'Mod1', priceChaos: 100 },
          { label: '', base: 'Simplex Amulet', ilvl: 84, influence: 'Warlord', mod: 'Mod1', priceChaos: 100 },
          { label: 'no mod', base: 'Simplex Amulet', ilvl: 84, influence: 'Warlord', mod: '', priceChaos: 100 },
        ],
      }),
    )!
    expect(problem.donors).toHaveLength(1)
    expect(problem.donors[0].label).toBe('complete')
  })

  it('copies the goods so a later edit cannot mutate the sent problem', () => {
    const state = stateWith({ goods: [{ ...main, target: structuredClone(main.target) }] })
    const problem = buildProblem(state)!
    state.goods[0].target.requirements[0].minValue = 999
    expect(problem.goods[0].target.requirements[0].minValue).not.toBe(999)
  })
})
