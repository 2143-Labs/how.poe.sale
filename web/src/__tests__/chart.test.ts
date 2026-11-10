import { describe, expect, it } from 'vitest'
import type { Client } from '../client'
import { chainFrom } from '../flow/chain'
import { SCOPES } from '../flow/scope'
import { createFlowStore } from '../flow/store'
import { encodeShare } from '../share'
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

function nodeOf(id: string, spec: AbsSpec = root): NodeView {
  return {
    id,
    spec,
    view: {
      base: spec.base,
      itemLevel: spec.itemLevel,
      rarity: spec.rarity,
      corrupted: spec.corrupted,
      influences: spec.influences,
      prefixes: spec.prefixes.map((slot) => slot.id),
      suffixes: spec.suffixes.map((slot) => slot.id),
      anyPrefixes: spec.anyPrefixes,
      anySuffixes: spec.anySuffixes,
    },
    members: null,
    approx: false,
    source: null,
  }
}

/** A client whose every chart answer is immediate, so a test can wait one tick and read state. */
class FakeClient implements Client {
  pinned: PinnedView[] = []
  /** The item each pinned node stands for; nodes named here and nowhere else are this item. */
  specs = new Map<string, AbsSpec>()

  node(id: string): NodeView {
    return nodeOf(id, this.specs.get(id) ?? root)
  }

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

  async chartForward(node: string) {
    // A method that leaves the card where it is, so two cards compare equal only when they are.
    return {
      node,
      method: 'm',
      name: 'M',
      cost: 'none',
      simulated: null,
      repeatable: false,
      outcomes: [{ key: node, kind: 'item' as const, p: 1, interval: null, view: this.node(node).view, reason: null }],
      watched: [],
    }
  }

  async chartMadeBy(node: string) {
    return { node, inputs: [], skipped: [] }
  }

  async chartPinForward(node: string, method: string, key: string) {
    const edge: EdgeView = { from: node, to: key, method, name: 'M', p: 0.5, interval: null, repeatable: false }
    const pinned: PinnedView = { node: this.node(key), edge }
    this.pinned.push(pinned)
    return pinned
  }

