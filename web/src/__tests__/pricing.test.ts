import { describe, expect, it } from 'vitest'
import draftJson from '../__fixtures__/simplexDraft.json'
import { DEFAULT_PRICES, DEFAULT_PRICES_DATE } from '../defaultPrices'
import pricesJson from '../defaultPrices.json'
import { buildProblem, draftToTarget, normalizeDraft, type ProblemState } from '../problem'

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

function stateWith(overrides: Partial<ProblemState> = {}): ProblemState {
  return {
    target,
    prices: { ...DEFAULT_PRICES },
    donors: [],
    added: [],
    expanded: ['goal'],
    disabled: [],
    buy: {},
    sell: {},
    ...overrides,
  }
}

describe('default prices', () => {
  it('ships the verified 2026-11-02 snapshot', () => {
    expect(DEFAULT_PRICES_DATE).toBe('2026-11-02')
    expect(DEFAULT_PRICES).toEqual(VERIFIED)
  })

  it('is the same table the Rust tests read', () => {
    expect(pricesJson.prices).toEqual(VERIFIED)
  })
})

describe('buildProblem', () => {
  it('forwards edited prices unchanged (clamping is the solver\'s job)', () => {
    const prices = { chaos: 1, divine: 500, 'primal-lifeforce': 0.037 }
    const problem = buildProblem(stateWith({ prices }))!
    expect(problem.prices).toEqual(prices)
  })

  it('is null without a target', () => {
    expect(buildProblem(stateWith({ target: null }))).toBeNull()
  })

  it('sorts buy and sell entries and copies the target', () => {
    const problem = buildProblem(
      stateWith({
        buy: { goal: 311600, 'state:x': 5 },
        sell: { 'state:y': 2000 },
        added: ['state:x'],
        expanded: ['goal', 'state:x'],
        disabled: [['state:x', '{"kind":"Chaos"}']],
      }),
    )!
    expect(problem.version).toBe(1)
    expect(problem.bundlePatch).toBe('3.29.3.3')
    expect(problem.buy).toEqual([
      ['goal', 311600],
      ['state:x', 5],
    ])
    expect(problem.sell).toEqual([['state:y', 2000]])
    expect(problem.disabled).toEqual([['state:x', '{"kind":"Chaos"}']])
    expect(problem.target).toEqual(target)
    expect(problem.added).toEqual(['state:x'])
    expect(problem.expanded).toEqual(['goal', 'state:x'])
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
})
