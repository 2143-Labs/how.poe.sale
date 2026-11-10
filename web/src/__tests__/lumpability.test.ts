/// <reference types="node" />
// Adversarial checks on the claim behind merging: two cards may share one node only when the
// method followed next cannot tell them apart. The test is the Kemeny-Snell condition read off
// the engine: for the cards a merge would fold, the chance of landing in every target group must
// agree. Where it does not agree, the merge would be a lie, and the numbers below are the proof.
import { expect, it } from 'vitest'
import type { Client } from '../client'
import { mergeBlocker } from '../flow/merge'
import type { AbsSpec, NodeView } from '../types'
import { simpleClient } from './bridge'

const LIFE = 'SimpleLife4'
const LOCK_PREFIXES = 'bench/add/StrMasterItemGenerationCannotChangePrefixes'

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

/** A row of outcomes keyed by class, so two cards' rows line up. */
async function row(client: Client, node: string, method: string) {
  const applied = await client.chartForward(node, method)
  const out = new Map<string, { p: number; width: number }>()
  for (const outcome of applied.outcomes) {
    if (outcome.kind !== 'item' || !outcome.view) continue
    const { prefixes, suffixes, anyPrefixes, anySuffixes } = outcome.view
    const key = JSON.stringify([prefixes, suffixes, anyPrefixes, anySuffixes])
    const width = outcome.interval ? (outcome.interval[1] - outcome.interval[0]) / 2 : 0
    const seen = out.get(key)
    out.set(key, { p: (seen?.p ?? 0) + outcome.p, width: Math.max(seen?.width ?? 0, width) })
  }
  return out
}

/** The largest gap between two rows, over the classes either row has. */
function gap(a: Map<string, { p: number }>, b: Map<string, { p: number }>) {
  let worst = 0
  let at = ''
  for (const key of new Set([...a.keys(), ...b.keys()])) {
    const d = Math.abs((a.get(key)?.p ?? 0) - (b.get(key)?.p ?? 0))
    if (d > worst) {
      worst = d
      at = key
    }
  }
  return { worst, at }
}

const countsOf = (node: NodeView) => [
  node.view.prefixes.length + node.view.anyPrefixes,
  node.view.suffixes.length + node.view.anySuffixes,
]
const namesOf = (node: NodeView) => [...node.view.prefixes, ...node.view.suffixes]

/** Everything Chaos leaves from the root: the root, five other lock-free cards, six Life hits. */
async function chaosBoard(client: Client) {
  const start = await client.chartStart(root, [{ id: LIFE }])
  const applied = await client.chartForward(start.id, 'currency/chaos')
  const nodes: NodeView[] = [start]
  for (const outcome of applied.outcomes) {
    if (outcome.kind !== 'item') continue
    const pinned = await client.chartPinForward(start.id, 'currency/chaos', outcome.key)
    if (!nodes.some((node) => node.id === pinned.node.id)) nodes.push(pinned.node)
  }
  return { start, nodes }
}

it('Chaos cannot tell apart two cards it reforges the same way', { timeout: 120_000 }, async () => {
  const client = await simpleClient()
  const { start, nodes } = await chaosBoard(client)
  expect(nodes).toHaveLength(12)
  const plain = nodes.filter((node) => namesOf(node).length === 0)
  expect(plain).toHaveLength(6)

  // A full reforge only sees base, item level, rarity and the mods that survive it: with none of
  // those differing, every card Chaos leaves behaves like the root under another Chaos. That is
  // what makes a miss a legitimate place to loop back from.
  const base = await row(client, start.id, 'currency/chaos')
  for (const node of plain) {
    const other = await row(client, node.id, 'currency/chaos')
    const { worst } = gap(base, other)
    expect(worst).toBeLessThan(0.01)
  }

  // The six cards have six different count shapes, so the merge rule refuses all of them anyway.
  expect(new Set(plain.map((node) => countsOf(node).join('/'))).size).toBe(6)
  client.dispose()
})

it('a crafted metamod breaks the claim, so no merge may drop one', { timeout: 120_000 }, async () => {
  const client = await simpleClient()
  const { start, nodes } = await chaosBoard(client)
  const applied = await client.chartForward(start.id, LOCK_PREFIXES)
  const lock = await client.chartPinForward(start.id, LOCK_PREFIXES, applied.outcomes[0].key)
  // Same counts as one of the plain cards: two prefixes, three suffixes.
  expect(countsOf(lock.node)).toEqual([2, 3])
  const plain = nodes.find((node) => namesOf(node).length === 0 && countsOf(node).join('/') === '2/3')
  expect(plain).toBeDefined()

  // With the lock, Chaos keeps the prefix side and reforges the suffixes; without it, the prefixes
  // move too. Two cards this far apart must never share a node, which is why the guard refuses it.
  const withLock = await row(client, lock.node.id, 'currency/chaos')
  const without = await row(client, plain!.id, 'currency/chaos')
  const chaosGap = gap(withLock, without)
  expect(chaosGap.worst).toBeGreaterThan(0.25)
  expect(chaosGap.at).toBeTruthy()

  // Annulment tells them apart as well: it may not take a protected modifier freely.
  const annulLock = await row(client, lock.node.id, 'currency/annulment')
  const annulPlain = await row(client, plain!.id, 'currency/annulment')
  expect(gap(annulLock, annulPlain).worst).toBeGreaterThan(0.2)

  expect(mergeBlocker(lock.node, plain!)).toBe('only one carries a crafted metamod')
  expect(mergeBlocker(plain!, lock.node)).toBe('the target names a modifier this item lacks')
  client.dispose()
})

it('refuses every fold among the six cards Chaos leaves', { timeout: 120_000 }, async () => {
  const client = await simpleClient()
  const { nodes } = await chaosBoard(client)
  const plain = nodes.filter((node) => namesOf(node).length === 0)
  for (const a of plain) {
    for (const b of plain) {
      if (a.id === b.id) continue
      expect(mergeBlocker(a, b)).toMatch(/^different number of (prefixes|suffixes)$/)
    }
  }
  client.dispose()
})
