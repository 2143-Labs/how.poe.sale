/// <reference types="node" />
import { expect, it } from 'vitest'
import { chainFrom } from '../flow/chain'
import { mergeBlocker } from '../flow/merge'
import { SCOPES } from '../flow/scope'
import { createFlowStore } from '../flow/store'
import { encodeShare } from '../share'
import { normalizeSpec, type AbsSpec, type NodeView } from '../types'
import { simpleClient } from './bridge'

const LIFE = 'SimpleLife4'
const VEILED = ['VeiledPrefix', 'VeiledSuffix']
const LOCK = 'bench/add/StrMasterItemGenerationCannotChangePrefixes'

const root: AbsSpec = {
  base: 'Helmet',
  itemLevel: 87,
  rarity: 'rare',
  influences: [],
  prefixes: [],
  suffixes: [],
  anyPrefixes: 2,
  anySuffixes: 2,
  corrupted: false,
}

function spec(node: NodeView): AbsSpec {
  const read = normalizeSpec(node.spec)
  if (!read) throw new Error(`unreadable spec on ${node.id}`)
  return read
}

const named = (node: NodeView) => [...spec(node).prefixes, ...spec(node).suffixes].map((slot) => slot.id)
const hasLife = (node: NodeView) => named(node).includes(LIFE)
const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0))

async function until(done: () => boolean) {
  for (let i = 0; i < 1000 && !done(); i++) await tick()
  expect(done()).toBe(true)
}

it('loops, merges and folds on the simple game', { timeout: 120_000 }, async () => {
  const client = await simpleClient()
  const hash = `#s=${encodeShare('simple', { root, watch: [{ id: LIFE }], pins: [], merges: [] })}`
  const flow = createFlowStore(SCOPES.simple, { client, hash, storage: null, dropHash: () => undefined })
  await until(() => flow.getState().nodes.length === 1)
  const rootId = flow.getState().nodes[0].id

  /** Pins every item outcome of `method` from `node`, merging alike when asked. */
  const pinAll = async (node: string, method: string, merging = false) => {
    const applied = await client.chartForward(node, method)
    const keys = applied.outcomes.filter((o) => o.kind === 'item').map((o) => o.key)
    flow.setState({ selected: node, method })
    await (merging ? flow.getState().pinAllMerging(keys) : flow.getState().pinAll(keys))
    return keys
  }

  // 1. Wrap-around: Chaos on the root gives the root back as one of its outcomes.
  const chaos = await pinAll(rootId, 'currency/chaos')
  expect(chaos).toHaveLength(12)
  expect(chaos).toContain(rootId)
  const self = flow.getState().edges.find((e) => e.from === rootId && e.to === rootId)
  expect(self?.p).toBeGreaterThan(0.2)
  expect(self?.p).toBeLessThan(0.27)
  expect(flow.getState().nodes).toHaveLength(12)

  // 2. Chaos from every other miss closes the loop: the craft only ends on a hit.
  const misses = flow.getState().nodes.filter((n) => n.id !== rootId && !hasLife(n))
  expect(misses).toHaveLength(5)
  for (const miss of misses) await pinAll(miss.id, 'currency/chaos')
  expect(flow.getState().nodes).toHaveLength(12)
  const chain = chainFrom(flow.getState().edges, rootId)
  if (chain.kind !== 'totals') throw new Error(`expected totals, got ${chain.kind}`)
  expect(chain.off).toBeLessThan(1e-6)
  const nodes = flow.getState().nodes
  for (const end of chain.ends) expect(hasLife(nodes.find((n) => n.id === end.node)!)).toBe(true)
  expect(chain.ends.reduce((sum, end) => sum + end.p, 0)).toBeCloseTo(1, 6)
  expect(chain.uses[0].method).toBe('currency/chaos')
  expect(chain.uses[0].n).toBeGreaterThan(12)
  expect(chain.uses[0].n).toBeLessThan(15)

  // 3. Veiled Chaos, merging alike: every veiled card folds into a card already on the chart.
  const veiled = await pinAll(rootId, 'currency/veiled-chaos', true)
  expect(veiled).toHaveLength(23)
  const state = flow.getState()
  expect(state.nodes).toHaveLength(12)
  expect(state.merges).toHaveLength(23)
  expect(state.nodes.some((n) => named(n).some((id) => VEILED.includes(id)))).toBe(false)
  for (const merge of state.merges) {
    const from = state.rawNodes.find((n) => n.id === merge.from)!
    const into = state.rawNodes.find((n) => n.id === merge.into)!
    expect(hasLife(into)).toBe(hasLife(from))
  }

  // 4. By hand: the bench lock has the plain card's counts, and folding it in is refused, because
  // Chaos would keep the locked side and give one item two different sets of odds.
  const [lockKey] = await pinAll(rootId, LOCK)
  const lock = flow.getState().nodes.find((n) => n.id === lockKey)!
  expect(spec(lock)).toMatchObject({ prefixes: [], anyPrefixes: 2, anySuffixes: 2 })
  expect(spec(lock).suffixes).toHaveLength(1)
  const plain = flow
    .getState()
    .nodes.find((n) => {
      const s = spec(n)
      return named(n).length === 0 && s.anyPrefixes === 2 && s.anySuffixes === 3
    })!
  const before = flow.getState().merges
  flow.getState().merge(lock.id, plain.id)
  expect(flow.getState().error).toBe('These items cannot be merged: only one carries a crafted metamod.')
  flow.getState().merge(plain.id, lock.id)
  expect(flow.getState().error).toBe('These items cannot be merged: the target names a modifier this item lacks.')
  expect(flow.getState().merges).toBe(before)
  expect(flow.getState().nodes.some((n) => n.id === lock.id)).toBe(true)

  client.dispose()
})

it('folds one card by hand on a real chart', { timeout: 120_000 }, async () => {
  const client = await simpleClient()
  const flow = createFlowStore(SCOPES.simple, {
    client,
    hash: `#s=${encodeShare('simple', { root, watch: [{ id: LIFE }], pins: [], merges: [] })}`,
    storage: null,
    dropHash: () => undefined,
  })
  await until(() => flow.getState().nodes.length === 1)
  const rootId = flow.getState().nodes[0].id

  // A Chaos chart first, so there are plain cards for a veiled card to fold into.
  const pick = async (method: string) => {
    const applied = await client.chartForward(rootId, method)
    const keys = applied.outcomes.filter((o) => o.kind === 'item').map((o) => o.key)
    flow.setState({ selected: rootId, method })
    await flow.getState().pinAll(keys)
  }
  await pick('currency/chaos')
  expect(flow.getState().nodes).toHaveLength(12)
  await pick('currency/veiled-chaos')
  const state = flow.getState()
  expect(state.nodes).toHaveLength(35)
  expect(state.merges).toEqual([])

  const veiled = state.nodes.find((n) => named(n).some((id) => VEILED.includes(id)))!
  const target = state.nodes.find(
    (n) => n.id !== veiled.id && named(n).every((id) => !VEILED.includes(id)) && mergeBlocker(veiled, n) === null,
  )!
  flow.getState().merge(veiled.id, target.id)
  expect(flow.getState().error).toBeNull()
  expect(flow.getState().merges).toEqual([{ from: veiled.id, into: target.id }])
  expect(flow.getState().nodes.some((n) => n.id === veiled.id)).toBe(false)
  expect(flow.getState().rawNodes.some((n) => n.id === veiled.id)).toBe(true)
  client.dispose()
})
