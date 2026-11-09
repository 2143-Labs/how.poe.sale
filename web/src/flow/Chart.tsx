import { useEffect, useMemo, useState } from 'react'
import { Background, Controls, ReactFlow } from '@xyflow/react'
import type { Edge, EdgeTypes, Node, NodeProps, NodeTypes, ReactFlowInstance } from '@xyflow/react'
import type { EdgeView, NodeView } from '../types'
import { Card } from './Card'
import { useFlow, useFlowState } from './context'
import { itemsFrom, percent, range, tries } from './format'
import { NODE_WIDTH, layoutGraph, type LayoutPositions } from './layout'
import { LoopEdge } from './LoopEdge'
import { pathChance } from './path'

type ItemNodeData = {
  node: NodeView
  root: boolean
  /** The steps that reached this item. */
  in: EdgeView[]
  /** The chance of the pinned path from here to the root, when it has one. */
  chain: string | null
  /** Where a node reached by several steps got its items. */
  from: string | null
}

type ItemFlowNode = Node<ItemNodeData, 'item'>

function estimateHeight(data: ItemNodeData): number {
  const view = data.node.view
  const chips =
    view.prefixes.length + view.suffixes.length + (view.anyPrefixes > 0 ? 1 : 0) + (view.anySuffixes > 0 ? 1 : 0)
  return 64 + chips * 20 + data.in.length * 16 + (data.from ? 16 : 0)
}

function ItemCard({ data, selected }: NodeProps<ItemFlowNode>) {
  const badges: string[] = []
  if (data.root) badges.push('root')
  if (data.node.approx) badges.push('≈')
  const footer = data.in.map(
    (edge) =>
      `${edge.name} · ${edge.interval ? '≈ ' : ''}${percent(edge.p)}${range(edge.interval)}${edge.repeatable ? ` · ${tries(edge.p)}` : ''}`,
  )
  if (data.chain) footer.push(data.chain)
  if (data.from) footer.push(data.from)
  return <Card view={data.node.view} badges={badges} footer={footer} selected={selected} root={data.root} />
}

const nodeTypes: NodeTypes = { item: ItemCard }
const edgeTypes: EdgeTypes = { loop: LoopEdge }

/** The chart: the root, and every step pinned from it in either direction. */
export function Chart() {
  const { store } = useFlow()
  const nodes = useFlowState((state) => state.nodes)
  const edges = useFlowState((state) => state.edges)
  const selected = useFlowState((state) => state.selected)
  const [positions, setPositions] = useState<LayoutPositions>({})
  const [instance, setInstance] = useState<ReactFlowInstance<ItemFlowNode, Edge> | null>(null)

  const rootId = nodes[0]?.id ?? null
  const data = useMemo(
    () =>
      nodes.map((node, index): ItemNodeData => {
        const chain = rootId ? pathChance(edges, node.id, rootId) : null
        return {
          node,
          root: index === 0,
          in: edges.filter((edge) => edge.to === node.id),
          chain:
            chain && chain.steps > 1
              ? `path to the root · ${percent(chain.p)}${chain.approx ? ' ≈' : ''} over ${chain.steps} steps`
              : null,
          from: itemsFrom(node, nodes, edges),
        }
      }),
    [nodes, edges, rootId],
  )
  const flowEdges = useMemo<Edge[]>(
    () =>
      edges.map((edge) => ({
        id: `${edge.from}->${edge.to}:${edge.method}`,
        source: edge.from,
        target: edge.to,
        type: edge.from === edge.to ? 'loop' : 'default',
        className: 'edge-chosen',
      })),
    [edges],
  )
  const signature = useMemo(
    () =>
      [
        data.map((d) => `${d.node.id}#${estimateHeight(d)}`).join('|'),
        flowEdges.map((e) => e.id).join('|'),
      ].join('/'),
    [data, flowEdges],
  )

  useEffect(() => {
    let cancelled = false
    layoutGraph(
      data.map((d) => ({ id: d.node.id, height: estimateHeight(d) })),
      // A step back to the same item is drawn beside its card (`LoopEdge`), not laid out.
      flowEdges
        .filter((edge) => edge.source !== edge.target)
        .map((edge) => ({ id: edge.id, source: edge.source, target: edge.target })),
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

  const flowNodes = useMemo<ItemFlowNode[]>(
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

  if (nodes.length === 0) {
    return <div className="graph-canvas graph-empty">Starting the chart…</div>
  }

  return (
    <div className="graph-canvas">
      <ReactFlow<ItemFlowNode, Edge>
        nodes={flowNodes}
        edges={flowEdges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodeClick={(_event, node) => store.getState().select(node.id)}
        onInit={setInstance}
        colorMode="dark"
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
