import { describe, expect, it } from 'vitest'
import { buildShareUrl, parseShareHash } from '../share'

describe('share links', () => {
  it('round-trips the pattern and the item', () => {
    const url = buildShareUrl('{"name":"crown-shaper"}', 'Item Class: Helmets\nRarity: Rare', 'https://how.poe.sale')
    const hash = url.slice(url.indexOf('#'))
    expect(parseShareHash(hash)).toEqual({
      kind: 'ok',
      pattern: '{"name":"crown-shaper"}',
      item: 'Item Class: Helmets\nRarity: Rare',
    })
  })

  it('tells an old link from a broken one', () => {
    expect(parseShareHash('#p=abc')).toEqual({ kind: 'legacy' })
    expect(parseShareHash('#s=%%%')).toEqual({ kind: 'invalid' })
    expect(parseShareHash('')).toEqual({ kind: 'none' })
  })
})
