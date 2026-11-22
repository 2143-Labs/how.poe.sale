/** Main thread → worker, once, before any request: the URL of the game's data bundle. */
export interface InitMessage {
  type: 'init'
  bundle: string
}

export type AppRequestType = 'start' | 'mods' | 'loop' | 'apply'

/** The arguments `mods` needs; `loop` and `apply` carry their request as JSON. */
export interface ModsPayload {
  base: string
  itemLevel: number
  influences: string[]
}

/** Main thread → worker. `payload` is the module's JSON, for the calls that take arguments. */
export type AppRequest =
  | { type: 'start'; id: number }
  | { type: 'mods'; id: number; payload: ModsPayload }
  | { type: 'loop'; id: number; payload: string }
  | { type: 'apply'; id: number; payload: string }

/** Worker → main thread. Every answer echoes the request id and carries `error` on failure. */
export interface AppResponse {
  type: AppRequestType
  id: number
  /** The answer as JSON. */
  result?: string
  error?: string
}
