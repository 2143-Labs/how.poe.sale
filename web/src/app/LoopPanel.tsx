import { chance, chaos, count } from './format'
import { useAppState } from './context'

/** The loop's depth table and, once priced, the chaos it costs. */
export function LoopPanel() {
  const start = useAppState((state) => state.start)
  const loop = useAppState((state) => state.loop)
  const busy = useAppState((state) => state.busy)
  if (!start) return null

  return (
    <section className="panel">
      <div className="panel-header">
        <h2>Craft loop</h2>
        {busy && <span className="busy">Computing…</span>}
      </div>

      {loop === null ? (
        <p className="hint">Waiting for the engine…</p>
      ) : (
        <>
          <p className="item-text">{loop.start.text}</p>
          <p className="hint">stage: {loop.start.stage}</p>

          {loop.priced && (
            <>
              <p className="priced-total">
                {chaos(loop.priced.expectedChaos)} ± {chaos(loop.priced.sdChaos)} chaos
              </p>
              <p className="hint">
                {loop.priced.closure} classes closed
                {loop.priced.truncated > 0 ? `, cut at ${loop.priced.truncated}: the cost is an upper bound` : ''}
              </p>
              <ul className="warnings">
                {loop.priced.spend.map((spend) => (
                  <li key={spend.key}>
                    {spend.name}: {count(spend.count)}
                  </li>
                ))}
              </ul>
            </>
          )}

          <div className="loop-table">
            <table>
              <thead>
                <tr>
                  <th>depth</th>
                  {start.stages.map((stage) => (
                    <th key={stage} className="num">
                      {stage}
                    </th>
                  ))}
                  <th className="num">live</th>
                </tr>
              </thead>
              <tbody>
                {loop.depths.map((depth) => (
                  <tr key={depth.depth}>
                    <td>{depth.depth}</td>
                    {start.stages.map((stage) => {
                      const at = depth.stages.find((entry) => entry.stage === stage)
                      return (
                        <td key={stage} className="num">
                          {at ? chance(at.mass) : ''}
                        </td>
                      )
                    })}
                    <td className="num">{chance(depth.live)}</td>
                  </tr>
                ))}
                <tr>
                  <td>total</td>
                  {start.stages.map((stage) => {
                    const at = loop.totals.find((entry) => entry.stage === stage)
                    return (
                      <td key={stage} className="num">
                        {at ? chance(at.mass) : ''}
                      </td>
                    )
                  })}
                  <td className="num">{chance(loop.live)}</td>
                </tr>
              </tbody>
            </table>
          </div>

          {loop.stopped && <p className="hint">stopped: {loop.stopped}</p>}
          {loop.warnings.length > 0 && (
            <ul className="warnings">
              {loop.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  )
}
