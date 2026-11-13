/// <reference types="node" />
// Every chart page a `.link` file describes, rebuilt here from its pins and compared with the
// checked-in snapshot: the cards, the paced steps and the chain's totals. A page is exported by
// pressing Share and saving the copied link as `src/__tests__/charts/<name>.link`.
//
// A snapshot that no longer matches is a finding: read the diff, decide whether the engine or the
// page is wrong, and only then `npm test -- -u`.
import { readdirSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it, vi } from 'vitest'
import { chainFrom } from '../flow/chain'
import { SCOPES } from '../flow/scope'
import { createFlowStore, type FlowStore } from '../flow/store'
import { parseShareHash } from '../share'
import { clientFor } from './bridge'

const DIR = fileURLToPath(new URL('./charts', import.meta.url))

const cases = readdirSync(DIR)
  .filter((name) => name.endsWith('.link'))
  .sort()

/** The report's tolerance: engine dust on an exact number is not a finding. */
const round = (x: number) => Math.round(x * 1e9) / 1e9

/** The share fragment inside a `.link` file, whether it holds a whole URL or only the fragment. */
function fragmentOf(name: string): string {
  const text = readFileSync(`${DIR}/${name}`, 'utf8').trim()
  const at = text.indexOf('#')
  return at === -1 ? text : text.slice(at)
}

/**
 * Waits for the replay's pins and for the refresh that follows them. A replay that failed stops the
 * wait early: the caller reports its error or notice rather than a timeout.
 */
async function replayed(flow: FlowStore, pins: number): Promise<void> {
  await vi.waitFor(
    () => {
      const state = flow.getState()
      if (state.error || state.notice) return
      // `restart` selects and refreshes the last card once its pins land; `methods` is that answer.
      expect(state.pins).toHaveLength(pins)
      expect(state.methods.length).toBeGreaterThan(0)
    },
    { timeout: 60_000 },
  )
}

describe('charts kept as .link files', () => {
  it('has at least one chart', () => {
    expect(cases).not.toHaveLength(0)
  })

  for (const name of cases) {
    it(name.replace(/\.link$/, ''), { timeout: 180_000 }, async () => {
      const share = parseShareHash(fragmentOf(name))
      if (share.kind !== 'ok') throw new Error(`${name}: ${share.kind} share link`)
      const flow = createFlowStore(SCOPES[share.scope], {
        client: await clientFor(share.scope),
        hash: fragmentOf(name),
        storage: null,
        dropHash: () => undefined,
      })
      await replayed(flow, share.pins.length)
      const state = flow.getState()
      expect(state.notice).toBeNull()
      expect(state.error).toBeNull()
      expect(state.nodes.length).toBeGreaterThan(0)
      // The link is what the test is: a page that replays fewer steps than it holds has lost one.
      expect(state.pins).toHaveLength(share.pins.length)

      // Cards are named by their place on the chart, so a step reads as "from card 0 to card 3".
      const at = new Map(state.nodes.map((node, i) => [node.id, i]))
      const card = (id: string) => {
        const i = at.get(id)
        if (i === undefined) throw new Error(`a step points at a card that is not on the chart: ${id}`)
        return i
      }
      const chain = chainFrom(state.edges, state.nodes[0].id)

      expect(chain.kind).not.toBe('none')
      if (chain.kind === 'totals') expect(chain.off).toBeLessThan(1e-6)

      const summary = {
        root: state.root,
        watch: state.watch,
        cards: state.nodes.map((node) => ({
          view: node.view,
          approx: node.approx,
          members: node.members,
        })),
        steps: state.edges.map((edge) => ({
          from: card(edge.from),
          method: edge.method,
          name: edge.name,
          p: round(edge.p),
          to: card(edge.to),
          repeatable: edge.repeatable,
        })),
        merges: state.merges.map((merge) => ({ from: at.get(merge.from) ?? merge.from, into: at.get(merge.into) ?? merge.into })),
        chain:
          chain.kind === 'totals'
            ? {
                kind: chain.kind,
                off: round(chain.off),
                approx: chain.approx,
                ends: chain.ends.map((end) => ({ card: card(end.node), p: round(end.p) })),
                uses: chain.uses.map((use) => ({ method: use.method, name: use.name, n: round(use.n) })),
                ignored: chain.ignored.map((entry) => ({ card: card(entry.node), follows: entry.follows, others: entry.others })),
              }
            : chain,
      }
      expect(summary).toMatchSnapshot()
    })
  }
})
