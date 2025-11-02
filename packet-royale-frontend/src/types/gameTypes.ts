/**
 * Game Data Types for Network Warfare
 */

export interface HexCoordinate {
  q: number; // Cube coordinate q (column)
  r: number; // Cube coordinate r (row)
  s: number; // Cube coordinate s (computed: q + r + s = 0)
}

export const NodeType = {
  NEUTRAL: 'NEUTRAL',
  OWNED: 'OWNED',
  BASE: 'BASE',
} as const;

export type NodeType = (typeof NodeType)[keyof typeof NodeType];

export const NodeState = {
  IDLE: 'IDLE',
  UNDER_ATTACK: 'UNDER_ATTACK',
  CAPTURING: 'CAPTURING',
  CAPTURED: 'CAPTURED',
} as const;

export type NodeState = (typeof NodeState)[keyof typeof NodeState];

export interface GameNode {
  id: string;
  position: HexCoordinate;
  type: NodeType;
  state: NodeState;
  ownerId: number | null; // null = neutral, 0-3 = player ID
  bandwidth: number;      // Current bandwidth capacity (Gbps)
  maxBandwidth: number;   // Maximum bandwidth capacity
  captureProgress: number; // 0-1, progress towards capture
  explored: boolean;      // Has this node been revealed?
}

export interface BandwidthStream {
  id: string;
  sourceNodeId: string;
  targetNodeId: string;
  ownerId: number;
  bandwidth: number;      // Current bandwidth usage (Gbps)
  maxBandwidth: number;   // Stream capacity
  packetsSent: number;    // Total packets transmitted
  packetsLost: number;    // Packets lost (for packet loss rate)
  active: boolean;
}

export interface PlayerState {
  id: number;
  name: string;
  color: number;
  totalThroughput: number;  // Gbps
  nodesOwned: number;
  maxNodes: number;
  baseNodeId: string;
  isAlive: boolean;
}

export interface GameState {
  players: PlayerState[];
  nodes: Map<string, GameNode>;
  streams: Map<string, BandwidthStream>;
  currentTick: number;
  currentPlayerId: number; // Local player
}

// Helper function to create hex coordinate
export function hexCoord(q: number, r: number): HexCoordinate {
  return { q, r, s: -q - r };
}

// Helper function to get node key
export function getNodeKey(coord: HexCoordinate): string {
  return `${coord.q},${coord.r}`;
}

// Helper function to calculate packet loss rate
export function getPacketLossRate(stream: BandwidthStream): number {
  if (stream.packetsSent === 0) return 0;
  return stream.packetsLost / stream.packetsSent;
}
