import type { CraftRequest, CraftResponse } from './protocol'
import type { Catalog, ModView, RunRequest, RunView } from './types'

/** The slice of `Worker` this client needs, so a page can be driven without one. */
export interface WorkerLike {
  postMessage(message: unknown): void
  addEventListener(type: string, listener: (event: MessageEvent) => void): void
  removeEventListener(type: string, listener: (event: MessageEvent) => void): void
  terminate?(): void
}

export interface CraftClient {
  catalog(): Promise<Catalog>
  mods(base: string): Promise<ModView[]>
  /** Runs one craft. Only the newest run settles, so a slow answer cannot overwrite a newer one. */
  run(request: RunRequest): Promise<RunView>
}

export interface CraftClientHooks {
  /** Whether a run is still being computed. */
  onBusy?: (busy: boolean) => void
}

type Waiting = {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

/** Owns the worker and matches every answer to the request that asked for it. */
export function createCraftClient(worker: WorkerLike, hooks: CraftClientHooks = {}): CraftClient {
  let nextId = 1
  let latest = -1
  const waiting = new Map<number, Waiting>()

  const request = <T>(message: CraftRequest): Promise<T> =>
    new Promise<T>((resolve, reject) => {
      waiting.set(message.id, { resolve: resolve as (value: unknown) => void, reject })
      worker.postMessage(message)
    })

  const onMessage = (event: MessageEvent) => {
    const response = event.data as CraftResponse | null | undefined
    if (!response || typeof response !== 'object' || typeof response.type !== 'string') return
    const pending = waiting.get(response.id)
    if (!pending) return
    if (response.type === 'run') {
      // A superseded run's answer is dropped without settling its call.
      if (response.id !== latest) return
      hooks.onBusy?.(false)
    }
    waiting.delete(response.id)
    if (response.error !== undefined) {
      pending.reject(new Error(response.error))
      return
    }
    switch (response.type) {
      case 'catalog':
        if (response.catalog) pending.resolve(response.catalog)
        else pending.reject(new Error('The engine answered with nothing.'))
        return
      case 'mods':
        pending.resolve(response.mods ?? [])
        return
      case 'run':
        if (response.result) pending.resolve(response.result)
        else pending.reject(new Error('The engine answered with nothing.'))
        return
    }
  }

  worker.addEventListener('message', onMessage)

  return {
    catalog() {
      return request<Catalog>({ type: 'catalog', id: nextId++ })
    },

    mods(base) {
      return request<ModView[]>({ type: 'mods', id: nextId++, base })
    },

    run(body) {
      const id = nextId++
      // The older run never settles: its caller is already looking at the newer one.
      if (latest !== -1) waiting.delete(latest)
      latest = id
      hooks.onBusy?.(true)
      return request<RunView>({ type: 'run', id, request: JSON.stringify(body) })
    },
  }
}
