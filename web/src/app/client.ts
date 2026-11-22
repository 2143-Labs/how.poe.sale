import type { AppRequest, AppRequestType, AppResponse } from './protocol'
import type { ApplyRequest, ApplyView, LoopRequest, LoopView, ModOptionView, Start } from './types'

/** The slice of `Worker` this client needs, so tests can drive a fake one. */
export interface WorkerLike {
  postMessage(message: unknown): void
  addEventListener(type: string, listener: (event: MessageEvent) => void): void
  removeEventListener(type: string, listener: (event: MessageEvent) => void): void
  terminate?(): void
}

export interface AppClient {
  start(): Promise<Start>
  mods(base: string, itemLevel: number, influences: string[]): Promise<ModOptionView[]>
  /** The loop over `request`. Only the newest call settles, so a slow one cannot overwrite a newer one. */
  loopReport(request: LoopRequest): Promise<LoopView>
  applyCurrency(request: ApplyRequest): Promise<ApplyView>
  dispose(): void
}

export interface AppClientHooks {
  /** Whether a loop report is still being computed. */
  onBusy?: (busy: boolean) => void
}

type Waiting = {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

/** The answers that supersede their predecessor. */
const LATEST: Partial<Record<AppRequestType, true>> = { loop: true }

/** Owns the worker and matches every answer to the request that asked for it. */
export function createAppClient(worker: WorkerLike, hooks: AppClientHooks = {}): AppClient {
  let nextId = 1
  const latest = new Map<AppRequestType, number>()
  const waiting = new Map<number, Waiting>()

  const request = <T>(message: AppRequest): Promise<T> =>
    new Promise<T>((resolve, reject) => {
      waiting.set(message.id, { resolve: resolve as (value: unknown) => void, reject })
      worker.postMessage(message)
    })

  /** A request whose successor wins: the older call never settles. */
  const latestOnly = <T>(message: AppRequest): Promise<T> => {
    const previous = latest.get(message.type)
    if (previous !== undefined) waiting.delete(previous)
    latest.set(message.type, message.id)
    hooks.onBusy?.(true)
    return request<T>(message)
  }

  const onMessage = (event: MessageEvent) => {
    const response = event.data as AppResponse | null | undefined
    if (!response || typeof response !== 'object' || typeof response.type !== 'string') return
    const pending = waiting.get(response.id)
    if (!pending) return
    waiting.delete(response.id)

    if (LATEST[response.type]) {
      if (response.id !== latest.get(response.type)) return
      hooks.onBusy?.(false)
    }
    if (response.error !== undefined) {
      pending.reject(new Error(response.error))
      return
    }
    if (response.result === undefined) {
      pending.reject(new Error('The engine answered with nothing.'))
      return
    }
    try {
      pending.resolve(JSON.parse(response.result))
    } catch {
      pending.reject(new Error('The engine answered with unreadable JSON.'))
    }
  }

  worker.addEventListener('message', onMessage)

  return {
    start() {
      return request<Start>({ type: 'start', id: nextId++ })
    },

    mods(base, itemLevel, influences) {
      return request<ModOptionView[]>({ type: 'mods', id: nextId++, payload: { base, itemLevel, influences } })
    },

    loopReport(given) {
      return latestOnly<LoopView>({ type: 'loop', id: nextId++, payload: JSON.stringify(given) })
    },

    applyCurrency(given) {
      return request<ApplyView>({ type: 'apply', id: nextId++, payload: JSON.stringify(given) })
    },

    dispose() {
      worker.removeEventListener('message', onMessage)
      worker.terminate?.()
    },
  }
}
