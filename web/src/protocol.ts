import type { ItemView, PatternSpec, SolveView, StartInfo } from './types'

/** Main thread → worker. */
export type SolverRequest =
  | { type: 'start'; id: number }
  | { type: 'parse'; id: number; text: string }
  | { type: 'target'; id: number; text: string }
  | { type: 'solve'; id: number; pattern: string; item: string }

/** Worker → main thread. Every answer echoes the request id and carries `error` on failure. */
export type SolverResponse =
  | ({ type: 'start'; id: number; error?: string } & Partial<StartInfo>)
  | { type: 'parse'; id: number; item?: ItemView; error?: string }
  | { type: 'target'; id: number; pattern?: PatternSpec; error?: string }
  | { type: 'solve'; id: number; result?: SolveView; error?: string }
