import { useMemo, useState } from 'react'
import type { AppliedView, InputView, NodeView, OutcomeView, WatchView } from '../types'
import { chainFrom, type Chain } from './chain'
import { errorText, useFlow, useFlowState } from './context'
import { count, percent, range, summary, tries } from './format'
import { foldShape, mergedInto, resolveId } from './merge'
import { pathChance } from './path'

/** The most outcome rows the table shows. */
const ROWS = 100

function OutcomeRow({
  outcome,
  repeatable,
  onChart,
  onPin,
}: {
  outcome: OutcomeView
  repeatable: boolean
  onChart: boolean
  onPin: (key: string) => void
}) {
  return (
    <tr>
      <td>
        {outcome.view ? summary(outcome.view) : (outcome.reason ?? outcome.kind)}
        {onChart && <span className="badge badge-expanded">on chart</span>}
      </td>
      <td className="num">
        {outcome.interval ? '≈ ' : ''}
        {percent(outcome.p)}
        {range(outcome.interval)}
      </td>
      <td className="num">{repeatable ? tries(outcome.p) : '—'}</td>
      <td>
        {outcome.kind === 'item' && (
          <button type="button" onClick={() => onPin(outcome.key)}>
            Pin
          </button>
        )}
      </td>
    </tr>
  )
}

/**
 * What the step gives each watched modifier: the outcome rows split by every tracked slot, so these are their sums. A
 * simulated chance no run hit shows its upper bound rather than "never".
 */
