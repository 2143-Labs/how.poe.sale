import type {
  AppliedView,
  ItemView,
  MadeByView,
  MethodOption,
  ModOption,
  NodeView,
  PinnedView,
  StartInfo,
} from './types'

/** Main thread → worker. */
export type SolverRequest =
  | { type: 'start'; id: number }
  | { type: 'parse'; id: number; text: string }
  | { type: 'mods'; id: number; base: string; itemLevel: number; influences: string[] }
  /** `root` and `watch` are JSON: an `AbsSpec` and a list of `SlotSpec`. */
  | { type: 'chartStart'; id: number; root: string; watch: string }
  | { type: 'chartMethods'; id: number; node: string }
  | { type: 'chartForward'; id: number; node: string; method: string }
  /** `methods` empty asks for the chart's own backward set. */
  | { type: 'chartMadeBy'; id: number; node: string; methods: string[] }
  | { type: 'chartPinForward'; id: number; node: string; method: string; key: string }
  | { type: 'chartPinBackward'; id: number; node: string; method: string; key: string }

/** Worker → main thread. Every answer echoes the request id and carries `error` on failure. */
export type SolverResponse =
  | ({ type: 'start'; id: number; error?: string } & Partial<StartInfo>)
  | { type: 'parse'; id: number; item?: ItemView; error?: string }
  | { type: 'mods'; id: number; mods?: ModOption[]; error?: string }
  | { type: 'chartStart'; id: number; node?: NodeView; error?: string }
  | { type: 'chartMethods'; id: number; methods?: MethodOption[]; error?: string }
  | { type: 'chartForward'; id: number; applied?: AppliedView; error?: string }
  | { type: 'chartMadeBy'; id: number; madeBy?: MadeByView; error?: string }
  | { type: 'chartPinForward'; id: number; pinned?: PinnedView; error?: string }
  | { type: 'chartPinBackward'; id: number; pinned?: PinnedView; error?: string }
