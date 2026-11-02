import { compressToEncodedURIComponent, decompressFromEncodedURIComponent } from 'lz-string'
import type { AnyProblem, ProblemV3 } from './types'

export const SHARE_PREFIX = '#p='

export function encodeProblem(problem: AnyProblem): string {
  return compressToEncodedURIComponent(JSON.stringify(problem))
}

/**
 * Accepts both contract versions: v3 (this UI's shape) and v1 (links shared
 * before the market landed). A v1 problem is returned untouched — the solver
 * upgrades it, so old links keep their exact semantics.
 */
export function decodeProblem(encoded: string): AnyProblem | null {
  let json: string | null
  try {
    json = decompressFromEncodedURIComponent(encoded)
  } catch {
    return null
  }
  if (!json) return null
  try {
    const problem = JSON.parse(json) as Partial<ProblemV3> & { target?: unknown } | null
    if (!problem || typeof problem !== 'object') return null
    if (typeof problem.version !== 'number') return null
    if (!problem.prices || typeof problem.prices !== 'object') return null
    // v3 carries goods; v1 carries a single target.
    if (problem.version === 3) {
      if (!Array.isArray(problem.goods) || problem.goods.length === 0 || typeof problem.root !== 'string') {
        return null
      }
      return problem as ProblemV3
    }
    if (!problem.target || typeof problem.target !== 'object') return null
    return problem as AnyProblem
  } catch {
    return null
  }
}

export type ShareParse = { kind: 'none' } | { kind: 'ok'; problem: AnyProblem } | { kind: 'invalid' }

/** Distinguishes "no shared problem" from "a shared problem that will not decode". */
export function parseShareHash(hash: string): ShareParse {
  if (!hash.startsWith(SHARE_PREFIX)) return { kind: 'none' }
  const problem = decodeProblem(hash.slice(SHARE_PREFIX.length))
  return problem ? { kind: 'ok', problem } : { kind: 'invalid' }
}

export function buildShareUrl(problem: AnyProblem, origin = ''): string {
  return `${origin}/#p=${encodeProblem(problem)}`
}
