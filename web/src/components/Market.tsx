import { useCallback, useEffect, useMemo, useState } from 'react'
import { Background, Controls, Handle, Position, ReactFlow } from '@xyflow/react'
import type { EdgeMouseHandler, NodeProps, NodeTypes, ReactFlowInstance } from '@xyflow/react'
import { layoutGraph } from '../layout'
import { marketToFlow, type MarketFlowEdge, type MarketFlowNode } from '../market'
import { buildProblem } from '../problem'
import { formatChaos } from '../solution'
import { useAppStore } from '../store'
import { RecipeCard } from './RecipeCard'
import type { ProcessValueJson } from '../types'

function goodNodeHeight(node: MarketFlowNode): number {
  const chips = node.data.good.view.prefixes.length + node.data.good.view.suffixes.length
  return 96 + chips * 16
}

function GoodNodeCard({ data, selected }: NodeProps<MarketFlowNode>) {
  const { good, buy, sell } = data
  const className = ['good-node', selected ? 'is-selected' : '', good.needed ? 'is-needed' : '']
    .filter(Boolean)
    .join(' ')
  return (
    <div className={className}>
      <Handle type="target" position={Position.Top} />
      <div className="node-title">{good.name}</div>
      <div className="node-meta">
        {good.via && <span className="badge">{good.via === 'buy' ? 'buy' : good.via}</span>}
        {good.needed && <span className="badge badge-unreachable">needed</span>}
      </div>
      <div className="node-value">
        {good.value == null ? 'no value' : `${formatChaos(good.value)} chaos`}
        {good.sd != null && <span className="hint"> · sd {formatChaos(good.sd)}</span>}
      </div>
      <div className="good-prices">
        {buy != null && <span>buy {formatChaos(buy)}</span>}
        {sell != null && <span>sell {formatChaos(sell)}</span>}
      </div>
      <Handle type="source" position={Position.Bottom} />
    </div>
  )
}

const nodeTypes: NodeTypes = { goodNode: GoodNodeCard }

export function Market() {
  const solution = useAppStore((state) => state.solution)
  const goods = useAppStore((state) => state.goods)
  const processes = useAppStore((state) => state.processes)
  const root = useAppStore((state) => state.root)
  const focus = useAppStore((state) => state.focus)
  const prices = useAppStore((state) => state.prices)
  const donors = useAppStore((state) => state.donors)
  const authoredRecipes = useAppStore((state) => state.authoredRecipes)
  const positions = useAppStore((state) => state.positions)
  const setPosition = useAppStore((state) => state.setPosition)
  const setFocus = useAppStore((state) => state.setFocus)
  const addBrickGood = useAppStore((state) => state.addBrickGood)
  const selectGood = useAppStore((state) => state.selectGood)
  const selectedGood = useAppStore((state) => state.selectedGood)

  const [layoutPositions, setLayoutPositions] = useState<Record<string, { x: number; y: number }>>({})
  const [instance, setInstance] = useState<ReactFlowInstance<MarketFlowNode, MarketFlowEdge> | null>(null)
  const [openProcess, setOpenProcess] = useState<ProcessValueJson | null>(null)

  const problem = useMemo(
    () => buildProblem({ goods, processes, root, focus, prices, donors, authoredRecipes }),
    [goods, processes, root, focus, prices, donors, authoredRecipes],
  )

  const flow = useMemo(
    () => (solution && problem ? marketToFlow(solution, problem) : { nodes: [], edges: [] }),
    [solution, problem],
  )

  const signature = useMemo(
    () =>
      `${flow.nodes.map((node) => node.id).join('|')}#${flow.edges
        .map((edge) => `${edge.source}>${edge.target}`)
        .join('|')}`,
    [flow],
  )

  useEffect(() => {
    let cancelled = false
    layoutGraph(
      flow.nodes.map((node) => ({ id: node.id, height: goodNodeHeight(node) })),
      flow.edges.map((edge) => ({ id: edge.id, source: edge.source, target: edge.target })),
    )
      .then((laid) => {
        if (!cancelled) setLayoutPositions(laid)
      })
      .catch(() => {
        // layout is cosmetic: fall back to the persisted / default positions
      })
    return () => {
      cancelled = true
    }
  }, [signature])

  const onEdgeClick = useCallback<EdgeMouseHandler<MarketFlowEdge>>(
    (_event, edge) => {
      const processId = edge.data?.processId
      if (!processId) return
      const value = solution?.market.processes.find((process) => process.id === processId) ?? null
      setOpenProcess(value)
    },
    [setFocus, solution],
  )

  if (flow.nodes.length === 0) {
    return (
      <div className="graph-empty">
        Nothing to show yet — paste a target item and use a proposal to add a process to the market.
      </div>
    )
  }

  return (
    <div className="market-view">
      <div className="graph-canvas">
        <ReactFlow
          key={signature}
          nodes={flow.nodes.map((node) => ({
            ...node,
            selected: node.id === selectedGood,
            position:
              positions[`market:${node.id}`] ??
              layoutPositions[node.id] ?? { x: 0, y: 0 },
          }))}
          edges={flow.edges}
          nodeTypes={nodeTypes}
          onEdgeClick={onEdgeClick}
          onNodeClick={(_event, node) => selectGood(node.id)}
          onNodeDragStop={(_event, node) => setPosition(`market:${node.id}`, { x: node.position.x, y: node.position.y })}
          onInit={setInstance}
          nodesConnectable={false}
          minZoom={0.05}
          proOptions={{ hideAttribution: true }}
        >
          <Background />
          <Controls showInteractive={false} />
        </ReactFlow>
      </div>
      {instance && flow.nodes.length > 0 && (
        <button type="button" className="fit" onClick={() => instance.fitView({ padding: 0.2, maxZoom: 1 })}>
          fit
        </button>
      )}
      {openProcess && (
        <div className="process-card-wrap">
          <RecipeCard
            process={openProcess}
            goods={goods}
            onHide={() => setOpenProcess(null)}
            onOpenTree={
              processes.some((p) => p.id === openProcess.id && 'craft' in p.kind)
                ? () => setFocus(openProcess.id)
                : undefined
            }
            onMakeBrick={(fromGoodId, name, target) => addBrickGood(fromGoodId, name, target)}
          />
        </div>
      )}
    </div>
  )
}
