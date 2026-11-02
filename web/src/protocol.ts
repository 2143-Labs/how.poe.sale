import type { Problem } from './types'

/** Main thread → worker. */
export type SolverRequest =
  | { type: 'version'; id: number }
  | { type: 'solve'; id: number; problem: Problem }
  | { type: 'parse'; id: number; text: string }
  | { type: 'influenceMods'; id: number; base: string; influence: string }

/**
 * Worker → main thread. Every answer echoes the request id and carries `error`
 * on failure. `solve`/`parse`/`influenceMods` payloads stay `unknown` because
 * the Step-2 stubs answer with `{"version":1}` / `{}` / `[]`; the client
 * normalises whatever arrives.
 */
export type SolverResponse =
  | { type: 'version'; id: number; version?: string; error?: string }
  | { type: 'solve'; id: number; solution?: unknown; error?: string }
  | { type: 'parse'; id: number; draft?: unknown; error?: string }
  | { type: 'influenceMods'; id: number; mods?: unknown; error?: string }
