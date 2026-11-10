import { describe, expect, it } from 'vitest'
import type { Client } from '../client'
import { createFlowStore } from '../flow/store'
import { SCOPES } from '../flow/scope'
import type { AbsSpec, EdgeView, ItemView, MethodOption, NodeView, PinnedView } from '../types'

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

  async chartMethods(): Promise<MethodOption[]> {
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

  async chartPinForward(node: string, method: string, key: string) {
    const edge: EdgeView = { from: node, to: key, method, name: 'M', p: 0.5, interval: null, repeatable: false }
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

/** A client whose method lists wait until the test releases them. */
class SlowMethods extends FakeClient {
  release: () => void = () => undefined
  async chartMethods(): Promise<MethodOption[]> {
    await new Promise<void>((resolve) => {
      this.release = resolve
    })
    return [{ id: 'm', name: 'M', family: 'currency', cost: '1 × M' }]
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

  it('pins every outcome and stays on the source', async () => {
    const { flow } = await store()
    flow.setState({ selected: 'root', method: 'm' })
    await flow.getState().pinAll(['a', 'b', 'a'])
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root', 'a', 'b'])
    expect(flow.getState().edges).toHaveLength(2)
    expect(flow.getState().selected).toBe('root')
    expect(flow.getState().pins).toHaveLength(2)
  })

  it("keeps a method chosen while the next node's methods load", async () => {
    const client = new SlowMethods()
    const flow = createFlowStore(SCOPES.simple, { client, hash: '', storage: null, dropHash: () => undefined })
    await flush()
    client.release()
    await flush()
    expect(flow.getState().methods).toHaveLength(1)
    flow.setState({ method: 'm' })
    flow.getState().pinForward('a')
    await flush()
    expect(flow.getState().selected).toBe('a')
    expect(flow.getState().methods).toEqual([])
    flow.getState().chooseMethod('k')
    await flush()
    client.release()
    await flush()
    expect(flow.getState().method).toBe('k')
    expect(flow.getState().applied).not.toBeNull()
    expect(flow.getState().methods).toHaveLength(1)
  })

  it('keeps only the pins a reload can replay after a removal', async () => {
    const { flow } = await store()
    const pin = async (from: string, key: string) => {
      flow.setState({ selected: from, method: 'm' })
      flow.getState().pinForward(key)
      await flush()
    }
    await pin('root', 'a')
    await pin('a', 'b')
    await pin('b', 'c')
    await pin('root', 'b')
    flow.getState().unpin('a')
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root', 'b', 'c'])
    expect(flow.getState().pins).toEqual([
      { dir: 'f', node: 'root', method: 'm', key: 'b' },
      { dir: 'f', node: 'b', method: 'm', key: 'c' },
    ])
  })

  it('folds a merged card into the card it merged into', async () => {
    const { flow } = await store()
    flow.setState({ selected: 'root', method: 'm' })
    await flow.getState().pinAll(['a', 'b'])
    flow.getState().merge('b', 'a')
    const state = flow.getState()
    expect(state.nodes.map((node) => node.id)).toEqual(['root', 'a'])
    expect(state.edges).toHaveLength(1)
    expect(state.edges[0]).toMatchObject({ from: 'root', to: 'a', p: 1 })
    expect(state.rawNodes).toHaveLength(3)
    expect(state.merges).toEqual([{ from: 'b', into: 'a' }])

    flow.setState({ selected: 'root', method: 'n' })
    await flow.getState().pinAll(['b'])
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root', 'a'])

    flow.getState().merge('root', 'a')
    expect(flow.getState().error).toBe('The root cannot be merged into another item.')

    flow.getState().unpin('a')
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root'])
    expect(flow.getState().merges).toEqual([])
    expect(flow.getState().pins).toEqual([])
  })

  it('saves merges and opens them again', async () => {
    const saved = new Map<string, string>()
    const storage = {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => void saved.set(key, value),
    }
    const flow = createFlowStore(SCOPES.simple, { client: new FakeClient(), hash: '', storage, dropHash: () => undefined })
    await flush()
    flow.setState({ selected: 'root', method: 'm' })
    await flow.getState().pinAll(['a', 'b'])
    flow.getState().merge('b', 'a')
    expect(saved.get(SCOPES.simple.storageKey)).toContain('"merges":[{"from":"b","into":"a"}]')

    const again = createFlowStore(SCOPES.simple, { client: new FakeClient(), hash: '', storage, dropHash: () => undefined })
    // the replay is fire-and-forget: wait for its last step, the merge, to land
    for (let tick = 0; tick < 100 && again.getState().merges.length === 0; tick++) await flush()
    expect(again.getState().nodes.map((node) => node.id)).toEqual(['root', 'a'])
    expect(again.getState().notice).toBeNull()
  })
})
