import type { ItemView, ModOption, PreimagesView, StartInfo } from './types'

/** Main thread → worker. */
export type SolverRequest =
  | { type: 'start'; id: number }
  | { type: 'parse'; id: number; text: string }
  | { type: 'mods'; id: number; base: string; itemLevel: number; influences: string[] }
  /** `target` is `AbsSpec` JSON. */
  | { type: 'preimages'; id: number; target: string; methods: string[] }

/** Worker → main thread. Every answer echoes the request id and carries `error` on failure. */
export type SolverResponse =
  | ({ type: 'start'; id: number; error?: string } & Partial<StartInfo>)
  | { type: 'parse'; id: number; item?: ItemView; error?: string }
  | { type: 'mods'; id: number; mods?: ModOption[]; error?: string }
  | { type: 'preimages'; id: number; result?: PreimagesView; error?: string }
