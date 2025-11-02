/**
 * Backend API Service
 * Handles communication with CamHack backend server
 */

// Backend API Response Types (matching Rust backend structures)
export interface BackendNodeCoord {
  q: number;
  r: number;
}

export interface BackendPlayerInfo {
  player_id: number;
  name: string;
  capital_coord: BackendNodeCoord;
  alive: boolean;
  node_count: number;
}

// Backend sends AttackTarget as an enum: { Coordinate: {...} } | { Player: number }
export type BackendAttackTarget =
  | { Coordinate: BackendNodeCoord }
  | { Player: number };

export interface BackendNodeInfo {
  coord: BackendNodeCoord;
  owner_id: number | null;
  current_target: BackendAttackTarget | null;  // Updated to handle enum
  bandwidth_in?: number; // Bytes per second (optional - may not be available)
  packet_loss?: number; // 0.0 to 1.0 (optional - may not be available)
}

export interface BackendGameState {
  players: BackendPlayerInfo[];
  nodes: BackendNodeInfo[];
  total_events: number;
}

export interface BackendError {
  error: string;
}

// Game Discovery Types (for lobby screen)
export interface GameInfo {
  game_id: string;
  worker_count: number;
  created_at_secs: number;
}

export interface GameListResponse {
  games: GameInfo[];
}

export interface JoinRequest {
  player_name: string;
  game_id: string;
}

export interface JoinStatusResponse {
  joined: boolean;
  player_id?: number;
  player_name?: string;
  game_id?: string;
}

// Configuration
const DEFAULT_BACKEND_URL = 'http://localhost:8080';

function getBackendUrl(): string {
  // Check environment variable first (Vite uses import.meta.env)
  if (import.meta.env.VITE_BACKEND_URL) {
    return import.meta.env.VITE_BACKEND_URL;
  }
  return DEFAULT_BACKEND_URL;
}

/**
 * Fetch current game state from backend
 */
export async function fetchGameState(): Promise<BackendGameState> {
  const url = `${getBackendUrl()}/game/state`;

  try {
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
      },
    });

    if (!response.ok) {
      const errorData: BackendError = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
      }));
      throw new Error(`Failed to fetch game state: ${errorData.error}`);
    }

    const data: BackendGameState = await response.json();
    return data;
  } catch (error) {
    if (error instanceof Error) {
      console.error('Backend API Error:', error.message);
      throw error;
    }
    throw new Error('Unknown error fetching game state');
  }
}

/**
 * Set attack target for a node
 * @param nodeCoord Source node coordinates
 * @param targetCoord Target node coordinates (null to stop attacking)
 */
export async function setAttackTarget(
  nodeCoord: BackendNodeCoord,
  targetCoord: BackendNodeCoord | null
): Promise<void> {
  const url = `${getBackendUrl()}/events`;

  try {
    const event = {
      SetNodeTarget: {
        node_coord: nodeCoord,
        target_coord: targetCoord,
        timestamp: Date.now(),
      },
    };

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(event),
    });

    if (!response.ok) {
      const errorData: BackendError = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
      }));
      throw new Error(`Failed to set attack target: ${errorData.error}`);
    }
  } catch (error) {
    if (error instanceof Error) {
      console.error('Backend API Error:', error.message);
      throw error;
    }
    throw new Error('Unknown error setting attack target');
  }
}

/**
 * Stop attacking (convenience wrapper for setAttackTarget with null)
 */
export async function stopAttack(nodeCoord: BackendNodeCoord): Promise<void> {
  return setAttackTarget(nodeCoord, null);
}

/**
 * Join game as a player
 * @param playerName Player's display name
 * @param capitalCoord Starting capital coordinates
 */
export async function joinGame(
  playerName: string,
  capitalCoord: BackendNodeCoord
): Promise<number> {
  const url = `${getBackendUrl()}/events`;

  try {
    const playerId = Date.now(); // Use timestamp as unique player ID
    const event = {
      PlayerJoin: {
        player_id: playerId,
        name: playerName,
        capital_coord: capitalCoord,
        node_ip: '0.0.0.0', // Frontend doesn't have real IP
        timestamp: Date.now(),
      },
    };

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(event),
    });

    if (!response.ok) {
      const errorData: BackendError = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
      }));
      throw new Error(`Failed to join game: ${errorData.error}`);
    }

    return playerId;
  } catch (error) {
    if (error instanceof Error) {
      console.error('Backend API Error:', error.message);
      throw error;
    }
    throw new Error('Unknown error joining game');
  }
}

/**
 * Check if backend is reachable
 */
export async function pingBackend(): Promise<boolean> {
  try {
    const response = await fetch(`${getBackendUrl()}/game/state`, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
      },
    });
    // Backend is reachable if we get ANY valid HTTP response (even errors)
    // 200 = Game state returned successfully
    // 400 = Not joined yet (but server is alive and responding)
    // Both indicate backend is reachable
    return response.ok || response.status === 400;
  } catch {
    return false;
  }
}

/**
 * Get backend connection info
 */
export function getBackendInfo(): { url: string; configured: boolean } {
  const url = getBackendUrl();
  const configured = !!import.meta.env.VITE_BACKEND_URL;
  return { url, configured };
}

/**
 * Discover available games
 * Calls client's /discover endpoint which queries the master
 */
export async function discoverGames(): Promise<GameListResponse> {
  const url = `${getBackendUrl()}/discover`;

  try {
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
      },
    });

    if (!response.ok) {
      const errorData: BackendError = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
      }));
      throw new Error(`Failed to discover games: ${errorData.error}`);
    }

    const data: GameListResponse = await response.json();
    return data;
  } catch (error) {
    if (error instanceof Error) {
      console.error('Backend API Error:', error.message);
      throw error;
    }
    throw new Error('Unknown error discovering games');
  }
}

/**
 * Join a game (creates new game if doesn't exist)
 * @param playerName Player's display name
 * @param gameId Game identifier (will be created if doesn't exist)
 */
export async function joinGameLobby(
  playerName: string,
  gameId: string
): Promise<string> {
  const url = `${getBackendUrl()}/join`;

  try {
    const request: JoinRequest = {
      player_name: playerName,
      game_id: gameId,
    };

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const errorData: BackendError = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
      }));
      throw new Error(`Failed to join game: ${errorData.error}`);
    }

    const result = await response.text();
    return result;
  } catch (error) {
    if (error instanceof Error) {
      console.error('Backend API Error:', error.message);
      throw error;
    }
    throw new Error('Unknown error joining game');
  }
}

/**
 * Check current join status
 */
export async function checkJoinStatus(): Promise<JoinStatusResponse> {
  const url = `${getBackendUrl()}/status`;

  try {
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
      },
    });

    if (!response.ok) {
      const errorData: BackendError = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
      }));
      throw new Error(`Failed to check join status: ${errorData.error}`);
    }

    const data: JoinStatusResponse = await response.json();
    return data;
  } catch (error) {
    if (error instanceof Error) {
      console.error('Backend API Error:', error.message);
      throw error;
    }
    throw new Error('Unknown error checking join status');
  }
}
