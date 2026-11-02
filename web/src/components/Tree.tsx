import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Background, Controls, Handle, MiniMap, Position, ReactFlow, applyNodeChanges } from '@xyflow/react'
import type { NodeChange, NodeProps, NodeTypes, OnNodeDrag, ReactFlowInstance } from '@xyflow/react'
import { NODE_WIDTH, estimateNodeHeight, layoutFlow } from '../layout'
import { formatChaos, toFlow, type FlowEdge, type FlowNode } from '../solution'
import { useAppStore } from '../store'
import type { NodeChip, Solution } from '../types'

function chipClass(chip: NodeChip): string {
  const ok = chip.kind === 'wanted' && chip.ok ? 'chip-ok' : ''
  return ['chip', `chip-${chip.kind}`, ok].filter(Boolean).join(' ')
}

function SolutionNodeCard({ data, selected }: NodeProps<FlowNode>) {
  const node = data.node
  const className = ['solution-node', selected ? 'is-selected' : '', node.kind].filter(Boolean).join(' ')

  if (!node.view) {
    return (
      <div className={className}>
        <Handle type="target" position={Position.Top} />
        <div className="node-title">{node.kind === 'start' ? 'Start' : 'Goal'}</div>
        <div className="node-value">{node.value == null ? '—' : `${formatChaos(node.value)} chaos`}</div>
        <Handle type="source" position={Position.Bottom} />
      </div>
    )
  }

  const view = node.view
  return (
    <div className={className}>
      <Handle type="target" position={Position.Top} />
      <div className="node-title">{view.title}</div>
      <div className="node-meta">
        {view.influences.length > 0 && <span>{view.influences.join(' + ')}</span>}
        {view.catalyst && <span className="badge">{view.catalyst}</span>}
        {view.lock && <span className="badge badge-lock">{view.lock}</span>}
        {node.expanded && <span className="badge badge-expanded">expanded</span>}
        {!node.inR && <span className="badge badge-unreachable">unreachable</span>}
      </div>
      <div className="node-sides">
        <ul className="chips">
          <li className="chips-label">P</li>
          {view.prefixes.map((chip, index) => (
            <li key={`p${index}`} className={chipClass(chip)}>
              {chip.kind === 'wanted' && chip.ok ? '✓ ' : ''}
              {chip.text}
            </li>
          ))}
        </ul>
        <ul className="chips">
          <li className="chips-label">S</li>
          {view.suffixes.map((chip, index) => (
            <li key={`s${index}`} className={chipClass(chip)}>
              {chip.kind === 'wanted' && chip.ok ? '✓ ' : ''}
              {chip.text}
            </li>
          ))}
        </ul>
      </div>
      <div className="node-value">{node.value == null ? 'unreachable' : `${formatChaos(node.value)} chaos`}</div>
      <Handle type="source" position={Position.Bottom} />
    </div>
  )
}

const nodeTypes: NodeTypes = { solutionNode: SolutionNodeCard }

interface TreeProps {
  solution: Solution
  selected: string
  onSelect: (key: string) => void
}

export function Tree({ solution, selected, onSelect }: TreeProps) {
  const positions = useAppStore((state) => state.positions)
  const setPosition = useAppStore((state) => state.setPosition)
  const flow = useMemo(() => toFlow(solution), [solution])
  const signature = useMemo(() => solution.nodes.map((node) => node.key).join('|'), [solution.nodes])
  const [layoutPositions, setLayoutPositions] = useState<Record<string, { x: number; y: number }>>({})
  const [instance, setInstance] = useState<ReactFlowInstance<FlowNode, FlowEdge> | null>(null)
  const selectedRef = useRef(selected)
  selectedRef.current = selected

  useEffect(() => {
    let cancelled = false
    layoutFlow(
      flow.nodes,
      flow.edges.filter((edge) => edge.data?.kind === 'craft'),
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
    // `flow` is derived from `solution`; re-layout only when the node set changes.
  }, [signature])

  // One expansion fans out into ~80 leaves in a single layer (~25,000 px wide), and
  // the expanded states end up far apart, so a fit-to-all view is unreadable. After
  // each layout, center on the node being worked on (Use selects it); the minimap
  // and the fit-view control give the overview.
  useEffect(() => {
    if (!instance) return
    const key = selectedRef.current
    const position = positions[key] ?? layoutPositions[key]
    const node = solution.nodes.find((candidate) => candidate.key === key)
    const handle = requestAnimationFrame(() => {
      if (position && node) {
        instance.setCenter(position.x + NODE_WIDTH / 2, position.y + estimateNodeHeight(node) / 2, { zoom: 0.8 })
      } else {
        instance.fitView({ padding: 0.2, maxZoom: 1 })
      }
    })
    return () => cancelAnimationFrame(handle)
  }, [instance, layoutPositions, signature])

  const canvasNodes = useMemo<FlowNode[]>(
    () =>
      flow.nodes.map((node) => ({
        ...node,
        selected: node.id === selected,
        position: positions[node.id] ?? layoutPositions[node.id] ?? node.position,
      })),
    [flow, positions, layoutPositions, selected],
  )

  const [nodes, setNodes] = useState<FlowNode[]>(canvasNodes)
  useEffect(() => {
    setNodes(canvasNodes)
  }, [canvasNodes])

  const onNodesChange = useCallback((changes: NodeChange<FlowNode>[]) => {
    setNodes((current) => applyNodeChanges(changes, current))
  }, [])

  const onNodeDragStop = useCallback<OnNodeDrag<FlowNode>>(
    (_event, node) => setPosition(node.id, { x: node.position.x, y: node.position.y }),
    [setPosition],
  )

  if (flow.nodes.length === 0) {
    return <div className="graph-empty">Nothing to show yet — paste a target item and the solver will grow the tree here.</div>
  }

  return (
    <div className="graph-canvas">
      <ReactFlow
        key={signature}
        nodes={nodes}
        edges={flow.edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onNodeDragStop={onNodeDragStop}
        onNodeClick={(_event, node) => onSelect(node.id)}
        onInit={setInstance}
        nodesConnectable={false}
        minZoom={0.02}
        proOptions={{ hideAttribution: true }}
      >
        <Background />
        <Controls showInteractive={false} />
        <MiniMap<FlowNode>
          pannable
          zoomable
          bgColor="#0e0f14"
          maskColor="rgba(18, 19, 26, 0.7)"
          nodeColor={(node) =>
            node.id === selected
              ? '#f0b44d'
              : node.data.node.kind !== 'state' || node.data.node.expanded
                ? '#63d18a'
                : '#4a4f66'
          }
        />
      </ReactFlow>
    </div>
  )
}
