import { useState } from 'react'
import { useAppStore } from '../store'
import type { PatternSpec } from '../types'

/** The goal: a built-in pattern, a pattern made from a pasted item, or the pattern JSON itself. */
export function TargetPanel({
  patterns,
  target,
}: {
  patterns: PatternSpec[]
  target: (text: string) => Promise<PatternSpec>
}) {
  const patternText = useAppStore((state) => state.patternText)
  const targetText = useAppStore((state) => state.targetText)
  const setPatternText = useAppStore((state) => state.setPatternText)
  const setTargetText = useAppStore((state) => state.setTargetText)
  const [targetError, setTargetError] = useState<string | null>(null)

  async function useAsTarget() {
    try {
      const pattern = await target(targetText)
      setPatternText(JSON.stringify(pattern, null, 2))
      setTargetError(null)
    } catch (error) {
      setTargetError(error instanceof Error ? error.message : String(error))
    }
  }

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Target</h2>
      </header>
      <div className="row-actions">
        <span className="hint">Built-in:</span>
        {patterns.map((pattern) => (
          <button key={pattern.name} type="button" onClick={() => setPatternText(JSON.stringify(pattern, null, 2))}>
            {pattern.name}
          </button>
        ))}
      </div>
      <details>
        <summary>From an item you want</summary>
        <textarea
          rows={10}
          value={targetText}
          placeholder="Paste the item you want to end with"
          onChange={(event) => setTargetText(event.target.value)}
        />
        <div className="row-actions">
          <button type="button" onClick={useAsTarget} disabled={!targetText.trim()}>
            Use as target
          </button>
          {targetError && <span className="error-inline">{targetError}</span>}
        </div>
      </details>
      <h3>Pattern (JSON)</h3>
      <textarea rows={16} value={patternText} onChange={(event) => setPatternText(event.target.value)} />
      <p className="hint">
        Each class lists the modifiers it accepts and a grade (desired, good, middling, neutral, bad); each slot names
        the classes it accepts.
      </p>
    </section>
  )
}
