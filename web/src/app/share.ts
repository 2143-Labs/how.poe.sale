import { compressToEncodedURIComponent, decompressFromEncodedURIComponent } from 'lz-string'
import type { Accept, GoalSpec, ItemSpec, Prices, SelectorSpec, Setup, Side } from './types'

export const SHARE_PREFIX = '#s='

/** The prefix of links made by the first version of the site, which cannot be opened. */
const LEGACY_PREFIX = '#p='

/** What a link and the saved setup carry; `v` lets an old payload be refused rather than misread. */
export interface SharedSetup extends Setup {
  v: 1
}

export function encodeShare(setup: Setup): string {
  return compressToEncodedURIComponent(JSON.stringify({ v: 1, ...setup } satisfies SharedSetup))
}

export function buildShareUrl(scope: 'poe1' | 'simple', setup: Setup, origin = ''): string {
  return `${origin}/${scope}/${SHARE_PREFIX}${encodeShare(setup)}`
}

/** The last prefix a link or saved setup used, so a page can say why it ignored one. */
export function shareKind(hash: string): 'share' | 'legacy' | null {
  if (hash.startsWith(SHARE_PREFIX)) return 'share'
  if (hash.startsWith(LEGACY_PREFIX)) return 'legacy'
  return null
}

const SIDES: Side[] = ['prefix', 'suffix']
const ACCEPTS: Accept[] = ['has-desired', 'desired-or-good-both-sides', 'any']
const RARITIES: ItemSpec['rarity'][] = ['normal', 'magic', 'rare']

function objectOf(raw: unknown): Record<string, unknown> | null {
  return raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : null
}

function selectorsOf(raw: unknown): SelectorSpec[] | null {
  if (!Array.isArray(raw)) return null
  const out: SelectorSpec[] = []
  for (const entry of raw) {
    const value = objectOf(entry)
    if (!value) return null
    const side = SIDES.find((known) => known === value.side)
    if (!side || typeof value.id !== 'string') return null
    out.push({ side, id: value.id, orBetter: value.orBetter === true })
  }
  return out
}

function itemOf(raw: unknown): ItemSpec | null {
  const value = objectOf(raw)
  if (!value) return null
  const rarity = RARITIES.find((known) => known === value.rarity)
  if (typeof value.base !== 'string' || typeof value.itemLevel !== 'number' || !rarity) return null
  if (!Array.isArray(value.influences) || value.influences.some((name) => typeof name !== 'string')) return null
  return { base: value.base, itemLevel: value.itemLevel, rarity, influences: value.influences }
}

function goalOf(raw: unknown): GoalSpec | null {
  const value = objectOf(raw)
  if (!value) return null
  const desired = selectorsOf(value.desired)
  const good = selectorsOf(value.good)
  const accept = ACCEPTS.find((known) => known === value.accept)
  if (!desired || !good || !accept) return null
  return { desired, good, accept }
}

function pricesOf(raw: unknown): Prices | null {
  const value = objectOf(raw)
  if (!value || typeof value.tradePenalty !== 'number') return null
  const map = objectOf(value.prices)
  if (!map) return null
  const prices: Record<string, number> = {}
  for (const [key, price] of Object.entries(map)) {
    if (typeof price !== 'number' || !Number.isFinite(price)) return null
    prices[key] = price
  }
  return { prices, tradePenalty: value.tradePenalty }
}

/** A setup read back from a link or from storage, or `null` when it does not read as one. */
export function parseSetup(raw: unknown): Setup | null {
  const value = objectOf(raw)
  if (!value || value.v !== 1) return null
  const item = itemOf(value.item)
  const goal = goalOf(value.goal)
  const prices = pricesOf(value.prices)
  if (!item || !goal || !prices || typeof value.depth !== 'number' || !Number.isInteger(value.depth)) return null
  return { item, goal, prices, depth: value.depth }
}

export function parseShareHash(hash: string): Setup | null {
  if (!hash.startsWith(SHARE_PREFIX)) return null
  const json = decompressFromEncodedURIComponent(hash.slice(SHARE_PREFIX.length))
  if (!json) return null
  try {
    return parseSetup(JSON.parse(json))
  } catch {
    return null
  }
}
