import { useMemo, useState } from 'react'
import type { AppliedView, InputView, MethodOption, OutcomeView, WatchView } from '../types'
import { errorText, useFlow, useFlowState } from './context'
import { percent, range, summary, tries } from './format'
import { pathChance } from './path'

/** The most outcome rows the table shows. */
const ROWS = 100

/** The methods a page offers: on the simple page, its own set plus every essence. */
function offered(scopeId: string, scopeMethods: string[], methods: MethodOption[]): MethodOption[] {
  if (scopeId !== 'simple') return methods
  return methods.filter((m) => scopeMethods.includes(m.id) || m.id.startsWith('essence/'))
}

function OutcomeRow({ outcome, onPin }: { outcome: OutcomeView; onPin: (key: string) => void }) {
  return (
    <tr>
      <td>
        {outcome.view ? summary(outcome.view) : (outcome.reason ?? outcome.kind)}
        {outcome.goal && <span className="badge badge-expanded">goal</span>}
      </td>
      <td className="num">
        {outcome.interval ? '≈ ' : ''}
        {percent(outcome.p)}
        {range(outcome.interval)}
      </td>
      <td className="num">{tries(outcome.p)}</td>
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

/** What the step gives each watched modifier: the outcome rows split by every tracked slot, so these are their sums. */
function Watched({ watched }: { watched: WatchView[] }) {
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
            <td className="num">
              {w.interval ? '≈ ' : ''}
              {percent(w.p)}
              {range(w.interval)}
            </td>
            <td className="num">{w.p > 0 ? tries(w.p) : 'never'}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function Applied({ applied }: { applied: AppliedView }) {
  const { store } = useFlow()
  const rows = applied.outcomes
  const shown = rows.slice(0, ROWS)
  const rest = rows.slice(ROWS)
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
      {applied.watched.length > 0 && <Watched watched={applied.watched} />}
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
            <OutcomeRow key={outcome.key} outcome={outcome} onPin={(key) => store.getState().pinForward(key)} />
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
      <td className="num">{tries(input.p)}</td>
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
  const { scope, store } = useFlow()
  const nodes = useFlowState((state) => state.nodes)
  const edges = useFlowState((state) => state.edges)
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
  const chain = node && rootId ? pathChance(edges, node.id, rootId) : null
  const list = useMemo(() => {
    const words = query.trim().toLowerCase()
    const all = offered(scope.id, scope.methods, methods)
    return words.length < 2
      ? all.slice(0, 40)
      : all.filter((m) => m.name.toLowerCase().includes(words) || m.id.toLowerCase().includes(words)).slice(0, 200)
  }, [methods, query, scope.id, scope.methods])

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
