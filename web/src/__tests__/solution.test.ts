import { describe, expect, it } from 'vitest'
import rawSolution from '../__fixtures__/simplexSolution.json'
import { formatChaos, formatPercent, normalizeSolution, normalizeSolutionV3, recoveryEdgeIds, toFlow } from '../solution'

const solution = normalizeSolution(rawSolution)

describe('normalizeSolution', () => {
  it('turns the Step-2 stub into an empty, renderable solution', () => {
    const stub = normalizeSolution({ version: 1 })
    expect(stub.version).toBe(1)
    expect(stub.nodes).toEqual([])
    expect(stub.edges).toEqual([])
    expect(stub.stats).toBeNull()
    expect(stub.proposals).toEqual({})
    expect(toFlow(stub)).toEqual({ nodes: [], edges: [] })
  })

  it('survives a missing or malformed payload', () => {
    expect(toFlow(normalizeSolution(undefined)).nodes).toEqual([])
    expect(toFlow(normalizeSolution({ version: 1, nodes: 'nope', edges: [null] })).edges).toEqual([])
  })
})

describe('toFlow', () => {
  it('renders the fixture nodes and labels craft edges with p', () => {
    const flow = toFlow(solution)
    expect(flow.nodes.map((node) => node.id)).toEqual([
      'start',
      'goal',
      'state:ideal',
      'state:locked',
      'state:miss',
      'state:brick',
    ])
    const harvest = flow.edges.find((edge) => edge.source === 'state:locked' && edge.target === 'state:ideal')
    expect(harvest?.label).toBe('Harvest: reforge influenced rare · 4.27%')
    expect(harvest?.className).toContain('edge-chosen')

    const miss = flow.edges.find((edge) => edge.source === 'state:locked' && edge.target === 'state:miss')
    expect(miss?.className).toContain('edge-chosen')

    const annul = flow.edges.find((edge) => edge.source === 'state:locked' && edge.target === 'state:brick')
    expect(annul?.className).toContain('edge-faint')

    const buy = flow.edges.find((edge) => edge.data?.kind === 'buy' && edge.source === 'start')
    expect(buy?.label).toBe('Buy')
  })

  it('does not draw the Sell edges (one per node, all back to start)', () => {
    const flow = toFlow(solution)
    expect(flow.edges.every((edge) => edge.data?.kind !== 'sell')).toBe(true)
    expect(flow.edges.some((edge) => edge.target === 'start')).toBe(false)
  })

  it('marks only the edges that close a cycle as recovery', () => {
    const flow = toFlow(solution)
    const loop = flow.edges.find((edge) => edge.source === 'state:miss' && edge.target === 'state:locked')
    expect(loop?.className).toContain('edge-recovery')

    const forward = flow.edges.find((edge) => edge.source === 'state:locked' && edge.target === 'state:ideal')
    expect(forward?.className).not.toContain('edge-recovery')
  })

  it('agrees with recoveryEdgeIds over the drawn edges', () => {
    const visible = { nodes: solution.nodes, edges: solution.edges.filter((edge) => edge.kind !== 'sell') }
    const ids = recoveryEdgeIds(visible)
    expect(ids.size).toBe(1)
    expect(ids.has('e4->state:ideal')).toBe(false)
    expect(ids.has('e6->state:locked')).toBe(true)
  })
})

describe('formatting', () => {
  it('formats probabilities the way the plan quotes them', () => {
    expect(formatPercent(0.0427)).toBe('4.27%')
    expect(formatPercent(0.005051)).toBe('0.51%')
    expect(formatPercent(0.000319)).toBe('0.03%')
    expect(formatPercent(0.0625)).toBe('6.25%')
    expect(formatPercent(1)).toBe('100%')
    expect(formatPercent(0)).toBe('0%')
    expect(formatPercent(Number.NaN)).toBe('—')
  })

  it('formats chaos with thousands separators', () => {
    expect(formatChaos(311600.01)).toBe('311,600')
    expect(formatChaos(58.4)).toBe('58.4')
    expect(formatChaos(273.4625)).toBe('273.46')
    expect(formatChaos(-399.99)).toBe('-399.99')
    expect(formatChaos(null)).toBe('—')
  })
})

describe('normalizeSolutionV3', () => {
  const claim = {
    id: 'recombine.count_table',
    statement: 'How many mods a recombine keeps on one side',
    reason: 'player-collected samples',
    url: 'https://www.reddit.com/r/pathofexile/comments/1exyavx/325_updated_guide_to_recombinators/',
  }

  it('keeps the approximate rules and the value range', () => {
    const s = normalizeSolutionV3({
      version: 3,
      market: { goods: [], processes: [{ id: 'rm', claims: ['recombine.cost', 7] }] },
      approximate: [claim, { id: 'half', statement: 'no reason or url' }],
      range: { low: 10, high: 30, readings: [{ label: 'α 0.4', value: 10 }, { label: 'α 1.6', value: null }, { value: 3 }] },
    })
    expect(s.approximate).toEqual([claim])
    expect(s.range).toEqual({
      low: 10,
      high: 30,
      readings: [
        { label: 'α 0.4', value: 10 },
        { label: 'α 1.6', value: null },
      ],
    })
    expect(s.market.processes[0].claims).toEqual(['recombine.cost'])
  })

  it('defaults both when absent or malformed', () => {
    const absent = normalizeSolutionV3({ version: 3 })
    expect(absent.approximate).toEqual([])
    expect(absent.range).toBeNull()
    const malformed = normalizeSolutionV3({ version: 3, approximate: 'nope', range: { low: 'x', high: 2 } })
    expect(malformed.approximate).toEqual([])
    expect(malformed.range).toBeNull()
  })
})
