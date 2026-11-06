import { compressToEncodedURIComponent, decompressFromEncodedURIComponent } from 'lz-string'

export const SHARE_PREFIX = '#s='

/** The prefix of links made by the previous version of the site, which cannot be opened. */
const LEGACY_PREFIX = '#p='

export type ShareParse =
  | { kind: 'none' }
  | { kind: 'ok'; pattern: string; item: string }
  | { kind: 'legacy' }
  | { kind: 'invalid' }

/** The pattern text and the item text, in one URL fragment. */
export function encodeShare(pattern: string, item: string): string {
  return compressToEncodedURIComponent(JSON.stringify({ v: 1, pattern, item }))
}

/** Distinguishes "no shared state", "an old link" and "a link that will not decode". */
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
    const value = JSON.parse(json) as { v?: unknown; pattern?: unknown; item?: unknown } | null
    if (!value || value.v !== 1 || typeof value.pattern !== 'string' || typeof value.item !== 'string') {
      return { kind: 'invalid' }
    }
    return { kind: 'ok', pattern: value.pattern, item: value.item }
  } catch {
    return { kind: 'invalid' }
  }
}

export function buildShareUrl(pattern: string, item: string, origin = ''): string {
  return `${origin}/${SHARE_PREFIX}${encodeShare(pattern, item)}`
}
