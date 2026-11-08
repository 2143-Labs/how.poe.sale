import { Handle, Position } from '@xyflow/react'
import type { ClassView } from '../types'
import { nodeTitle } from './format'

interface CardProps {
  view: ClassView
  /** Short labels under the title: `root`, `≈`. */
  badges: string[]
  /** One line per step that reached this item. */
  footer: string[]
  selected: boolean
  root: boolean
}

/** One item on the chart: its title, badges, its two sides of modifiers, and its steps. */
export function Card({ view, badges, footer, selected, root }: CardProps) {
  const className = ['solution-node', selected ? 'is-selected' : '', root ? 'goal' : ''].filter(Boolean).join(' ')
  return (
    <div className={className}>
      <Handle type="target" position={Position.Top} />
      <div className="node-title">{nodeTitle(view)}</div>
      <div className="node-meta">
        <span className="badge">{view.rarity}</span>
        {view.corrupted && <span className="badge badge-expanded">corrupted</span>}
        {view.influences.map((influence) => (
          <span key={influence} className="badge badge-lock">
            {influence}
          </span>
        ))}
        {badges.map((badge) => (
          <span key={badge} className="badge badge-expanded">
            {badge}
          </span>
        ))}
      </div>
      <div className="node-sides">
        <ul className="chips">
          <li className="chips-label">Prefixes</li>
          {view.prefixes.map((text, index) => (
            <li key={`p${index}`} className="chip chip-wanted">
              {text}
            </li>
          ))}
          {view.anyPrefixes > 0 && <li className="chip chip-other">any prefix ×{view.anyPrefixes}</li>}
        </ul>
        <ul className="chips">
          <li className="chips-label">Suffixes</li>
          {view.suffixes.map((text, index) => (
            <li key={`s${index}`} className="chip chip-wanted">
              {text}
            </li>
          ))}
          {view.anySuffixes > 0 && <li className="chip chip-other">any suffix ×{view.anySuffixes}</li>}
        </ul>
      </div>
      {footer.map((line, index) => (
        <div key={`${index}:${line}`} className="node-value">
          {line}
        </div>
      ))}
      <Handle type="source" position={Position.Bottom} />
    </div>
  )
}
