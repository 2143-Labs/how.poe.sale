import { compressToEncodedURIComponent } from 'lz-string'
import { describe, expect, it } from 'vitest'
import { buildShareUrl, parseShareHash, type ShareScope } from '../share'
import type { AbsSpec } from '../types'

const crown: AbsSpec = {
  base: 'Archdemon Crown',
  itemLevel: 87,
  rarity: 'rare',
  influences: ['shaper'],
  prefixes: [{ id: 'IncreasedLife9' }],
  suffixes: [{ id: 'SocketedGemsDealMoreElementalDamageEssence1' }],
  anyPrefixes: 1,
  anySuffixes: 2,
}

describe('share links', () => {
  it.each<ShareScope>(['poe1', 'simple'])('round-trips a %s target', (scope) => {
    const url = buildShareUrl(scope, crown, 'https://how.poe.sale')
    expect(url.startsWith(`https://how.poe.sale/${scope}/#s=`)).toBe(true)
    expect(parseShareHash(url.slice(url.indexOf('#')))).toEqual({ kind: 'ok', scope, target: crown })
  })

  it('tells an old link from a broken one', () => {
    expect(parseShareHash('#p=abc')).toEqual({ kind: 'legacy' })
    const v1 = compressToEncodedURIComponent(JSON.stringify({ v: 1, pattern: '{}', item: 'Rarity: Rare' }))
    expect(parseShareHash(`#s=${v1}`)).toEqual({ kind: 'legacy' })
    expect(parseShareHash('#s=%%%')).toEqual({ kind: 'invalid' })
    expect(parseShareHash('')).toEqual({ kind: 'none' })
  })
})
