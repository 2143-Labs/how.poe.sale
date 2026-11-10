import { compressToEncodedURIComponent, decompressFromEncodedURIComponent } from 'lz-string'
import { mergesOf } from './flow/merge'
import { normalizeSpec, type AbsSpec, type Merge, type Pin, type SlotSpec } from './types'

export const SHARE_PREFIX = '#s='

/** The prefix of links made by the first version of the site, which cannot be opened. */
const LEGACY_PREFIX = '#p='

/** The pages a share link can open. */
export type ShareScope = 'poe1' | 'simple'

const SCOPES: ShareScope[] = ['poe1', 'simple']

/** What a chart page needs to rebuild itself from a link. */
export interface ChartShare {
  root: AbsSpec
  watch: SlotSpec[]
  pins: Pin[]
  merges: Merge[]
}

export type ShareParse =
  | { kind: 'none' }
  | ({ kind: 'ok'; scope: ShareScope } & ChartShare)
  | { kind: 'legacy' }
  | { kind: 'invalid' }

function pinsOf(raw: unknown): Pin[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((entry: unknown) => {
    if (!entry || typeof entry !== 'object') return []
    const value = entry as Record<string, unknown>
    if (value.dir !== 'f' && value.dir !== 'b') return []
    if (typeof value.node !== 'string' || typeof value.method !== 'string' || typeof value.key !== 'string') return []
    return [{ dir: value.dir, node: value.node, method: value.method, key: value.key } satisfies Pin]
  })
}

function slotsOf(raw: unknown): SlotSpec[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((entry: unknown) => {
    if (!entry || typeof entry !== 'object' || !('id' in entry) || typeof entry.id !== 'string') return []
    return 'orBetter' in entry && entry.orBetter === true
      ? [{ id: entry.id, orBetter: true }]
      : [{ id: entry.id }]
  })
}

/** The page, its root and its pinned steps, in one URL fragment. */
export function encodeShare(scope: ShareScope, share: ChartShare): string {
  return compressToEncodedURIComponent(JSON.stringify({ v: 3, scope, ...share }))
}

/** Distinguishes "no shared state", "a link from an older version" and "a link that will not decode". */
export function parseShareHash(hash: string): ShareParse {
  if (hash.startsWith(LEGACY_PREFIX)) return { kind: 'legacy' }
  if (!hash.startsWith(SHARE_PREFIX)) return { kind: 'none' }
  let json: string | null
  try {
    json = decompressFromEncodedURIComponent(hash.slice(SHARE_PREFIX.length))
  } catch {
    return { kind: 'invalid' }
  }
  if (!json) return { kind: 'invalid' }
  try {
    const value = JSON.parse(json) as Record<string, unknown> | null
    if (!value || typeof value !== 'object') return { kind: 'invalid' }
    // A pattern, or a target from the one-step table.
    if (value.v === 1 || value.v === 2) return { kind: 'legacy' }
    const root = normalizeSpec(value.root)
    if (value.v !== 3 || !SCOPES.includes(value.scope as ShareScope) || !root) return { kind: 'invalid' }
    return {
      kind: 'ok',
      scope: value.scope as ShareScope,
      root,
      watch: slotsOf(value.watch),
      pins: pinsOf(value.pins),
      merges: mergesOf(value.merges),
    }
  } catch {
    return { kind: 'invalid' }
  }
}

export function buildShareUrl(scope: ShareScope, share: ChartShare, origin = ''): string {
  return `${origin}/${scope}/${SHARE_PREFIX}${encodeShare(scope, share)}`
}
