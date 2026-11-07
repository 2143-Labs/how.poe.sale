import { describe, expect, it } from 'vitest'
import { createClient, type WorkerLike } from '../client'
import type { SolverRequest, SolverResponse } from '../protocol'
import type { AbsSpec, PreimagesView } from '../types'

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

const target: AbsSpec = {
  base: 'Archdemon Crown',
  itemLevel: 87,
  rarity: 'rare',
  influences: [],
  prefixes: [],
  suffixes: [],
  anyPrefixes: 0,
  anySuffixes: 0,
}

function result(anyPrefixes: number): PreimagesView {
  return {
    target: {
      base: 'Archdemon Crown',
      itemLevel: 87,
      rarity: 'rare',
      influences: [],
      prefixes: [],
      suffixes: [],
      anyPrefixes,
      anySuffixes: 0,
    },
    steps: [],
    skipped: [],
  }
}

describe('client', () => {
  it('delivers only the newest preimages', async () => {
    const worker = new FakeWorker()
    const busy: boolean[] = []
    const client = createClient(worker, { onBusy: (b) => busy.push(b) })
    const delivered: number[] = []
    void client.preimages({ ...target, anyPrefixes: 1 }, []).then((r) => delivered.push(r.target.anyPrefixes))
    const newest = client.preimages({ ...target, anyPrefixes: 2 }, []).then((r) => delivered.push(r.target.anyPrefixes))
    const [first, second] = worker.messages
    if (second.type !== 'preimages') throw new Error(`expected a preimages request, got ${second.type}`)
    expect(JSON.parse(second.target).anyPrefixes).toBe(2)
    worker.respond({ type: 'preimages', id: second.id, result: result(2) })
    worker.respond({ type: 'preimages', id: first.id, result: result(1) })
    // Had the older answer settled, its callback would have been queued before this one ran.
    await newest
    expect(delivered).toEqual([2])
    expect(busy).toEqual([true, true, false])
  })

  it('rejects a parse the worker could not answer', async () => {
    const worker = new FakeWorker()
    const client = createClient(worker, { onBusy: () => undefined })
    const parsed = client.parse('hello')
    worker.respond({ type: 'parse', id: worker.messages[0].id, error: 'item: no known base item in the pasted text' })
    await expect(parsed).rejects.toThrow('item: no known base item in the pasted text')
  })
})
