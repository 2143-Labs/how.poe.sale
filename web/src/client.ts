import type { SolverRequest, SolverResponse } from './protocol'
import type { AbsSpec, ItemView, ModOption, PreimagesView, StartInfo } from './types'

/** The slice of `Worker` this client needs, so tests can drive a fake one. */
export interface WorkerLike {
  postMessage(message: unknown): void
  addEventListener(type: string, listener: (event: MessageEvent) => void): void
  removeEventListener(type: string, listener: (event: MessageEvent) => void): void
  terminate?(): void
}

export interface ClientCallbacks {
  /** Whether a `preimages` request is waiting for its answer. */
  onBusy: (busy: boolean) => void
}

export interface Client {
  start(): Promise<StartInfo>
  parse(text: string): Promise<ItemView>
  mods(base: string, itemLevel: number, influences: string[]): Promise<ModOption[]>
  /**
   * The steps that could have produced `target`. Only the newest call settles: an older one
   * whose answer arrives later never resolves or rejects, so a slow answer cannot overwrite a
   * newer one.
   */
  preimages(target: AbsSpec, methods: string[]): Promise<PreimagesView>
  dispose(): void
}

type Waiting = {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

/** Owns the worker and matches every answer to the request that asked for it. */
export function createClient(worker: WorkerLike, callbacks: ClientCallbacks): Client {
  let nextId = 1
  let latestPreimages = 0
  const waiting = new Map<number, Waiting>()

  const request = <T>(message: SolverRequest): Promise<T> =>
    new Promise<T>((resolve, reject) => {
      waiting.set(message.id, { resolve: resolve as (value: unknown) => void, reject })
      worker.postMessage(message)
    })

  const onMessage = (event: MessageEvent) => {
    const response = event.data as SolverResponse | null | undefined
    if (!response || typeof response !== 'object' || typeof response.type !== 'string') return
    const pending = waiting.get(response.id)
    if (!pending) return
    waiting.delete(response.id)

    if (response.type === 'preimages') {
      if (response.id !== latestPreimages) return
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
      case 'preimages':
        if (response.result) pending.resolve(response.result)
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

    preimages(target, methods) {
      const id = nextId++
      // An older request still waiting will never be answered to its caller.
      if (latestPreimages) waiting.delete(latestPreimages)
      latestPreimages = id
      callbacks.onBusy(true)
      return request<PreimagesView>({ type: 'preimages', id, target: JSON.stringify(target), methods })
    },

    dispose() {
      worker.removeEventListener('message', onMessage)
      worker.terminate?.()
    },
  }
}
