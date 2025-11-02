/**
 * Graph-based network topology types
 */

export interface NetworkNode {
  id: string;
  position: { x: number; y: number }; // Actual pixel position
  type: 'NEUTRAL' | 'OWNED' | 'BASE';
  state: 'IDLE' | 'UNDER_ATTACK' | 'CAPTURING' | 'CAPTURED';
  ownerId: number | null; // null = neutral
  bandwidth: number;
  maxBandwidth: number;
  captureProgress: number;
  explored: boolean;
  connections: string[]; // IDs of connected nodes
  hexCoord?: { q: number; r: number }; // Hex grid coordinates from backend
}

export interface NetworkEdge {
  id: string;
  sourceNodeId: string;
  targetNodeId: string;
  ownerId: number;
  bandwidth: number;
  maxBandwidth: number;
  packetsSent: number;
  packetsLost: number;
  active: boolean;
}

export interface PlayerState {
  id: number;
  name: string;
  color: number;
  totalThroughput: number;
  nodesOwned: number;
  maxNodes: number;
  baseNodeId: string;
  isAlive: boolean;
}

export interface NetworkGameState {
  players: PlayerState[];
  nodes: Map<string, NetworkNode>;
  edges: Map<string, NetworkEdge>;
  currentTick: number;
  currentPlayerId: number;
}
