import type { SolverRequest, SolverResponse } from './protocol'
import type {
  AbsSpec,
  AppliedView,
  ItemView,
  MadeByView,
  MethodOption,
  ModOption,
  NodeView,
  PinnedView,
  SlotSpec,
  StartInfo,
} from './types'

/** The slice of `Worker` this client needs, so tests can drive a fake one. */
export interface WorkerLike {
  postMessage(message: unknown): void
  addEventListener(type: string, listener: (event: MessageEvent) => void): void
  removeEventListener(type: string, listener: (event: MessageEvent) => void): void
  terminate?(): void
}

export interface ClientCallbacks {
  /** Whether a chart forward or backward answer is still being computed. */
  onBusy: (busy: boolean) => void
}

export interface Client {
  start(): Promise<StartInfo>
  parse(text: string): Promise<ItemView>
  mods(base: string, itemLevel: number, influences: string[]): Promise<ModOption[]>
  /** Starts a chart whose root is `root` and which also tracks `watch`. */
  chartStart(root: AbsSpec, watch: SlotSpec[]): Promise<NodeView>
  chartMethods(node: string): Promise<MethodOption[]>
  /** `method`'s outcomes from `node`. Only the newest call settles. */
  chartForward(node: string, method: string): Promise<AppliedView>
  /** The inputs that could have produced `node`. Only the newest call settles. */
  chartMadeBy(node: string, methods: string[]): Promise<MadeByView>
  chartPinForward(node: string, method: string, key: string): Promise<PinnedView>
  chartPinBackward(node: string, method: string, key: string): Promise<PinnedView>
  dispose(): void
}

type Waiting = {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

/** The answers that supersede their predecessor, so a slow one cannot overwrite a newer one. */
const LATEST: Record<string, true> = { chartForward: true, chartMadeBy: true }

/** Owns the worker and matches every answer to the request that asked for it. */
export function createClient(worker: WorkerLike, callbacks: ClientCallbacks): Client {
  let nextId = 1
  const latest = new Map<string, number>()
  const waiting = new Map<number, Waiting>()

  const request = <T>(message: SolverRequest): Promise<T> =>
    new Promise<T>((resolve, reject) => {
      waiting.set(message.id, { resolve: resolve as (value: unknown) => void, reject })
      worker.postMessage(message)
    })

  /** A request whose successor wins: the older call never settles. */
  const latestOnly = <T>(message: SolverRequest): Promise<T> => {
    const previous = latest.get(message.type)
    if (previous) waiting.delete(previous)
    latest.set(message.type, message.id)
    callbacks.onBusy(true)
    return request<T>(message)
  }

  const onMessage = (event: MessageEvent) => {
    const response = event.data as SolverResponse | null | undefined
    if (!response || typeof response !== 'object' || typeof response.type !== 'string') return
    const pending = waiting.get(response.id)
    if (!pending) return
    waiting.delete(response.id)

    if (LATEST[response.type]) {
      if (response.id !== latest.get(response.type)) return
      callbacks.onBusy(false)
    }
    if (response.error) {
      pending.reject(new Error(response.error))
      return
    }
    switch (response.type) {
      case 'start':
        pending.resolve({
          version: response.version ?? '',
          examples: response.examples ?? [],
          bases: response.bases ?? [],
        } satisfies StartInfo)
        return
      case 'parse':
        pending.resolve(response.item)
        return
      case 'mods':
        pending.resolve(response.mods ?? [])
        return
      case 'chartStart':
        if (response.node) pending.resolve(response.node)
        else pending.reject(new Error('The solver answered with nothing.'))
        return
      case 'chartMethods':
        pending.resolve(response.methods ?? [])
        return
      case 'chartForward':
        if (response.applied) pending.resolve(response.applied)
        else pending.reject(new Error('The solver answered with nothing.'))
        return
      case 'chartMadeBy':
        if (response.madeBy) pending.resolve(response.madeBy)
        else pending.reject(new Error('The solver answered with nothing.'))
        return
      case 'chartPinForward':
      case 'chartPinBackward':
        if (response.pinned) pending.resolve(response.pinned)
        else pending.reject(new Error('The solver answered with nothing.'))
        return
    }
  }

  worker.addEventListener('message', onMessage)

  return {
    start() {
      return request<StartInfo>({ type: 'start', id: nextId++ })
    },

    parse(text) {
      return request<ItemView>({ type: 'parse', id: nextId++, text })
    },

    mods(base, itemLevel, influences) {
      return request<ModOption[]>({ type: 'mods', id: nextId++, base, itemLevel, influences })
    },

    chartStart(root, watch) {
      return request<NodeView>({
        type: 'chartStart',
        id: nextId++,
        root: JSON.stringify(root),
        watch: JSON.stringify(watch),
      })
    },

    chartMethods(node) {
      return request<MethodOption[]>({ type: 'chartMethods', id: nextId++, node })
    },

    chartForward(node, method) {
      return latestOnly<AppliedView>({ type: 'chartForward', id: nextId++, node, method })
    },

    chartMadeBy(node, methods) {
      return latestOnly<MadeByView>({ type: 'chartMadeBy', id: nextId++, node, methods })
    },

    chartPinForward(node, method, key) {
      return request<PinnedView>({ type: 'chartPinForward', id: nextId++, node, method, key })
    },

    chartPinBackward(node, method, key) {
      return request<PinnedView>({ type: 'chartPinBackward', id: nextId++, node, method, key })
    },

    dispose() {
      worker.removeEventListener('message', onMessage)
      worker.terminate?.()
    },
  }
}
