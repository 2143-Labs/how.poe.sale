import { compressToEncodedURIComponent } from 'lz-string'
import { describe, expect, it } from 'vitest'
import { buildShareUrl, parseShareHash, type ShareScope } from '../share'
import type { AbsSpec, Pin } from '../types'

const root: AbsSpec = {
  base: 'Archdemon Crown',
  itemLevel: 87,
  rarity: 'rare',
  influences: ['shaper'],
  prefixes: [{ id: 'IncreasedLife9', orBetter: true }],
  suffixes: [{ id: 'SocketedGemsDealMoreElementalDamageEssence1' }],
  anyPrefixes: 1,
  anySuffixes: 2,
  corrupted: false,
}

const pins: Pin[] = [
  { dir: 'b', node: 'root', method: 'essence/essence-of-horror', key: 'input' },
  { dir: 'f', node: 'input', method: 'currency/exalted', key: 'root' },
]

describe('share links', () => {
  it.each<ShareScope>(['poe1', 'simple'])('round-trips a %s chart', (scope) => {
    const url = buildShareUrl(scope, { root, watch: [{ id: 'ColdResist6', orBetter: true }], pins }, 'https://how.poe.sale')
    expect(url.startsWith(`https://how.poe.sale/${scope}/#s=`)).toBe(true)
    expect(parseShareHash(url.slice(url.indexOf('#')))).toEqual({
      kind: 'ok',
      scope,
      root,
      watch: [{ id: 'ColdResist6', orBetter: true }],
      pins,
    })
  })

  it('tells an old link from a broken one', () => {
    expect(parseShareHash('#p=abc')).toEqual({ kind: 'legacy' })
    for (const v of [1, 2]) {
      const old = compressToEncodedURIComponent(JSON.stringify({ v, target: root }))
      expect(parseShareHash(`#s=${old}`)).toEqual({ kind: 'legacy' })
    }
    expect(parseShareHash('#s=%%%')).toEqual({ kind: 'invalid' })
    expect(parseShareHash('')).toEqual({ kind: 'none' })
  })
})
