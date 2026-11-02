import { normalizeDraft } from './problem'
import { normalizeSolution } from './solution'
import type { SolverRequest, SolverResponse } from './protocol'
import type { InfluenceModEntry, Problem, Solution, TargetDraft } from './types'

/** The slice of `Worker` this client needs, so tests can drive a fake one. */
export interface WorkerLike {
  postMessage(message: unknown): void
  addEventListener(type: string, listener: (event: MessageEvent) => void): void
  removeEventListener(type: string, listener: (event: MessageEvent) => void): void
  terminate?(): void
}

export interface SolverCallbacks {
  onSolution: (solution: Solution) => void
  onBusy: (busy: boolean) => void
  onError: (message: string | null) => void
}

export interface SolverClient {
  solve(problem: Problem): void
  parse(text: string): Promise<TargetDraft>
  influenceMods(base: string, influence: string): Promise<InfluenceModEntry[]>
  version(): Promise<string>
  dispose(): void
}

type Waiting = {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

/**
 * Owns the worker. Only the newest `solve` request may answer: a response whose
 * id is not `latestSolve` is dropped, so a slow solve never overwrites a newer
 * one. A failed solve keeps the previous solution on screen and reports an
 * error instead.
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
      if (response.error) {
        callbacks.onError(response.error)
        return
      }
      callbacks.onError(null)
      callbacks.onSolution(normalizeSolution(response.solution))
      return
    }

    const pending = waiting.get(response.id)
    if (!pending) return
    waiting.delete(response.id)
    if (response.error) {
      pending.reject(new Error(response.error))
      return
    }
    if (response.type === 'version') pending.resolve(response.version ?? '')
    else if (response.type === 'parse') pending.resolve(normalizeDraft(response.draft))
    else pending.resolve(Array.isArray(response.mods) ? response.mods : [])
  }

  worker.addEventListener('message', onMessage)

  return {
    solve(problem) {
      const id = nextId++
      latestSolve = id
      callbacks.onError(null)
      callbacks.onBusy(true)
      worker.postMessage({ type: 'solve', id, problem } satisfies SolverRequest)
    },

    parse(text) {
      return request<TargetDraft>({ type: 'parse', id: nextId++, text })
    },

    influenceMods(base, influence) {
      return request<InfluenceModEntry[]>({ type: 'influenceMods', id: nextId++, base, influence })
    },

    version() {
      return request<string>({ type: 'version', id: nextId++ })
    },

    dispose() {
      worker.removeEventListener('message', onMessage)
      worker.terminate?.()
    },
  }
}
