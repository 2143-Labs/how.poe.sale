import { useMemo } from 'react'
import { catalystNames, formatChaos, formatDivine, formatPercent, summarizeStateKey } from '../solution'
import { useAppStore } from '../store'

interface NodePanelProps {
  selected: string
  onSelect: (key: string) => void
}

export function NodePanel({ selected, onSelect }: NodePanelProps) {
  const solution = useAppStore((state) => state.solution)
  const target = useAppStore((state) => state.target)
  const prices = useAppStore((state) => state.prices)
  const buy = useAppStore((state) => state.buy)
  const sell = useAppStore((state) => state.sell)
  const setBuy = useAppStore((state) => state.setBuy)
  const setSell = useAppStore((state) => state.setSell)
  const addSeed = useAppStore((state) => state.addSeed)
  const expand = useAppStore((state) => state.expand)
  const hide = useAppStore((state) => state.hide)

  const node = solution?.nodes.find((candidate) => candidate.key === selected) ?? null
  const proposals = solution?.proposals[selected] ?? []
  const divine = prices.divine ?? 0
  const catalysts = useMemo(() => (solution ? catalystNames(solution) : new Map<number, string>()), [solution])
  const title =
    selected === 'goal' ? 'Goal' : selected === 'start' ? 'Start' : (node?.view?.title ?? 'Selected state')
  const summary = summarizeStateKey(selected, target, catalysts)

  function priceField(value: number | undefined, onChange: (value: number | null) => void) {
    return (
      <input
        type="number"
        min="0"
        step="any"
        value={value ?? ''}
        placeholder="chaos"
        onChange={(event) => onChange(event.target.value === '' ? null : Number(event.target.value))}
      />
    )
  }

  return (
    <section className="panel node-panel">
      <header className="panel-header">
        <h2 title={selected}>{title}</h2>
        {node && node.kind === 'state' && !node.expanded && (
          <button type="button" onClick={() => expand(node.key)}>
            Expand node
          </button>
        )}
      </header>
      {summary && <p className="state-summary">{summary}</p>}

      {node && (
        <dl className="stats-grid">
          <dt>Value</dt>
          <dd>
            {node.value == null ? 'unreachable' : `${formatChaos(node.value)} chaos (${formatDivine(node.value, divine)})`}
          </dd>
          <dt>Expanded</dt>
          <dd>{node.expanded ? 'yes' : 'no'}</dd>
        </dl>
      )}

      <h3>Buy / Sell</h3>
      <div className="price-row">
        <label>
          Buy price
          {priceField(buy[selected], (value) => setBuy(selected, value))}
        </label>
        <label>
          Sell price
          {priceField(sell[selected], (value) => setSell(selected, value))}
        </label>
      </div>

      <h3>Proposals</h3>
      {solution === null ? (
        <p className="hint">Waiting for the first solve…</p>
      ) : proposals.length === 0 ? (
        <p className="hint">
          {node && !node.expanded
            ? 'This node has not been expanded, so no outcome edges are known yet.'
            : 'No action can produce this node.'}
        </p>
      ) : (
        <table className="proposals">
          <thead>
            <tr>
              <th>Action</th>
              <th>p mods</th>
              <th>p exact</th>
              <th>cost</th>
              <th>q</th>
              <th>status</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {proposals.map((proposal) => (
              <tr key={`${proposal.pre}|${proposal.actionKey}`} className={`status-${proposal.status.toLowerCase()}`}>
                <td>
                  {proposal.label}
                  <div className="pre-summary" title={proposal.pre}>
                    from {summarizeStateKey(proposal.pre, target, catalysts) ?? proposal.pre}
                  </div>
                </td>
                <td>{formatPercent(proposal.pMods)}</td>
                <td>{formatPercent(proposal.pReach)}</td>
                <td>{formatChaos(proposal.cost)}</td>
                <td>{proposal.q == null ? '—' : formatChaos(proposal.q)}</td>
                <td>{proposal.status}</td>
                <td className="row-actions">
                  <button type="button" onClick={() => { addSeed(proposal.pre); onSelect(proposal.pre) }}>
                    Use
                  </button>
                  <button type="button" onClick={() => expand(proposal.pre)}>
                    Expand
                  </button>
                  <button type="button" onClick={() => hide(proposal.pre, proposal.actionKey)}>
                    Hide
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}
