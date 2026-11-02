import type { AnyProblem, Side } from './types'

/** Main thread → worker. */
export type SolverRequest =
  | { type: 'version'; id: number }
  | { type: 'solve'; id: number; problem: AnyProblem }
  | { type: 'parse'; id: number; text: string }
  | { type: 'influenceMods'; id: number; base: string; influence: string }
  | { type: 'proposeProcesses'; id: number; problem: AnyProblem; goodId: string }
  | { type: 'contracts'; id: number }
  | { type: 'recipes'; id: number }
  | { type: 'modTiers'; id: number; base: string; group: string; side: Side }

/**
 * Worker → main thread. Every answer echoes the request id and carries `error`
 * on failure. Payloads stay `unknown` because the stubs answer with `{}` / `[]`;
 * the client normalises whatever arrives.
 */
export type SolverResponse =
  | { type: 'version'; id: number; version?: string; error?: string }
  | { type: 'solve'; id: number; solution?: unknown; error?: string }
  | { type: 'parse'; id: number; draft?: unknown; error?: string }
  | { type: 'influenceMods'; id: number; mods?: unknown; error?: string }
  | { type: 'proposeProcesses'; id: number; proposals?: unknown; error?: string }
  | { type: 'contracts'; id: number; contracts?: unknown; error?: string }
  | { type: 'recipes'; id: number; recipes?: unknown; error?: string }
  | { type: 'modTiers'; id: number; tiers?: unknown; error?: string }
