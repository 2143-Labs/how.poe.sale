import { describe, expect, it } from 'vitest'
import draftJson from '../__fixtures__/simplexDraft.json'
import { DEFAULT_PRICES } from '../defaultPrices'
import { buildProblem, draftToTarget, normalizeDraft } from '../problem'
import { buildShareUrl, decodeProblem, encodeProblem, parseShareHash } from '../share'
import type { Problem } from '../types'

const problem = buildProblem({
  target: draftToTarget(normalizeDraft(draftJson)),
  prices: { ...DEFAULT_PRICES },
  donors: [
    { label: 'Warlord amulet', base: 'Simplex Amulet', ilvl: 84, influence: 'Warlord', mod: 'Mod1', priceChaos: 545.7 },
  ],
  added: ['state:locked'],
  expanded: ['goal', 'state:locked'],
  disabled: [['state:miss', '{"kind":"Chaos"}']],
  buy: { goal: 311600.01 },
  sell: {},
})!

describe('share link', () => {
  it('round-trips a problem through the #p= fragment', () => {
    const encoded = encodeProblem(problem)
    const decoded = decodeProblem(encoded)
    expect(decoded).toEqual(problem)
  })

  it('builds a #p= URL and reads it back', () => {
    const url = buildShareUrl(problem, 'https://how.john2143.com')
    expect(url.startsWith('https://how.john2143.com/#p=')).toBe(true)
    const parsed = parseShareHash(new URL(url).hash)
    expect(parsed.kind).toBe('ok')
    if (parsed.kind === 'ok') expect(parsed.problem).toEqual(problem)
  })

  it('reports a missing fragment as none', () => {
    expect(parseShareHash('').kind).toBe('none')
    expect(parseShareHash('#other=1').kind).toBe('none')
  })

  it('reports an undecodable fragment as invalid', () => {
    expect(decodeProblem('this-is-not-lz-string')).toBeNull()
    expect(parseShareHash('#p=this-is-not-lz-string').kind).toBe('invalid')
    const json = encodeProblem({ ...problem, target: null } as unknown as Problem)
    expect(decodeProblem(json)).toBeNull()
  })
})
