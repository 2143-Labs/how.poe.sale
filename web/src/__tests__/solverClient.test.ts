import { describe, expect, it } from 'vitest'
import { createSolverClient, type WorkerLike } from '../solverClient'
import type { SolverRequest, SolverResponse } from '../protocol'
import type { Problem, SolutionV3 } from '../types'

class MockWorker implements WorkerLike {
  readonly messages: SolverRequest[] = []
  private listener: ((event: MessageEvent) => void) | null = null

  postMessage(message: unknown): void {
    this.messages.push(message as SolverRequest)
  }

  addEventListener(_type: string, listener: (event: MessageEvent) => void): void {
    this.listener = listener
  }

  removeEventListener(): void {
    this.listener = null
  }

  respond(response: SolverResponse): void {
    this.listener?.({ data: response } as unknown as MessageEvent)
  }
}

const problem: Problem = {
  version: 1,
  bundlePatch: '3.29.3.3',
  target: {
    base: 'Simplex Amulet',
    ilvlMin: 84,
    influences: ['Shaper', 'Warlord'],
    rarity: 'Rare',
    catalyst: null,
    minHits: 0,
    requirements: [],
  },
  prices: { chaos: 1 },
  donors: [],
  added: [],
  expanded: [],
  disabled: [],
  buy: [],
  sell: [],
  runs: 100,
  seed: 1,
}

function solutionWith(key: string): SolutionV3 {
  return {
    version: 3,
    error: null,
    warnings: [],
    market: {
      goods: [
        {
          id: key,
          name: key,
          value: null,
          sd: null,
          seconds: null,
          via: null,
          needed: false,
          view: { title: key, influences: [], catalyst: null, lock: null, prefixes: [], suffixes: [] },
        },
      ],
      processes: [],
    },
    focus: null,
    proposals: {},
  }
}

function harness() {
  const worker = new MockWorker()
  const solutions: SolutionV3[] = []
  const busy: boolean[] = []
  const errors: (string | null)[] = []
  const client = createSolverClient(worker, {
    onSolution: (solution) => solutions.push(solution),
    onBusy: (value) => busy.push(value),
    onError: (message) => errors.push(message),
  })
  return { worker, client, solutions, busy, errors }
}

describe('solver client', () => {
  it('drops a stale solve response and keeps the newest one', () => {
    const { worker, client, solutions, busy } = harness()
    client.solve(problem)
    client.solve(problem)
    const [first, second] = worker.messages
    expect(worker.messages).toHaveLength(2)

    worker.respond({ type: 'solve', id: first.id, solution: solutionWith('stale') })
    expect(solutions).toHaveLength(0)

    worker.respond({ type: 'solve', id: second.id, solution: solutionWith('fresh') })
    expect(solutions).toHaveLength(1)
    expect(solutions[0].market.goods[0].id).toBe('fresh')
    expect(solutions[0].version).toBe(3)
    expect(busy).toEqual([true, true, false])
  })

  it('keeps the previous solution when a solve fails', () => {
    const { worker, client, solutions, errors } = harness()
    client.solve(problem)
    const [first] = worker.messages
    worker.respond({ type: 'solve', id: first.id, solution: solutionWith('good') })
    expect(solutions).toHaveLength(1)

    client.solve(problem)
    const second = worker.messages[1]
    worker.respond({ type: 'solve', id: second.id, error: 'MissingPrice(wild-lifeforce)' })
    expect(solutions).toHaveLength(1)
    expect(solutions[0].market.goods[0].id).toBe('good')
    expect(errors.at(-1)).toBe('MissingPrice(wild-lifeforce)')
  })

  it('normalises the parse stub into an empty draft', async () => {
    const { worker, client } = harness()
    const pending = client.parse('Item Class: Amulets')
    const request = worker.messages[0]
    worker.respond({ type: 'parse', id: request.id, draft: {} })
    const draft = await pending
    expect(draft.requirements).toEqual([])
    expect(draft.base).toBeNull()
    expect(draft.rarity).toBe('Rare')
  })

  it('answers version and influence-mods requests by id', async () => {
    const { worker, client } = harness()
    const version = client.version()
    worker.respond({ type: 'version', id: worker.messages[0].id, version: '0.1.0 / data 3.29.3.3' })
    expect(await version).toBe('0.1.0 / data 3.29.3.3')

    const mods = client.influenceMods('Simplex Amulet', 'Warlord')
    worker.respond({ type: 'influenceMods', id: worker.messages[1].id, mods: [] })
    expect(await mods).toEqual([])
  })

  it('asks for proposals, contracts, recipes and tiers with the v3 method names', async () => {
    const { worker, client } = harness()

    const proposals = client.proposeProcesses(problem, 'main')
    const proposeRequest = worker.messages[0]
    expect(proposeRequest).toMatchObject({ type: 'proposeProcesses', goodId: 'main' })
    worker.respond({
      type: 'proposeProcesses',
      id: proposeRequest.id,
      proposals: [
        {
          process: {
            id: 'p1',
            output: 'main',
            recipe: 'alt-spam',
            kind: { craft: { from: 'base', allowed: ['transmute', 'alteration'] } },
          },
          newGoods: [],
          name: 'Transmute, then alteration',
          cost: 122,
          pSuccess: 0.0009,
          status: 'Ready',
          reason: null,
        },
      ],
    })
    const list = await proposals
    expect(list).toHaveLength(1)
    expect(list[0].process.id).toBe('p1')
    expect(list[0].process.expanded).toBeNull()
    expect(list[0].status).toBe('Ready')

    const contracts = client.contracts()
    worker.respond({
      type: 'contracts',
      id: worker.messages[1].id,
      contracts: [{ id: 'scour', name: 'Orb of Scouring', requires: 'any', effect: 'clear', cost: [['scour', 1]], seconds: 1 }],
    })
    expect((await contracts)[0].id).toBe('scour')

    const recipes = client.recipes()
    worker.respond({
      type: 'recipes',
      id: worker.messages[2].id,
      recipes: [{ id: 'alt-spam', name: 'Alt spam', kind: 'craft', rarity: 'Magic', allowed: ['alteration'] }],
    })
    expect((await recipes)[0].kind).toBe('craft')

    const tiers = client.modTiers('Lapis Amulet', 'PercentageStrength', 'Suffix')
    expect(worker.messages[3]).toMatchObject({ type: 'modTiers', group: 'PercentageStrength', side: 'Suffix' })
    worker.respond({ type: 'modTiers', id: worker.messages[3].id, tiers: [{ id: 'T1', text: '26%', ilvl: 84, min: 26, max: 28 }] })
    expect(await tiers).toEqual([{ id: 'T1', text: '26%', ilvl: 84, min: 26, max: 28 }])
  })
})
