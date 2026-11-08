import { describe, expect, it } from 'vitest'
import { createClient, type WorkerLike } from '../client'
import type { SolverRequest, SolverResponse } from '../protocol'
import type { AbsSpec, AppliedView } from '../types'

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

const root: AbsSpec = {
  base: 'Archdemon Crown',
  itemLevel: 87,
  rarity: 'rare',
  influences: [],
  prefixes: [],
  suffixes: [],
  anyPrefixes: 0,
  anySuffixes: 0,
  corrupted: false,
}

function applied(method: string): AppliedView {
  return { node: 'root', method, name: method, cost: 'none', simulated: null, outcomes: [], watched: [] }
}

describe('client', () => {
  it('delivers only the newest forward answer', async () => {
    const worker = new FakeWorker()
    const busy: boolean[] = []
    const client = createClient(worker, { onBusy: (b) => busy.push(b) })
    const delivered: string[] = []
    void client.chartForward('root', 'first').then((a) => delivered.push(a.method))
    const newest = client.chartForward('root', 'second').then((a) => delivered.push(a.method))
    const [first, second] = worker.messages
    worker.respond({ type: 'chartForward', id: second.id, applied: applied('second') })
    worker.respond({ type: 'chartForward', id: first.id, applied: applied('first') })
    await newest
    expect(delivered).toEqual(['second'])
    expect(busy).toEqual([true, true, false])
  })

  it('sends the root as a spec', () => {
    const worker = new FakeWorker()
    const client = createClient(worker, { onBusy: () => undefined })
    void client.chartStart(root, [{ id: 'IncreasedLife9', orBetter: true }])
    const [first] = worker.messages
    if (first.type !== 'chartStart') throw new Error(`expected a chartStart request, got ${first.type}`)
    expect(JSON.parse(first.root).base).toBe('Archdemon Crown')
    expect(JSON.parse(first.watch)).toEqual([{ id: 'IncreasedLife9', orBetter: true }])
  })

  it('rejects a parse the worker could not answer', async () => {
    const worker = new FakeWorker()
    const client = createClient(worker, { onBusy: () => undefined })
    const parsed = client.parse('hello')
    worker.respond({ type: 'parse', id: worker.messages[0].id, error: 'item: no known base item in the pasted text' })
    await expect(parsed).rejects.toThrow('item: no known base item in the pasted text')
  })
})