function Watched({
  watched,
  simulated,
  repeatable,
}: {
  watched: WatchView[]
  simulated: number | null
  repeatable: boolean
}) {
  const chance = (w: WatchView) => {
    if (w.p === 0 && w.interval && simulated) {
      return `not seen in ${simulated.toLocaleString('en-US')} runs (95%: under ${percent(w.interval[1])})`
    }
    return `${w.interval ? '≈ ' : ''}${percent(w.p)}${range(w.interval)}`
  }
  const tried = (w: WatchView) => {
    if (w.p === 0) return w.interval ? '—' : 'never'
    return repeatable ? tries(w.p) : '—'
  }
  return (
    <table className="steps-table">
      <thead>
        <tr>
          <th>Holds</th>
          <th>Chance</th>
          <th>Tries</th>
        </tr>
      </thead>
      <tbody>
        {watched.map((w) => (
          <tr key={w.label}>
            <td>{w.label}</td>
            <td className="num">{chance(w)}</td>
            <td className="num">{tried(w)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function Applied({ applied }: { applied: AppliedView }) {
  const { store } = useFlow()
  const nodes = useFlowState((state) => state.nodes)
  const rawNodes = useFlowState((state) => state.rawNodes)
  const merges = useFlowState((state) => state.merges)
  const rows = applied.outcomes
  const shown = rows.slice(0, ROWS)
  const rest = rows.slice(ROWS)
  const items = rows.filter((o) => o.kind === 'item')
  const folded = mergedInto(rawNodes, merges, applied.node).length
  return (
    <>
      <p className="hint">
        {applied.name} · {applied.cost}
      </p>
      {applied.simulated && (
        <p className="hint">
          Simulated: {applied.simulated} seeded reforges; outcomes rarer than 1 in {applied.simulated} may be missing.
        </p>
      )}
      {applied.watched.length > 0 && (
        <Watched watched={applied.watched} simulated={applied.simulated} repeatable={applied.repeatable} />
      )}
      {folded > 0 && (
        <p className="hint">
          This card also stands for {folded} merged {folded === 1 ? 'card' : 'cards'}; each odds row below is one
          card's own, never a mix.
        </p>
      )}
      {items.length > 1 && items.length <= 20 && (
        <button type="button" onClick={() => void store.getState().pinAll(items.map((o) => o.key))}>
          Pin all {items.length}
        </button>
      )}
      {items.length > 1 && items.length <= 50 && (
        <button type="button" onClick={() => void store.getState().pinAllMerging(items.map((o) => o.key))}>
          Pin all {items.length}, merging alike
        </button>
      )}
      <table className="steps-table">
        <thead>
          <tr>
            <th>Outcome</th>
            <th>Chance</th>
            <th>Tries</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {shown.map((outcome) => (
            <OutcomeRow
              key={outcome.key}
              outcome={outcome}
              repeatable={applied.repeatable}
              onChart={nodes.some((n) => n.id === resolveId(merges, outcome.key))}
              onPin={(key) => store.getState().pinForward(key)}
            />
          ))}
        </tbody>
      </table>
      {rest.length > 0 && (
        <p className="hint">
          {rest.length} more outcomes, {percent(rest.reduce((sum, o) => sum + o.p, 0))} together.
        </p>
      )}
    </>
  )
}

/** Where the pinned steps from the selected item end, and how many of each method they use on average. */
function Totals({ chain, nodes }: { chain: Chain; nodes: NodeView[] }) {
  if (chain.kind === 'none') return null
  if (chain.kind === 'endless') {
    return <p className="hint">These pinned steps can go round forever: pin a way out of the loop.</p>
  }
  const label = (id: string) => {
    const node = nodes.find((n) => n.id === id)
    return node ? summary(node.view) : id
  }
  const approx = chain.approx ? '≈ ' : ''
  return (
    <>
      <h3>Following the pinned steps from here</h3>
      <table className="steps-table">
        <thead>
          <tr>
            <th>Ends at</th>
            <th>Chance</th>
          </tr>
        </thead>
        <tbody>
          {chain.ends.map((end) => (
            <tr key={end.node}>
              <td>{label(end.node)}</td>
              <td className="num">
                {approx}
                {percent(end.p)}
              </td>
            </tr>
          ))}
          {chain.off > 1e-9 && (
            <tr>
              <td>Leaves the pinned steps</td>
              <td className="num">
                {approx}
                {percent(chain.off)}
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <p className="hint">On average: {chain.uses.map((u) => `${count(u.n)} × ${u.name}`).join(', ')}.</p>
      {chain.ignored.map((entry) => (
        <p key={entry.node} className="hint">
          At {label(entry.node)}: follows {entry.follows}; the other pinned steps there ({entry.others.join(', ')}) are
          not counted.
        </p>
      ))}
    </>
  )
}

function InputRow({ input, onPin }: { input: InputView; onPin: (method: string, key: string) => void }) {
  return (
    <tr>
      <td>
        <code>{input.method}</code>
      </td>
      <td>{summary(input.view)}</td>
      <td className="num">
        {input.interval ? '≈ ' : ''}
        {percent(input.p)}
        {range(input.interval)}
      </td>
      <td className="num">{input.repeatable ? tries(input.p) : '—'}</td>
      <td>{input.cost}</td>
      <td className="step-note">{input.note}</td>
      <td>
        <button type="button" onClick={() => onPin(input.method, input.key)}>
          Pin
        </button>
      </td>
    </tr>
  )
}

/** What the selected node can be made from, and what it becomes. */
export function Inspector() {
  const { store } = useFlow()
  const nodes = useFlowState((state) => state.nodes)
  const edges = useFlowState((state) => state.edges)
  const rawEdges = useFlowState((state) => state.rawEdges)
  const selected = useFlowState((state) => state.selected)
  const tab = useFlowState((state) => state.tab)
  const methods = useFlowState((state) => state.methods)
  const method = useFlowState((state) => state.method)
  const applied = useFlowState((state) => state.applied)
  const madeBy = useFlowState((state) => state.madeBy)
  const busy = useFlowState((state) => state.busy)
  const [query, setQuery] = useState('')

  const node = nodes.find((n) => n.id === selected) ?? nodes[0] ?? null
  const rootId = nodes[0]?.id ?? null
  const planned = (id: string) => rawEdges.some((edge) => edge.from === id)
  const targets =
    node && node.id !== rootId
      ? nodes.filter((other) => other.id !== node.id && foldShape(node, other, planned) === null)
      : []
  const chain = node && rootId ? pathChance(edges, node.id, rootId) : null
  const totals = useMemo(() => (node ? chainFrom(edges, node.id) : null), [edges, node])
  const list = useMemo(() => {
    const words = query.trim().toLowerCase()
    return words.length < 2
      ? methods.slice(0, 40)
      : methods.filter((m) => m.name.toLowerCase().includes(words) || m.id.toLowerCase().includes(words)).slice(0, 200)
  }, [methods, query])

  if (!node) return <section className="panel inspector" />

  return (
    <section className="panel inspector">
      <div className="panel-header">
        <h2>Selected</h2>
        <span className="hint">{summary(node.view)}</span>
      </div>
      {chain && chain.steps > 1 && (
        <p className="hint">
          Along the {chain.steps} pinned steps between this item and the root: {chain.approx ? '≈ ' : ''}
          {percent(chain.p)}.
        </p>
      )}
      {totals && <Totals chain={totals} nodes={nodes} />}
      <div className="tabs">
        <button type="button" className={tab === 'forward' ? 'tab is-current' : 'tab'} onClick={() => store.getState().setTab('forward')}>
          Apply
        </button>
        <button type="button" className={tab === 'backward' ? 'tab is-current' : 'tab'} onClick={() => store.getState().setTab('backward')}>
          Made by
        </button>
        {node.id !== rootId && (
          <button type="button" className="tab row-actions" onClick={() => store.getState().unpin(node.id)}>
            Remove
          </button>
        )}
        {targets.length > 0 && (
          <select
            className="tab"
            value=""
            onChange={(event) => {
              if (event.target.value) void store.getState().merge(node.id, event.target.value)
            }}
          >
            <option value="">Merge into…</option>
            {targets.map((other) => (
              <option key={other.id} value={other.id}>
                {summary(other.view)}
              </option>
            ))}
          </select>
        )}
      </div>

      {tab === 'forward' ? (
        <>
          <input
            type="search"
            className="picker-search"
            placeholder={`Search ${methods.length} methods`}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <ul className="picker-list methods">
            {list.map((option) => (
              <li key={option.id} className={option.id === method ? 'method-row is-current' : 'method-row'}>
                <button
                  type="button"
                  onClick={() => {
                    try {
                      store.getState().chooseMethod(option.id)
                    } catch (reason) {
                      store.getState().setError(errorText(reason))
                    }
                  }}
                >
                  {option.name}
                </button>
                <span className="picker-level">{option.family}</span>
                <span className="picker-text">{option.cost}</span>
              </li>
            ))}
            {list.length === 0 && (
              <li className="hint">
                {query.trim().length < 2 ? 'No method applies to this item.' : 'No method matches the search.'}
              </li>
            )}
          </ul>
          {busy && <p className="busy">Computing…</p>}
          {applied && <Applied applied={applied} />}
        </>
      ) : madeBy ? (
        <>
          <table className="steps-table">
            <thead>
              <tr>
                <th>Method</th>
                <th>From</th>
                <th>Chance</th>
                <th>Tries</th>
                <th>Cost</th>
                <th />
                <th />
              </tr>
            </thead>
            <tbody>
              {madeBy.inputs.map((input) => (
                <InputRow
                  key={`${input.method}:${input.key}`}
                  input={input}
                  onPin={(by, key) => store.getState().pinBackward(by, key)}
                />
              ))}
            </tbody>
          </table>
          {madeBy.skipped.length > 0 && (
            <details className="skipped">
              <summary>{madeBy.skipped.length} steps that cannot be produced</summary>
              <ul>
                {madeBy.skipped.map((skip) => (
                  <li key={skip.method}>
                    <code>{skip.method}</code> {skip.reason}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </>
      ) : (
        <p className="hint">{busy ? 'Computing…' : 'Nothing to show yet.'}</p>
      )}
    </section>
  )
}
