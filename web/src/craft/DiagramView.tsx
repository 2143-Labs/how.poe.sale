import { useEffect, useMemo, useState } from 'react'
import { Background, Controls, ReactFlow } from '@xyflow/react'
import type { Edge, Node, NodeProps, NodeTypes, ReactFlowInstance } from '@xyflow/react'
import { useCraft } from './context'
import { NODE_WIDTH, layoutGraph, type LayoutPositions } from '../flow/layout'
import { chaos, percent } from './format'
import type { BoxView, DiagramView as Diagram } from './types'

type DiagramData = {
  title: string
  lines: string[]
}

type DiagramNode = Node<DiagramData, 'box'>

function BoxNode({ data }: NodeProps<DiagramNode>) {
  return (
    <div className="solution-node">
      <div className="node-title">{data.title}</div>
      {data.lines.map((line, index) => (
        <div key={`${index}:${line}`} className="node-value">
          {line}
        </div>
      ))}
    </div>
  )
}

const nodeTypes: NodeTypes = { box: BoxNode }

/** What a box says: its rarity and the grades it holds, the move it makes and its cost, or where it exits. */
function boxText(box: BoxView, names: Map<string, string>): DiagramData {
  if (box.kind === 'exit') {
    return { title: `exit ${box.exit ?? ''}`, lines: [`to ${box.to ?? ''}`] }
  }
  const lines: string[] = []
  const holds = (box.prefixes ?? []).length + (box.suffixes ?? []).length
  if (box.kind === 'keeps') {
    lines.push(`prefixes: ${(box.prefixes ?? []).join(', ') || '—'}`)
    lines.push(`suffixes: ${(box.suffixes ?? []).join(', ') || '—'}`)
  } else if (box.prefixes === null && box.suffixes === null) {
    lines.push('any grades')
  } else if (holds > 0) {
    lines.push(`prefixes: ${(box.prefixes ?? []).join(', ')}`)
    lines.push(`suffixes: ${(box.suffixes ?? []).join(', ')}`)
  }
  const move = box.move ? (names.get(box.move) ?? box.move) : null
  if (box.kind === 'keeps') {
    if (move) lines.push(`→ ${move} ${box.cost === null ? '(unpriced)' : chaos(box.cost)}`)
    if (box.options.length > 0) {
      lines.push(`or ${box.options.map((option) => `${option.name} ${chaos(option.cost)}`).join('; ')}`)
    }
  } else if (move) {
    lines.push(`move: ${move}`)
  }
  return { title: `${box.kind} ${box.rarity ?? 'any'}`, lines }
}

/** One step's method as a read-only graph: its moves, what each holds, and where the exits go. */
export function DiagramView({ diagram }: { diagram: Diagram }) {
  const catalog = useCraft((state) => state.catalog)
  const [positions, setPositions] = useState<LayoutPositions>({})
  const [instance, setInstance] = useState<ReactFlowInstance<DiagramNode, Edge> | null>(null)
  const names = useMemo(() => new Map(catalog.currencies.map((currency) => [currency.id, currency.name])), [catalog])

  const boxes = useMemo(
    () =>
      diagram.boxes.map((box) => {
        const text = boxText(box, names)
        return { id: `box-${box.id}`, data: text, height: 56 + text.lines.length * 18 }
      }),
    [diagram, names],
  )

  const edges = useMemo<Edge[]>(
    () =>
      diagram.edges.map((edge) => ({
        id: `e${edge.from}-${edge.to}`,
        source: `box-${edge.from}`,
        target: `box-${edge.to}`,
        label: percent(edge.share),
        labelBgStyle: { fill: '#1b1d27' },
        labelStyle: { fill: '#e6e7ee', fontSize: 11 },
      })),
    [diagram],
  )

  const signature = useMemo(
    () => JSON.stringify([boxes.map((box) => `${box.id}#${box.height}`), edges.map((edge) => edge.id)]),
    [boxes, edges],
  )

  useEffect(() => {
    let cancelled = false
    layoutGraph(
      boxes.map((box) => ({ id: box.id, height: box.height })),
      edges.map((edge) => ({ id: edge.id, source: edge.source, target: edge.target })),
    )
      .then((laid) => {
        if (!cancelled) setPositions(laid)
      })
      .catch(() => {
        // Layout is cosmetic: the boxes keep their last positions.
      })
    return () => {
      cancelled = true
    }
    // Re-layout only when the box or edge set changes.
  }, [signature])

  useEffect(() => {
    if (!instance) return
    const handle = requestAnimationFrame(() => instance.fitView({ padding: 0.2, maxZoom: 1 }))
    return () => cancelAnimationFrame(handle)
  }, [instance, positions])

  const nodes = useMemo<DiagramNode[]>(
    () =>
      boxes.map((box, index) => ({
        id: box.id,
        type: 'box',
        data: box.data,
        position: positions[box.id] ?? { x: index * (NODE_WIDTH + 40), y: 0 },
      })),
    [boxes, positions],
  )

  return (
    <div className="diagram-canvas">
      <ReactFlow<DiagramNode, Edge>
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onInit={setInstance}
        colorMode="dark"
        minZoom={0.2}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        proOptions={{ hideAttribution: true }}
      >
        <Background />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  )
}
