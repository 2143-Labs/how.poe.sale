import { compressToEncodedURIComponent, decompressFromEncodedURIComponent } from 'lz-string'
import type { Problem } from './types'

export const SHARE_PREFIX = '#p='

export function encodeProblem(problem: Problem): string {
  return compressToEncodedURIComponent(JSON.stringify(problem))
}

export function decodeProblem(encoded: string): Problem | null {
  let json: string | null
  try {
    json = decompressFromEncodedURIComponent(encoded)
  } catch {
    return null
  }
  if (!json) return null
  try {
    const problem = JSON.parse(json) as Partial<Problem> | null
    if (!problem || typeof problem !== 'object') return null
    if (typeof problem.version !== 'number') return null
    if (!problem.target || typeof problem.target !== 'object') return null
    if (!problem.prices || typeof problem.prices !== 'object') return null
    return problem as Problem
  } catch {
    return null
  }
}

export type ShareParse = { kind: 'none' } | { kind: 'ok'; problem: Problem } | { kind: 'invalid' }

/** Distinguishes "no shared problem" from "a shared problem that will not decode". */
export function parseShareHash(hash: string): ShareParse {
  if (!hash.startsWith(SHARE_PREFIX)) return { kind: 'none' }
  const problem = decodeProblem(hash.slice(SHARE_PREFIX.length))
  return problem ? { kind: 'ok', problem } : { kind: 'invalid' }
}

export function buildShareUrl(problem: Problem, origin = ''): string {
  return `${origin}/#p=${encodeProblem(problem)}`
}
