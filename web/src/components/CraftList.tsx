import type { ContractJson } from '../types'

interface CraftListProps {
  contracts: ContractJson[]
}

function costText(cost: [string, number][]): string {
  if (cost.length === 0) return 'free'
  return cost.map(([key, qty]) => `${qty}× ${key.replace(/-/g, ' ')}`).join(', ')
}

/** The single steps a player can perform on one item, with their declared contract. */
export function CraftList({ contracts }: CraftListProps) {
  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Crafts</h2>
        <span className="hint">single steps you can do on their own</span>
      </header>
      {contracts.length === 0 ? (
        <p className="hint">The craft table is not loaded yet.</p>
      ) : (
        <table className="crafts">
          <thead>
            <tr>
              <th>Craft</th>
              <th>requires</th>
              <th>effect</th>
              <th>cost</th>
            </tr>
          </thead>
          <tbody>
            {contracts.map((contract) => (
              <tr key={contract.id}>
                <td>{contract.name}</td>
                <td className="hint">{contract.requires}</td>
                <td>{contract.effect}</td>
                <td>{costText(contract.cost)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}
