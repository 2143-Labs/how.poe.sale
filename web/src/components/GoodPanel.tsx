import { useEffect, useMemo, useRef, useState } from 'react'
import { authoredProposal } from '../market'
import { buildProblem, draftToTarget, targetToDraft } from '../problem'
import { formatChaos } from '../solution'
import { useAppStore } from '../store'
import { RecipeCard } from './RecipeCard'
import { RequirementsEditor, type ModTierLookup } from './TargetEditor'
import type {
  AnyProblem,
  ContractJson,
  GoodJson,
  ProcessProposalJson,
  RecipeDef,
  TargetDraft,
} from '../types'

interface GoodPanelProps {
  proposeProcesses: (problem: AnyProblem, goodId: string) => Promise<ProcessProposalJson[]>
  contracts: ContractJson[]
  modTiers?: ModTierLookup
}

const INPUTS: NonNullable<RecipeDef['input']>[] = ['normal-base', 'same-base']

/** The selected good: target editor, prices, ranked processes and authored recipes. */
export function GoodPanel({ proposeProcesses, contracts, modTiers }: GoodPanelProps) {
  const goods = useAppStore((state) => state.goods)
  const processes = useAppStore((state) => state.processes)
  const root = useAppStore((state) => state.root)
  const focus = useAppStore((state) => state.focus)
  const prices = useAppStore((state) => state.prices)
  const donors = useAppStore((state) => state.donors)
  const authoredRecipes = useAppStore((state) => state.authoredRecipes)
  const selectedGood = useAppStore((state) => state.selectedGood)
  const solution = useAppStore((state) => state.solution)
  const proposals = useAppStore((state) => state.proposals)
  const setProposals = useAppStore((state) => state.setProposals)
  const setGoodPrice = useAppStore((state) => state.setGoodPrice)
  const updateGoodTarget = useAppStore((state) => state.updateGoodTarget)
  const useProcessProposal = useAppStore((state) => state.useProcessProposal)
  const removeProcess = useAppStore((state) => state.removeProcess)
  const addBrickGood = useAppStore((state) => state.addBrickGood)
  const addAuthoredRecipe = useAppStore((state) => state.addAuthoredRecipe)

  const good: GoodJson | null = goods.find((candidate) => candidate.id === selectedGood) ?? null
  const value = solution?.market.goods.find((candidate) => candidate.id === selectedGood) ?? null
  const [draft, setDraft] = useState<TargetDraft | null>(() => (good ? targetToDraft(good.target) : null))
  const [recipeName, setRecipeName] = useState('')
  const [recipeAllowed, setRecipeAllowed] = useState<string[]>([])
  const [recipeInput, setRecipeInput] = useState<NonNullable<RecipeDef['input']> | ''>('')

  // A different good is a different target; drop the previous good's edits.
  useEffect(() => {
    setDraft(good ? targetToDraft(good.target) : null)
  }, [good?.id, good?.target])

  const problem = useMemo(
    () => buildProblem({ goods, processes, root, focus, prices, donors, authoredRecipes }),
    [goods, processes, root, focus, prices, donors, authoredRecipes],
  )

  // Proposals are evaluated against current values, so any market or price edit refreshes them.
  const fetchKey = problem && selectedGood ? `${selectedGood}\u0000${JSON.stringify(problem)}` : null
  const fetchedFor = useRef<string | null>(null)
  const cached = selectedGood ? proposals[selectedGood] : undefined
  useEffect(() => {
    if (!problem || !selectedGood || !fetchKey) return
    if (cached && fetchedFor.current === fetchKey) return
    let cancelled = false
    proposeProcesses(problem, selectedGood)
      .then((list) => {
        if (cancelled) return
        fetchedFor.current = fetchKey
        setProposals(selectedGood, list)
      })
      .catch(() => {
        // the engine reports why a proposal list is empty through `reason`
      })
    return () => {
      cancelled = true
    }
  }, [problem, selectedGood, fetchKey, cached, proposeProcesses, setProposals])

  const authored = useMemo(() => {
    if (!good) return []
    return authoredRecipes
      .filter((recipe) => recipe.kind === 'craft' && recipe.rarity === good.target.rarity)
      .map((recipe) => authoredProposal(recipe, good))
      .filter((proposal): proposal is ProcessProposalJson => proposal !== null)
  }, [authoredRecipes, good])

  const ranked = useMemo(() => {
    const engine = cached ?? solution?.proposals[selectedGood ?? ''] ?? []
    return [...engine, ...authored].sort((a, b) => {
      if (a.cost == null) return b.cost == null ? 0 : 1
      if (b.cost == null) return -1
      return a.cost - b.cost
    })
  }, [authored, cached, selectedGood, solution])

  if (!good) {
    return (
      <section className="panel">
        <header className="panel-header">
          <h2>Good</h2>
        </header>
        <p className="hint">Select a good on the canvas to edit it.</p>
      </section>
    )
  }

  const usedProcesses = processes.filter(
    (process) =>
      ('craft' in process.kind
        ? process.kind.craft.from === good.id
        : process.kind.recombine.a === good.id || process.kind.recombine.b === good.id) ||
      process.output === good.id,
  )

  function saveAuthored() {
    if (!good || recipeName.trim() === '' || recipeAllowed.length === 0) return
    addAuthoredRecipe({
      id: `authored-${recipeName.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${authoredRecipes.length + 1}`,
      name: recipeName.trim(),
      kind: 'craft',
      rarity: good.target.rarity,
      allowed: [...recipeAllowed],
      ...(recipeInput ? { input: recipeInput } : {}),
    })
    setRecipeName('')
    setRecipeAllowed([])
    setRecipeInput('')
  }

  return (
    <section className="panel node-panel">
      <header className="panel-header">
        <h2>{good.name}</h2>
        {good.id === root && <span className="badge badge-expanded">root</span>}
        {value?.needed && <span className="badge badge-unreachable">needed</span>}
        {value?.value != null && <span className="hint">{formatChaos(value.value)} chaos</span>}
      </header>

      <div className="price-row">
        <label>
          Buy price
          <input
            type="number"
            min="0"
            step="any"
            value={good.buy ?? ''}
            placeholder="chaos"
            onChange={(event) =>
              setGoodPrice(good.id, { buy: event.target.value === '' ? null : Number(event.target.value) })
            }
          />
        </label>
        <label>
          Sell price
          <input
            type="number"
            min="0"
            step="any"
            value={good.sell ?? ''}
            placeholder="chaos"
            onChange={(event) =>
              setGoodPrice(good.id, { sell: event.target.value === '' ? null : Number(event.target.value) })
            }
          />
        </label>
        <button
          type="button"
          disabled={value?.value == null}
          onClick={() => value?.value != null && setGoodPrice(good.id, { sell: value.value })}
        >
          sell at value
        </button>
      </div>

      {draft && (
        <>
          <RequirementsEditor draft={draft} onChange={setDraft} modTiers={modTiers} />
          <div className="row-actions">
            <button
              type="button"
              onClick={() => {
                const target = draftToTarget(draft)
                if (target) updateGoodTarget(good.id, target)
              }}
            >
              Update target
            </button>
            <button type="button" onClick={() => setDraft(targetToDraft(good.target))}>
              Revert edits
            </button>
          </div>
        </>
      )}

      <h3>Processes</h3>
      {usedProcesses.length > 0 && (
        <ul className="process-list">
          {usedProcesses.map((process) => (
            <li key={process.id}>
              <span>
                {process.recipe ?? process.id} → {process.output}
              </span>
              <button type="button" onClick={() => removeProcess(process.id)}>
                remove
              </button>
            </li>
          ))}
        </ul>
      )}

      <h3>Proposals</h3>
      {ranked.length === 0 ? (
        <p className="hint">{solution ? 'No recipe applies to this good.' : 'Waiting for the first solve…'}</p>
      ) : (
        <div className="proposal-cards">
          {ranked.map((proposal) => (
            <RecipeCard
              key={proposal.process.id}
              proposal={proposal}
              goods={goods}
              onUse={(chosen) => useProcessProposal(chosen)}
              onMakeBrick={(fromGoodId, name, target) => addBrickGood(fromGoodId, name, target)}
            />
          ))}
        </div>
      )}

      <h3>Author a recipe</h3>
      <div className="authored">
        <label className="inline-field">
          name
          <input value={recipeName} onChange={(event) => setRecipeName(event.target.value)} />
        </label>
        <label className="inline-field">
          input
          <select value={recipeInput} onChange={(event) => setRecipeInput(event.target.value as typeof recipeInput)}>
            <option value="">the output item itself</option>
            {INPUTS.map((input) => (
              <option key={input} value={input}>
                {input}
              </option>
            ))}
          </select>
        </label>
        <div className="craft-picker">
          {contracts.map((contract) => (
            <label key={contract.id} className="inline-field">
              <input
                type="checkbox"
                checked={recipeAllowed.includes(contract.id)}
                onChange={(event) =>
                  setRecipeAllowed((current) =>
                    event.target.checked
                      ? [...current, contract.id]
                      : current.filter((id) => id !== contract.id),
                  )
                }
              />
              {contract.name}
            </label>
          ))}
        </div>
        <div className="row-actions">
          <button
            type="button"
            disabled={recipeName.trim() === '' || recipeAllowed.length === 0}
            onClick={saveAuthored}
          >
            Save recipe
          </button>
        </div>
      </div>
    </section>
  )
}
