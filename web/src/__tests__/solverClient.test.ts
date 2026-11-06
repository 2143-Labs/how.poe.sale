import { describe, expect, it } from 'vitest'
import { createSolverClient, type WorkerLike } from '../solverClient'
import type { SolverRequest, SolverResponse } from '../protocol'
import type { SolveView } from '../types'

class FakeWorker implements WorkerLike {
  readonly messages: SolverRequest[] = []
  private listener: ((event: MessageEvent) => void) | null = null

  postMessage(message: unknown): void {
    this.messages.push(message as SolverRequest)
  }

  addEventListener(_type: string, listener: (event: MessageEvent) => void): void {
    this.listener = listener
  }

  removeEventListener(): void {
    this.listener = null
  }

  respond(response: SolverResponse): void {
    this.listener?.({ data: response } as unknown as MessageEvent)
  }
}

function result(progress: number): SolveView {
  return {
    now: { progress, matches: 0, blocked: 0, fits: true },
    exact: [],
    certain: [],
    blind: [],
    unmet: [],
    sources: { found: 0, groups: [], skipped: [] },
  }
}

describe('solver client', () => {
  it('delivers only the newest solve', () => {
    const worker = new FakeWorker()
    const results: SolveView[] = []
    const client = createSolverClient(worker, {
      onResult: (r) => results.push(r),
      onBusy: () => undefined,
      onError: () => undefined,
    })
    client.solve('{}', 'first')
    client.solve('{}', 'second')
    const [first, second] = worker.messages
    worker.respond({ type: 'solve', id: second.id, result: result(0.5) })
    worker.respond({ type: 'solve', id: first.id, result: result(0.1) })
    expect(results).toEqual([result(0.5)])
  })

  it('rejects a parse the worker could not answer', async () => {
    const worker = new FakeWorker()
    const client = createSolverClient(worker, {
      onResult: () => undefined,
      onBusy: () => undefined,
      onError: () => undefined,
    })
    const parsed = client.parse('hello')
    worker.respond({ type: 'parse', id: worker.messages[0].id, error: 'item: no known base item in the pasted text' })
    await expect(parsed).rejects.toThrow('item: no known base item in the pasted text')
  })
})