  async chartPinBackward(node: string, method: string, key: string) {
    const edge: EdgeView = { from: key, to: node, method, name: 'M', p: 0.5, interval: null, repeatable: false }
    const pinned: PinnedView = { node: this.node(key), edge }
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
    await flow.getState().merge('b', 'a')
    const state = flow.getState()
    expect(state.nodes.map((node) => node.id)).toEqual(['root', 'a'])
    expect(state.edges).toHaveLength(1)
    expect(state.edges[0]).toMatchObject({ from: 'root', to: 'a', p: 1 })
    expect(state.rawNodes).toHaveLength(3)
    expect(state.merges).toEqual([{ from: 'b', into: 'a' }])

    flow.setState({ selected: 'root', method: 'n' })
    await flow.getState().pinAll(['b'])
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root', 'a'])

    await flow.getState().merge('root', 'a')
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
    await flow.getState().merge('b', 'a')
    expect(saved.get(SCOPES.simple.storageKey)).toContain('"merges":[{"from":"b","into":"a"}]')

    const again = createFlowStore(SCOPES.simple, { client: new FakeClient(), hash: '', storage, dropHash: () => undefined })
    // the replay is fire-and-forget: wait for its last step, the merge, to land
    for (let tick = 0; tick < 100 && again.getState().merges.length === 0; tick++) await flush()
    expect(again.getState().nodes.map((node) => node.id)).toEqual(['root', 'a'])
    expect(again.getState().notice).toBeNull()
  })

  it('refuses a merge with a card that is not on the chart', async () => {
    const { flow } = await store()
    flow.setState({ selected: 'root', method: 'm' })
    await flow.getState().pinAll(['a', 'b'])
    await flow.getState().merge('b', 'a')
    // `b` is folded away, so neither end of a second merge with it is a card any more.
    const error = 'Only two different items on the chart can be merged.'
    await flow.getState().merge('b', 'a')
    expect(flow.getState().error).toBe(error)
    await flow.getState().merge('a', 'a')
    expect(flow.getState().error).toBe(error)
    expect(flow.getState().merges).toEqual([{ from: 'b', into: 'a' }])
  })

  it('drops a saved merge that no longer applies, and says so', async () => {
    const saved = new Map<string, string>()
    const storage = {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => void saved.set(key, value),
    }
    // `zzz` was never pinned, so there is no card to fold.
    saved.set(
      SCOPES.simple.storageKey,
      JSON.stringify({ root, watch: [], pins: [], merges: [{ from: 'zzz', into: 'root' }] }),
    )
    const flow = createFlowStore(SCOPES.simple, { client: new FakeClient(), hash: '', storage, dropHash: () => undefined })
    for (let tick = 0; tick < 100 && flow.getState().nodes.length === 0; tick++) await flush()
    for (let tick = 0; tick < 100 && flow.getState().merges.length > 0; tick++) await flush()
    expect(flow.getState().merges).toEqual([])
    expect(flow.getState().notice).toBe('1 saved merge no longer applies and was dropped.')
  })

  it('drops the second half of a saved pair of merges that would close a ring', async () => {
    const saved = new Map<string, string>()
    const storage = {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => void saved.set(key, value),
    }
    const pins = [
      { dir: 'f' as const, node: 'root', method: 'm', key: 'a' },
      { dir: 'f' as const, node: 'root', method: 'm', key: 'b' },
    ]
    saved.set(
      SCOPES.simple.storageKey,
      JSON.stringify({ root, watch: [], pins, merges: [{ from: 'b', into: 'a' }, { from: 'a', into: 'b' }] }),
    )
    const flow = createFlowStore(SCOPES.simple, { client: new FakeClient(), hash: '', storage, dropHash: () => undefined })
    for (let tick = 0; tick < 200 && flow.getState().pins.length < 2; tick++) await flush()
    for (let tick = 0; tick < 200 && flow.getState().merges.length === 0; tick++) await flush()
    // Folding both ways would hide both cards; the second merge is refused instead.
    expect(flow.getState().merges).toEqual([{ from: 'b', into: 'a' }])
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root', 'a'])
    expect(flow.getState().notice).toBe('1 saved merge no longer applies and was dropped.')
  })

  it('wraps a step back onto the root by folding its card into the root', async () => {
    const { flow } = await store()
    flow.setState({ selected: 'root', method: 'm' })
    flow.getState().pinForward('a')
    await flush()
    // Folding `a` into the root turns the root's step to `a` into a step back to the root.
    await flow.getState().merge('a', 'root')
    expect(flow.getState().merges).toEqual([{ from: 'a', into: 'root' }])
    expect(flow.getState().edges.map((e) => [e.from, e.to, e.p])).toEqual([['root', 'root', 0.5]])
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root'])
    // Only half the root's outcomes are pinned, so every run leaves the pinned steps at some point.
    expect(chainFrom(flow.getState().edges, 'root')).toMatchObject({ kind: 'totals', ends: [], off: 1 })
  })

  it('forgets the merges when the chart is cleared', async () => {
    const { flow } = await store()
    flow.setState({ selected: 'root', method: 'm' })
    await flow.getState().pinAll(['a', 'b'])
    await flow.getState().merge('b', 'a')
    flow.getState().clearChart()
    expect(flow.getState().nodes.map((node) => node.id)).toEqual(['root'])
    expect(flow.getState().merges).toEqual([])
    expect(flow.getState().rawNodes.map((node) => node.id)).toEqual(['root'])
  })

  it('keeps a row whole when two outcomes land on one card', async () => {
    const { flow } = await store()
    // Two outcomes of the root, both landing on `a` once `b` is folded in: the row stays 100%.
    flow.setState({ selected: 'root', method: 'm' })
    await flow.getState().pinAll(['a', 'b'])
    await flow.getState().merge('b', 'a')
    expect(flow.getState().edges.map((e) => [e.from, e.to, e.p])).toEqual([['root', 'a', 1]])
    const whole = chainFrom(flow.getState().edges, 'root')
    if (whole.kind !== 'totals') throw new Error(`expected totals, got ${whole.kind}`)
    expect(whole.off).toBe(0)
  })

  it('refuses to fold a card the chart still has steps pinned from', async () => {
    const { flow } = await store()
    flow.setState({ selected: 'root', method: 'm' })
    await flow.getState().pinAll(['a'])
    flow.setState({ selected: 'a', method: 'm' })
    flow.getState().pinForward('c')
    await flush()
    // Folding `a` into `c` would give `c` nothing to follow, stopping a craft that goes on.
    await flow.getState().merge('a', 'c')
    expect(flow.getState().error).toBe('These items cannot be merged: one still has steps pinned from it.')
    expect(flow.getState().merges).toEqual([])
  })

  it('refuses a fold that would drop a modifier the chart tracks', async () => {
    const client = new FakeClient()
    client.specs.set('hit', { ...root, prefixes: [{ id: 'SimpleLife4' }], anyPrefixes: 1 })
    // Tracking "+40 to maximum Life": the hit names it, the plain card does not.
    const hash = `#s=${encodeShare('simple', { root, watch: [{ id: 'SimpleLife4' }], pins: [], merges: [] })}`
    const flow = createFlowStore(SCOPES.simple, { client, hash, storage: null, dropHash: () => undefined })
    await flush()
    flow.setState({ selected: 'root', method: 'm' })
    await flow.getState().pinAll(['hit', 'miss'])
    await flow.getState().merge('hit', 'miss')
    expect(flow.getState().error).toBe('These items cannot be merged: it would drop a modifier the chart tracks.')
    expect(flow.getState().merges).toEqual([])
  })
})
