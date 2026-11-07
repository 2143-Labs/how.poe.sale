import { compressToEncodedURIComponent, decompressFromEncodedURIComponent } from 'lz-string'
import { normalizeSpec, type AbsSpec } from './types'

export const SHARE_PREFIX = '#s='

/** The prefix of links made by the first version of the site, which cannot be opened. */
const LEGACY_PREFIX = '#p='

/** The pages a share link can open. */
export type ShareScope = 'poe1' | 'simple'

const SCOPES: ShareScope[] = ['poe1', 'simple']

export type ShareParse =
  | { kind: 'none' }
  | { kind: 'ok'; scope: ShareScope; target: AbsSpec }
  | { kind: 'legacy' }
  | { kind: 'invalid' }

/** The page and its target, in one URL fragment. */
export function encodeShare(scope: ShareScope, target: AbsSpec): string {
  return compressToEncodedURIComponent(JSON.stringify({ v: 2, scope, target }))
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
    const value = JSON.parse(json) as { v?: unknown; scope?: unknown; target?: unknown } | null
    if (!value || typeof value !== 'object') return { kind: 'invalid' }
    // A pattern and a pasted item, from the one-step table.
    if (value.v === 1) return { kind: 'legacy' }
    const target = normalizeSpec(value.target)
    if (value.v !== 2 || !SCOPES.includes(value.scope as ShareScope) || !target) return { kind: 'invalid' }
    return { kind: 'ok', scope: value.scope as ShareScope, target }
  } catch {
    return { kind: 'invalid' }
  }
}

export function buildShareUrl(scope: ShareScope, target: AbsSpec, origin = ''): string {
  return `${origin}/${scope}/${SHARE_PREFIX}${encodeShare(scope, target)}`
}
