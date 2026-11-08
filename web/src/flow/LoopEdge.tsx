import { BaseEdge, type EdgeProps } from '@xyflow/react'
import { NODE_WIDTH } from './layout'

/** A step that gives back an item of the same class: a loop out past the card's right side, from its foot to its head. */
export function LoopEdge(props: EdgeProps) {
  const { sourceX: sx, sourceY: sy, targetX: tx, targetY: ty } = props
  const out = NODE_WIDTH / 2 + 40
  const path = `M ${sx} ${sy} C ${sx + out} ${sy + 60}, ${tx + out} ${ty - 60}, ${tx} ${ty}`
  return <BaseEdge path={path} markerEnd={props.markerEnd} style={props.style} />
}
