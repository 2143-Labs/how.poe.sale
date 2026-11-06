import type { SolverRequest, SolverResponse } from './protocol'
import type { ItemView, PatternSpec, SolveView, StartInfo } from './types'

/** The slice of `Worker` this client needs, so tests can drive a fake one. */
export interface WorkerLike {
  postMessage(message: unknown): void
  addEventListener(type: string, listener: (event: MessageEvent) => void): void
  removeEventListener(type: string, listener: (event: MessageEvent) => void): void
  terminate?(): void
}

export interface SolverCallbacks {
  onResult: (result: SolveView) => void
  onBusy: (busy: boolean) => void
  onError: (message: string | null) => void
}

export interface SolverClient {
  start(): Promise<StartInfo>
  parse(text: string): Promise<ItemView>
  target(text: string): Promise<PatternSpec>
  solve(pattern: string, item: string): void
  dispose(): void
}

type Waiting = {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

/**
 * Owns the worker. Only the newest `solve` request may answer: a response whose
 * id is not `latestSolve` is dropped, so a slow solve never overwrites a newer
 * one. A failed solve keeps the previous result on screen and reports an error
 * instead.
 */
export function createSolverClient(worker: WorkerLike, callbacks: SolverCallbacks): SolverClient {
  let nextId = 1
  let latestSolve = 0
  const waiting = new Map<number, Waiting>()

  const request = <T>(request: SolverRequest): Promise<T> => {
    const id = request.id
    return new Promise<T>((resolve, reject) => {
      waiting.set(id, { resolve: resolve as (value: unknown) => void, reject })
      worker.postMessage(request)
    })
  }

  const onMessage = (event: MessageEvent) => {
    const response = event.data as SolverResponse | null | undefined
    if (!response || typeof response !== 'object' || typeof response.type !== 'string') return

    if (response.type === 'solve') {
      if (response.id !== latestSolve) return
      callbacks.onBusy(false)
      if (response.error || !response.result) {
        callbacks.onError(response.error ?? 'The solver answered with nothing.')
        return
      }
      callbacks.onError(null)
      callbacks.onResult(response.result)
      return
    }

    const pending = waiting.get(response.id)
    if (!pending) return
    waiting.delete(response.id)
    if (response.error) {
      pending.reject(new Error(response.error))
      return
    }
    switch (response.type) {
      case 'start':
        pending.resolve({
          version: response.version ?? '',
          patterns: response.patterns ?? [],
          examples: response.examples ?? [],
        } satisfies StartInfo)
        return
      case 'parse':
        pending.resolve(response.item)
        return
      case 'target':
        pending.resolve(response.pattern)
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

    target(text) {
      return request<PatternSpec>({ type: 'target', id: nextId++, text })
    },

    solve(pattern, item) {
      const id = nextId++
      latestSolve = id
      callbacks.onError(null)
      callbacks.onBusy(true)
      worker.postMessage({ type: 'solve', id, pattern, item } satisfies SolverRequest)
    },

    dispose() {
      worker.removeEventListener('message', onMessage)
      worker.terminate?.()
    },
  }
}
