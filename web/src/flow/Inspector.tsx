import { nodeId, normalizeSpec, type PreimageView } from '../types'
import { useFlow, useFlowState } from './context'
import { nodeTitle, percent, summary } from './format'

/** The selected node's steps: what could have produced it, each ready to pin onto the chart. */
export function Inspector() {
  const { store } = useFlow()
  const chart = useFlowState((state) => state.chart)
  const edges = useFlowState((state) => state.edges)
  const selected = useFlowState((state) => state.selected)
  const preimages = useFlowState((state) => state.preimages)
  const preimagesFor = useFlowState((state) => state.preimagesFor)

  const node = chart.find((candidate) => candidate.id === selected)
  if (!node) {
    return <section className="panel hint">Select an item on the chart.</section>
  }
  const isRoot = chart[0]?.id === node.id
  const ready = preimages && preimagesFor === node.id ? preimages : null

  const pinned = (step: PreimageView) => {
    const spec = normalizeSpec(step.inputSpec)
    const from = spec ? nodeId(spec) : ''
    return edges.some((edge) => edge.from === from && edge.to === node.id && edge.method === step.method)
  }

  return (
    <section className="panel inspector">
      <div className="panel-header">
        <h2>What could have made {nodeTitle(node.view)}</h2>
        <span className="hint">{summary(node.view)}</span>
        <span className="spacer" />
        {!isRoot && (
          <button type="button" onClick={() => store.getState().unpin(node.id)}>
            Remove
          </button>
        )}
      </div>
      {!ready ? (
        <p className="hint">computing…</p>
      ) : (
        <>
          {ready.steps.length === 0 ? (
            <p className="hint">Nothing this page models could have produced this item.</p>
          ) : (
            <>
              <p className="hint">
                Each chance is the chance the step turns its own input into this item. Rows with different
                inputs are different events, so their chances do not add up.
              </p>
              <table className="steps-table">
                <thead>
                  <tr>
                    <th>Step</th>
                    <th className="num">Chance</th>
                    <th>Cost</th>
                    <th>Input</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {ready.steps.map((step, index) => (
                    <tr key={`${step.method}:${index}`}>
                      <td>
                        {step.name}
                        <br />
                        <code>{step.method}</code>
                      </td>
                      <td className="num">{percent(step.p)}</td>
                      <td>{step.cost}</td>
                      <td>
                        {summary(step.input)}
                        {step.note && <div className="step-note">{step.note}</div>}
                      </td>
                      <td>
                        <button
                          type="button"
                          disabled={pinned(step)}
                          onClick={() => store.getState().pin(node.id, step)}
                        >
                          {pinned(step) ? 'Pinned' : 'Pin'}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
          {ready.skipped.length > 0 && (
            <details className="skipped">
              <summary>{ready.skipped.length} steps that cannot be produced</summary>
              <ul>
                {ready.skipped.map((s) => (
                  <li key={s.method}>
                    <strong>{s.name}</strong>: {s.reason}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </>
      )}
    </section>
  )
}
