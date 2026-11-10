/// <reference types="node" />
// The rule behind folding two cards: they may share one node only when the method followed next
// cannot tell them apart. That is the Kemeny-Snell condition on the chart's Markov chain, and the
// engine answers it directly: compare the two cards' rows, class by class.
import { expect, it } from 'vitest'
import type { Client } from '../client'
import { basicsBlocker, mergeBlocker } from '../flow/merge'
import { rowsDiffer, rowsOf, type Row } from '../flow/rows'
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

const countsOf = (node: NodeView) => [
  node.view.prefixes.length + node.view.anyPrefixes,
  node.view.suffixes.length + node.view.anySuffixes,
]
const namesOf = (node: NodeView) => [...node.view.prefixes, ...node.view.suffixes]
const hasLife = (node: NodeView) => namesOf(node).includes('+40 to maximum Life')

async function rowOf(client: Client, node: string, method: string): Promise<Row> {
  return rowsOf(await client.chartForward(node, method))
}

/** Everything Chaos leaves from the root, deduplicated by class. */
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

it('the six cards Chaos leaves are one card as far as Chaos can tell', { timeout: 120_000 }, async () => {
  const client = await simpleClient()
  const { nodes } = await chaosBoard(client)
  const plain = nodes.filter((node) => namesOf(node).length === 0)
  expect(plain).toHaveLength(6)

  // A full reforge only sees the mods that survive it, so the counts are invisible to Chaos: every
  // pair of these cards gives the same row, within sampling noise.
  const rows = await Promise.all(plain.map((node) => rowOf(client, node.id, 'currency/chaos')))
  for (let i = 0; i < rows.length; i++) {
    for (let j = i + 1; j < rows.length; j++) {
      const diff = rowsDiffer(rows[i], rows[j])
      expect(diff === null || diff.gap < 0.01).toBe(true)
    }
  }

  // Yet no two of them are the same item, so the item rule would keep all six apart: only the
  // engine's own answer lets them fold.
  expect(new Set(plain.map((node) => countsOf(node).join('/'))).size).toBe(6)
  for (const a of plain) {
    for (const b of plain) {
      if (a.id !== b.id) expect(mergeBlocker(a, b)).toMatch(/^different number of (prefixes|suffixes)$/)
      expect(basicsBlocker(a, b)).toBeNull()
    }
  }
  client.dispose()
})

it('the engine, not the items, refuses the fold once a card has a plan', { timeout: 120_000 }, async () => {
  const client = await simpleClient()
  const { start, nodes } = await chaosBoard(client)
  const applied = await client.chartForward(start.id, LOCK_PREFIXES)
  const lock = await client.chartPinForward(start.id, LOCK_PREFIXES, applied.outcomes[0].key)
  const plain = nodes.find((node) => namesOf(node).length === 0 && countsOf(node).join('/') === '2/3')!

  // Same base, same counts, nothing the shape test can object to.
  expect(basicsBlocker(lock.node, plain)).toBeNull()
  expect(mergeBlocker(lock.node, plain)).toBe('only one carries a crafted metamod')

  // The engine sees the lock freeze the prefix side.
  const chaos = rowsDiffer(await rowOf(client, lock.node.id, 'currency/chaos'), await rowOf(client, plain.id, 'currency/chaos'))
  expect(chaos!.gap).toBeGreaterThan(0.25)
  const annul = rowsDiffer(
    await rowOf(client, lock.node.id, 'currency/annulment'),
    await rowOf(client, plain.id, 'currency/annulment'),
  )
  expect(annul!.gap).toBeGreaterThan(0.2)
  expect(chaos!.at).toBeTruthy()
  expect(annul!.at).toBeTruthy()
  client.dispose()
})

it('a destination is not a state: two different hits stay two cards', { timeout: 120_000 }, async () => {
  const client = await simpleClient()
  const { nodes } = await chaosBoard(client)
  const hits = nodes.filter(hasLife)
  expect(hits).toHaveLength(6)

  // Every card's row is the same under Chaos, hits included, so the row test alone would let a hit
  // fold into a miss and the craft would carry on where the player meant to stop. Destinations are
  // held to the item rule instead.
  const a = await rowOf(client, hits[0].id, 'currency/chaos')
  const b = await rowOf(client, hits[1].id, 'currency/chaos')
  expect(rowsDiffer(a, b)).toBeNull()
  expect(mergeBlocker(hits[0], hits[1])).toMatch(/^different number of (prefixes|suffixes)$/)
  client.dispose()
})
