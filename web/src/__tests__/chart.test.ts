import { describe, expect, it } from 'vitest'
import type { Client } from '../client'
import { createFlowStore } from '../flow/store'
import { SCOPES } from '../flow/scope'
import type { AbsSpec, EdgeView, ItemView, NodeView, PinnedView } from '../types'

const root: AbsSpec = {
  base: 'Archdemon Crown',
  itemLevel: 87,
  rarity: 'rare',
  influences: [],
  prefixes: [],
  suffixes: [],
  anyPrefixes: 2,
  anySuffixes: 2,
  corrupted: false,
}

function nodeOf(id: string): NodeView {
  return {
    id,
    spec: root,
    view: {
      base: root.base,
      itemLevel: root.itemLevel,
      rarity: root.rarity,
      corrupted: false,
      influences: [],
      prefixes: [],
      suffixes: [],
      anyPrefixes: root.anyPrefixes,
      anySuffixes: root.anySuffixes,
    },
    members: null,
    approx: false,
    source: null,
  }
}

/** A client whose every chart answer is immediate, so a test can wait one tick and read state. */
class FakeClient implements Client {
  pinned: PinnedView[] = []

  async start() {
    return { version: 'v', examples: [], bases: [] }
  }

  async parse(text: string): Promise<ItemView> {
    throw new Error(`unused: ${text}`)
  }

  async mods() {
    return []
  }

  async chartStart() {
    return nodeOf('root')
  }

  async chartMethods() {
    return []
  }

  async chartForward() {
    return {
      node: 'root',
      method: 'm',
      name: 'M',
      cost: 'none',
      simulated: null,
      repeatable: false,
      outcomes: [],
      watched: [],
    }
  }

  async chartMadeBy(node: string) {
    return { node, inputs: [], skipped: [] }
  }

  async chartPinForward(_node: string, method: string, key: string) {
    const edge: EdgeView = { from: 'root', to: key, method, name: 'M', p: 0.5, interval: null, repeatable: false }
    const pinned: PinnedView = { node: nodeOf(key), edge }
    this.pinned.push(pinned)
    return pinned
  }

  async chartPinBackward(node: string, method: string, key: string) {
    const edge: EdgeView = { from: key, to: node, method, name: 'M', p: 0.5, interval: null, repeatable: false }
    const pinned: PinnedView = { node: nodeOf(key), edge }
    this.pinned.push(pinned)
    return pinned
  }

  dispose(): void {
    // nothing to release
  }
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0))

async function store() {
  const client = new FakeClient()
  const flow = createFlowStore(SCOPES.simple, { client, hash: '', storage: null, dropHash: () => undefined })
  await flush()
  return { flow, client }
}

describe('the chart', () => {
  it('pins the same step once', async () => {
    const { flow } = await store()
    flow.setState({ selected: 'root', method: 'm' })
    flow.getState().pinForward('a')
    await flush()
    flow.setState({ selected: 'root', method: 'm' })
    flow.getState().pinForward('a')
    await flush()
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root', 'a'])
    expect(flow.getState().edges).toHaveLength(1)
    expect(flow.getState().pins).toHaveLength(2)
  })

  it('removes the nodes only a removed node led to', async () => {
    const { flow } = await store()
    flow.setState({ selected: 'root', method: 'm' })
    flow.getState().pinForward('a')
    await flush()
    flow.setState({ selected: 'a' })
    flow.getState().pinBackward('essence/essence-of-horror', 'b')
    await flush()
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root', 'a', 'b'])

    flow.getState().unpin('a')
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root'])
    expect(flow.getState().edges).toEqual([])
    expect(flow.getState().selected).toBe('root')
  })

  it('keeps a detached node out of the chart when it rejoins', async () => {
    const { flow } = await store()
    flow.setState({ selected: 'root', method: 'm' })
    flow.getState().pinForward('a')
    await flush()
    flow.setState({ selected: 'root', method: 'm' })
    flow.getState().pinForward('b')
    await flush()
    flow.getState().unpin('b')
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root', 'a'])
  })
})
