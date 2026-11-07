import { useEffect, useMemo, useState } from 'react'
import { Background, Controls, Handle, Position, ReactFlow } from '@xyflow/react'
import type { Edge, Node, NodeProps, NodeTypes, ReactFlowInstance } from '@xyflow/react'
import type { ChartEdge, ChartNode } from '../types'
import { useFlow, useFlowState } from './context'
import { nodeTitle, percent } from './format'
import { NODE_WIDTH, layoutGraph, type LayoutPositions } from './layout'

type ItemNodeData = {
  node: ChartNode
  root: boolean
  /** The steps this node was pinned as the input of. */
  out: ChartEdge[]
}

type ItemFlowNode = Node<ItemNodeData, 'item'>

function estimateHeight(data: ItemNodeData): number {
  const view = data.node.view
  const chips =
    view.prefixes.length + view.suffixes.length + (view.anyPrefixes > 0 ? 1 : 0) + (view.anySuffixes > 0 ? 1 : 0)
  return 64 + chips * 20 + data.out.length * 16
}

function ItemCard({ data, selected }: NodeProps<ItemFlowNode>) {
  const view = data.node.view
  const className = ['solution-node', selected ? 'is-selected' : '', data.root ? 'goal' : ''].filter(Boolean).join(' ')
  return (
    <div className={className}>
      <Handle type="target" position={Position.Top} />
      <div className="node-title">{nodeTitle(view)}</div>
      <div className="node-meta">
        {data.root && <span className="badge badge-expanded">target</span>}
        <span className="badge">{view.rarity}</span>
        {view.influences.map((influence) => (
          <span key={influence} className="badge badge-lock">
            {influence}
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
      {data.out.map((edge) => (
        <div key={`${edge.to}:${edge.method}`} className="node-value">
          {edge.name} · {percent(edge.p)}
        </div>
      ))}
      <Handle type="source" position={Position.Bottom} />
    </div>
  )
}

const nodeTypes: NodeTypes = { item: ItemCard }

/**
 * The chart: the target at the bottom and every pinned input above the item it produced, so it
 * reads top-down in crafting order.
 */
export function Chart() {
  const { store } = useFlow()
  const chart = useFlowState((state) => state.chart)
  const edges = useFlowState((state) => state.edges)
  const selected = useFlowState((state) => state.selected)
  const [positions, setPositions] = useState<LayoutPositions>({})
  const [instance, setInstance] = useState<ReactFlowInstance<ItemFlowNode, Edge> | null>(null)

  const data = useMemo(
    () =>
      chart.map(
        (node, index): ItemNodeData => ({
          node,
          root: index === 0,
          out: edges.filter((edge) => edge.from === node.id),
        }),
      ),
    [chart, edges],
  )
  const flowEdges = useMemo<Edge[]>(
    () =>
      edges.map((edge) => ({
        id: `${edge.from}->${edge.to}:${edge.method}`,
        source: edge.from,
        target: edge.to,
        className: 'edge-chosen',
      })),
    [edges],
  )
  const signature = useMemo(
    () => [data.map((d) => `${d.node.id}#${estimateHeight(d)}`).join('|'), flowEdges.map((e) => e.id).join('|')].join('/'),
    [data, flowEdges],
  )

  useEffect(() => {
    let cancelled = false
    layoutGraph(
      data.map((d) => ({ id: d.node.id, height: estimateHeight(d) })),
      flowEdges.map((edge) => ({ id: edge.id, source: edge.source, target: edge.target })),
    )
      .then((laid) => {
        if (!cancelled) setPositions(laid)
      })
      .catch(() => {
        // layout is cosmetic: the nodes keep their last positions
      })
    return () => {
      cancelled = true
    }
    // Re-layout only when the node or edge set changes.
  }, [signature])

  useEffect(() => {
    if (!instance) return
    const handle = requestAnimationFrame(() => instance.fitView({ padding: 0.2, maxZoom: 1 }))
    return () => cancelAnimationFrame(handle)
  }, [instance, positions])

  const nodes = useMemo<ItemFlowNode[]>(
    () =>
      data.map((d, index) => ({
        id: d.node.id,
        type: 'item',
        data: d,
        selected: d.node.id === selected,
        position: positions[d.node.id] ?? { x: index * (NODE_WIDTH + 40), y: 0 },
      })),
    [data, positions, selected],
  )

  if (chart.length === 0) {
    return <div className="graph-canvas graph-empty">Computing the target…</div>
  }

  return (
    <div className="graph-canvas">
      <ReactFlow<ItemFlowNode, Edge>
        nodes={nodes}
        edges={flowEdges}
        nodeTypes={nodeTypes}
        onNodeClick={(_event, node) => store.getState().select(node.id)}
        onInit={setInstance}
        nodesConnectable={false}
        nodesDraggable={false}
        minZoom={0.1}
        proOptions={{ hideAttribution: true }}
      >
        <Background />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  )
}
