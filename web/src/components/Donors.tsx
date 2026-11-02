import { useEffect, useState } from 'react'
import { rootTarget, useAppStore } from '../store'
import { INFLUENCES } from '../types'
import type { Donor, InfluenceModEntry } from '../types'

interface DonorsProps {
  influenceMods: (base: string, influence: string) => Promise<InfluenceModEntry[]>
}

function newDonor(base: string, ilvl: number): Donor {
  return { label: '', base, ilvl, influence: 'Warlord', mod: '', priceChaos: 0 }
}

export function Donors({ influenceMods }: DonorsProps) {
  const donors = useAppStore((state) => state.donors)
  const setDonors = useAppStore((state) => state.setDonors)
  const target = useAppStore(rootTarget)
  const [mods, setMods] = useState<Record<string, InfluenceModEntry[]>>({})

  useEffect(() => {
    let cancelled = false
    for (const donor of donors) {
      if (!donor.base || !donor.influence) continue
      const key = `${donor.base}|${donor.influence}`
      if (key in mods) continue
      influenceMods(donor.base, donor.influence)
        .then((list) => {
          if (!cancelled) setMods((previous) => ({ ...previous, [key]: list }))
        })
        .catch(() => {
          if (!cancelled) setMods((previous) => ({ ...previous, [key]: [] }))
        })
    }
    return () => {
      cancelled = true
    }
  }, [donors, mods, influenceMods])

  function patch(index: number, change: Partial<Donor>) {
    setDonors(donors.map((donor, i) => (i === index ? { ...donor, ...change } : donor)))
  }

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Donors</h2>
        <span className="hint">items bought for Awakener&rsquo;s Orb</span>
        <button
          type="button"
          onClick={() => setDonors([...donors, newDonor(target?.base ?? '', target?.ilvlMin ?? 84)])}
        >
          add donor
        </button>
      </header>
      {donors.length === 0 && <p className="hint">No donors yet. Combine needs a declared donation item.</p>}
      {donors.length > 0 && (
        <table className="donors">
          <thead>
            <tr>
              <th>Label</th>
              <th>Base</th>
              <th>ilvl</th>
              <th>Influence</th>
              <th>Mod</th>
              <th>chaos</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {donors.map((donor, index) => {
              const options = mods[`${donor.base}|${donor.influence}`] ?? []
              const listId = `donor-mods-${index}`
              return (
                <tr key={index}>
                  <td>
                    <input value={donor.label} onChange={(event) => patch(index, { label: event.target.value })} />
                  </td>
                  <td>
                    <input value={donor.base} onChange={(event) => patch(index, { base: event.target.value })} />
                  </td>
                  <td>
                    <input
                      type="number"
                      min="1"
                      max="100"
                      value={donor.ilvl}
                      onChange={(event) => patch(index, { ilvl: Number(event.target.value) })}
                    />
                  </td>
                  <td>
                    <select value={donor.influence} onChange={(event) => patch(index, { influence: event.target.value })}>
                      {INFLUENCES.map((influence) => (
                        <option key={influence} value={influence}>
                          {influence}
                        </option>
                      ))}
                    </select>
                  </td>
                  <td>
                    <input
                      list={listId}
                      value={donor.mod}
                      placeholder="mod id"
                      onChange={(event) => patch(index, { mod: event.target.value })}
                    />
                    <datalist id={listId}>
                      {options.map((option) => (
                        <option key={option.id} value={option.id}>
                          {option.text}
                        </option>
                      ))}
                    </datalist>
                  </td>
                  <td>
                    <input
                      type="number"
                      min="0"
                      step="any"
                      value={donor.priceChaos}
                      onChange={(event) => patch(index, { priceChaos: Number(event.target.value) })}
                    />
                  </td>
                  <td>
                    <button
                      type="button"
                      onClick={() => setDonors(donors.filter((_, i) => i !== index))}
                      aria-label="remove donor"
                    >
                      ×
                    </button>
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      )}
    </section>
  )
}
