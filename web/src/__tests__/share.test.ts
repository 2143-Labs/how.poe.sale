import { describe, expect, it } from 'vitest'
import draftJson from '../__fixtures__/simplexDraft.json'
import { DEFAULT_PRICES } from '../defaultPrices'
import { buildProblem, draftToTarget, normalizeDraft, stateFromProblem } from '../problem'
import { buildShareUrl, decodeProblem, encodeProblem, parseShareHash, SHARE_PREFIX } from '../share'
import { createSolverClient, type WorkerLike } from '../solverClient'
import type { Problem, ProblemV3 } from '../types'
import type { SolverRequest } from '../protocol'

const target = draftToTarget(normalizeDraft(draftJson))!

const v3: ProblemV3 = buildProblem({
  goods: [
    { id: 'main', name: 'Rare Simplex Amulet ilvl 84', target, buy: 311600.01, sell: null },
    { id: 'base', name: 'Simplex Amulet ilvl 84 normal', target, buy: 5, sell: null },
  ],
  processes: [
    {
      id: 'p1',
      output: 'main',
      recipe: 'reforge',
      kind: { craft: { from: 'base', allowed: ['chaos'] } },
      expanded: null,
      added: [],
      disabled: [],
      buy: [],
      sell: [],
    },
  ],
  root: 'main',
  focus: 'p1',
  prices: { ...DEFAULT_PRICES },
  donors: [
    { label: 'Warlord amulet', base: 'Simplex Amulet', ilvl: 84, influence: 'Warlord', mod: 'Mod1', priceChaos: 545.7 },
  ],
  authoredRecipes: [],
})!

/** A link as it was written before the market: one target, one tree, `version: 1`. */
const v1: Problem = {
  version: 1,
  bundlePatch: '3.29.3.3',
  target,
  prices: { ...DEFAULT_PRICES },
  donors: [],
  added: ['state:locked'],
  expanded: ['goal', 'state:locked'],
  disabled: [['state:miss', '{"kind":"Chaos"}']],
  buy: [['goal', 311600.01]],
  sell: [],
  runs: 20000,
  seed: 7,
}

class MockWorker implements WorkerLike {
  readonly messages: SolverRequest[] = []

  postMessage(message: unknown): void {
    this.messages.push(message as SolverRequest)
  }

  addEventListener(): void {}
  removeEventListener(): void {}
}

describe('share link', () => {
  it('round-trips a v3 problem through the #p= fragment', () => {
    const encoded = encodeProblem(v3)
    expect(decodeProblem(encoded)).toEqual(v3)
  })

  it('builds a #p= URL and reads a v3 problem back', () => {
    const url = buildShareUrl(v3, 'https://how.john2143.com')
    expect(url.startsWith('https://how.john2143.com/#p=')).toBe(true)
    const parsed = parseShareHash(new URL(url).hash)
    expect(parsed.kind).toBe('ok')
    if (parsed.kind === 'ok') expect(parsed.problem).toEqual(v3)
  })

  it('decodes a v1 link unchanged, so the solver can upgrade it', () => {
    const hash = `${SHARE_PREFIX}${encodeProblem(v1)}`
    const parsed = parseShareHash(hash)
    expect(parsed.kind).toBe('ok')
    if (parsed.kind === 'ok') {
      expect(parsed.problem.version).toBe(1)
      expect(parsed.problem).toEqual(v1)
    }
  })

  it('sends a decoded v1 link to solve as-is', () => {
    const parsed = parseShareHash(`${SHARE_PREFIX}${encodeProblem(v1)}`)
    expect(parsed.kind).toBe('ok')
    if (parsed.kind !== 'ok') return

    const worker = new MockWorker()
    const client = createSolverClient(worker, {
      onSolution: () => undefined,
      onBusy: () => undefined,
      onError: () => undefined,
    })
    client.solve(parsed.problem)

    expect(worker.messages).toHaveLength(1)
    const request = worker.messages[0]
    expect(request.type).toBe('solve')
    if (request.type === 'solve') {
      expect(request.problem).toBe(parsed.problem)
      expect(request.problem.version).toBe(1)
    }
  })

  it('reports a missing fragment as none', () => {
    expect(parseShareHash('').kind).toBe('none')
    expect(parseShareHash('#other=1').kind).toBe('none')
  })

  it('reports an undecodable fragment as invalid', () => {
    expect(decodeProblem('this-is-not-lz-string')).toBeNull()
    expect(parseShareHash('#p=this-is-not-lz-string').kind).toBe('invalid')
  })

  it('rejects a v3 problem without goods and a v1 problem without a target', () => {
    const noGoods = encodeProblem({ ...v3, goods: [] } as unknown as ProblemV3)
    expect(decodeProblem(noGoods)).toBeNull()
    const noTarget = encodeProblem({ ...v1, target: null } as unknown as Problem)
    expect(decodeProblem(noTarget)).toBeNull()
  })
})

describe('custom crafts in links', () => {
  it('keep every field through encode, decode and load', () => {
    const custom = {
      ...v3,
      processes: [
        {
          id: 'custom-1',
          output: 'main',
          recipe: null,
          kind: { custom: { from: 'base', name: 'Veiled chaos and unveil', chance: 0.25, cost: 10, keepsInput: true } },
          expanded: null,
          added: [],
          disabled: [],
          buy: [],
          sell: [],
        },
      ],
      focus: null,
    } satisfies ProblemV3
    const decoded = decodeProblem(encodeProblem(custom))
    expect(decoded).not.toBeNull()
    const state = stateFromProblem(decoded!)
    expect(state.processes[0].kind).toEqual({
      custom: { from: 'base', name: 'Veiled chaos and unveil', chance: 0.25, cost: 10, keepsInput: true },
    })
  })
})
