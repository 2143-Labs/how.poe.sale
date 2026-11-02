import { normalizeDraft } from './problem'
import {
  normalizeContracts,
  normalizeProposalList,
  normalizeRecipes,
  normalizeSolutionV3,
  normalizeTiers,
} from './solution'
import type { SolverRequest, SolverResponse } from './protocol'
import type {
  AnyProblem,
  ContractJson,
  InfluenceModEntry,
  ProcessProposalJson,
  RecipeDef,
  Side,
  SolutionV3,
  TargetDraft,
  TierJson,
} from './types'

/** The slice of `Worker` this client needs, so tests can drive a fake one. */
export interface WorkerLike {
  postMessage(message: unknown): void
  addEventListener(type: string, listener: (event: MessageEvent) => void): void
  removeEventListener(type: string, listener: (event: MessageEvent) => void): void
  terminate?(): void
}

export interface SolverCallbacks {
  onSolution: (solution: SolutionV3) => void
  onBusy: (busy: boolean) => void
  onError: (message: string | null) => void
}

export interface SolverClient {
  solve(problem: AnyProblem): void
  parse(text: string): Promise<TargetDraft>
  influenceMods(base: string, influence: string): Promise<InfluenceModEntry[]>
  proposeProcesses(problem: AnyProblem, goodId: string): Promise<ProcessProposalJson[]>
  contracts(): Promise<ContractJson[]>
  recipes(): Promise<RecipeDef[]>
  modTiers(base: string, group: string, side: Side): Promise<TierJson[]>
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
      callbacks.onSolution(normalizeSolutionV3(response.solution))
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
      case 'version':
        pending.resolve(response.version ?? '')
        return
      case 'parse':
        pending.resolve(normalizeDraft(response.draft))
        return
      case 'influenceMods':
        pending.resolve(Array.isArray(response.mods) ? response.mods : [])
        return
      case 'proposeProcesses':
        pending.resolve(normalizeProposalList(response.proposals))
        return
      case 'contracts':
        pending.resolve(normalizeContracts(response.contracts))
        return
      case 'recipes':
        pending.resolve(normalizeRecipes(response.recipes))
        return
      case 'modTiers':
        pending.resolve(normalizeTiers(response.tiers))
        return
    }
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

    proposeProcesses(problem, goodId) {
      return request<ProcessProposalJson[]>({ type: 'proposeProcesses', id: nextId++, problem, goodId })
    },

    contracts() {
      return request<ContractJson[]>({ type: 'contracts', id: nextId++ })
    },

    recipes() {
      return request<RecipeDef[]>({ type: 'recipes', id: nextId++ })
    },

    modTiers(base, group, side) {
      return request<TierJson[]>({ type: 'modTiers', id: nextId++, base, group, side })
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
