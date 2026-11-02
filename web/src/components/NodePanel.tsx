import { useMemo } from 'react'
import { catalystNames, formatChaos, formatDivine, formatPercent, summarizeStateKey } from '../solution'
import { useAppStore } from '../store'

interface NodePanelProps {
  selected: string
  onSelect: (key: string) => void
}

export function NodePanel({ selected, onSelect }: NodePanelProps) {
  const solution = useAppStore((state) => state.solution)
  const goods = useAppStore((state) => state.goods)
  const processes = useAppStore((state) => state.processes)
  const focus = useAppStore((state) => state.focus)
  const setView = useAppStore((state) => state.setView)
  const tree = solution?.focus ?? null
  const focusProcess = processes.find((process) => process.id === focus) ?? null
  const target = goods.find((good) => good.id === focusProcess?.output)?.target ?? null
  const prices = useAppStore((state) => state.prices)

  const setBuy = useAppStore((state) => state.setBuy)
  const setSell = useAppStore((state) => state.setSell)
  const addSeed = useAppStore((state) => state.addSeed)
  const expand = useAppStore((state) => state.expand)
  const hide = useAppStore((state) => state.hide)

  const buy = Object.fromEntries(focusProcess?.buy ?? []) as Record<string, number>
  const sell = Object.fromEntries(focusProcess?.sell ?? []) as Record<string, number>
  const node = tree?.nodes.find((candidate) => candidate.key === selected) ?? null
  const proposals = tree?.proposals[selected] ?? []
  const divine = prices.divine ?? 0
  const catalysts = useMemo(() => (tree ? catalystNames(tree) : new Map<number, string>()), [tree])
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
        <span className="hint">{focusProcess?.recipe ?? focusProcess?.id ?? 'no process'}</span>
        <button type="button" onClick={() => setView('market')}>
          Market
        </button>
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
      {tree === null ? (
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
