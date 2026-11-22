import type { Catalog, ModView, RunView } from './types'

/** Main thread → worker. `run` carries the `RunRequest` as JSON. */
export type CraftRequest =
  | { type: 'catalog'; id: number }
  | { type: 'mods'; id: number; base: string }
  | { type: 'run'; id: number; request: string }

/** Worker → main thread. Every answer echoes the request id and carries `error` on failure. */
export type CraftResponse =
  | { type: 'catalog'; id: number; catalog?: Catalog; error?: string }
  | { type: 'mods'; id: number; mods?: ModView[]; error?: string }
  | { type: 'run'; id: number; result?: RunView; error?: string }
