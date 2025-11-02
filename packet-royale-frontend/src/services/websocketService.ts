/**
 * WebSocket Service for Real-Time Game Updates
 *
 * Manages WebSocket connection to the backend with automatic reconnection
 * Falls back to HTTP polling if WebSocket is unavailable
 */

import { getBackendUrl } from '../config/backend';

export interface GameStateUpdate {
  log_index: number;
  event_count: number;
  player_count: number;
  node_count: number;
  alive_players: number;
  latest_event?: string;
}

export type UpdateCallback = (update: GameStateUpdate) => void;
export type ConnectionCallback = (connected: boolean) => void;
export type ErrorCallback = (error: Error) => void;

export class WebSocketService {
  private ws: WebSocket | null = null;
  private reconnectTimeout: number | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000; // Start with 1 second
  private maxReconnectDelay = 30000; // Max 30 seconds

  private updateCallbacks: UpdateCallback[] = [];
  private connectionCallbacks: ConnectionCallback[] = [];
  private errorCallbacks: ErrorCallback[] = [];

  private isConnected = false;
  private shouldReconnect = true;

  constructor() {
    console.log('[WebSocket] Service initialized');
  }

  /**
   * Connect to the WebSocket endpoint
   */
  public connect(): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      console.log('[WebSocket] Already connected');
      return;
    }

    const backendUrl = getBackendUrl();
    const wsUrl = backendUrl.replace('http://', 'ws://').replace('https://', 'wss://') + '/ws';

    console.log('[WebSocket] Connecting to:', wsUrl);

    try {
      this.ws = new WebSocket(wsUrl);

      this.ws.onopen = () => {
        console.log('[WebSocket] Connected successfully');
        this.isConnected = true;
        this.reconnectAttempts = 0;
        this.reconnectDelay = 1000; // Reset delay
        this.notifyConnection(true);
      };

      this.ws.onmessage = (event) => {
        try {
          const update: GameStateUpdate = JSON.parse(event.data);
          this.notifyUpdate(update);
        } catch (error) {
          console.error('[WebSocket] Failed to parse message:', error);
          this.notifyError(new Error('Failed to parse WebSocket message'));
        }
      };

      this.ws.onerror = (event) => {
        console.error('[WebSocket] Error:', event);
        this.notifyError(new Error('WebSocket error occurred'));
      };

      this.ws.onclose = () => {
        console.log('[WebSocket] Connection closed');
        this.isConnected = false;
        this.ws = null;
        this.notifyConnection(false);

        // Attempt reconnection if enabled
        if (this.shouldReconnect && this.reconnectAttempts < this.maxReconnectAttempts) {
          this.scheduleReconnect();
        } else if (this.reconnectAttempts >= this.maxReconnectAttempts) {
          console.warn('[WebSocket] Max reconnection attempts reached. Giving up.');
          this.notifyError(new Error('Max reconnection attempts reached'));
        }
      };

    } catch (error) {
      console.error('[WebSocket] Failed to create connection:', error);
      this.notifyError(error as Error);
      this.scheduleReconnect();
    }
  }

  /**
   * Disconnect from WebSocket
   */
  public disconnect(): void {
    console.log('[WebSocket] Disconnecting...');
    this.shouldReconnect = false;

    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  /**
   * Schedule a reconnection attempt
   */
  private scheduleReconnect(): void {
    if (this.reconnectTimeout) {
      return; // Already scheduled
    }

    this.reconnectAttempts++;
    const delay = Math.min(this.reconnectDelay * this.reconnectAttempts, this.maxReconnectDelay);

    console.log(`[WebSocket] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);

    this.reconnectTimeout = window.setTimeout(() => {
      this.reconnectTimeout = null;
      this.connect();
    }, delay);
  }

  /**
   * Check if currently connected
   */
  public isWebSocketConnected(): boolean {
    return this.isConnected && this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }

  /**
   * Register callback for game state updates
   */
  public onUpdate(callback: UpdateCallback): void {
    this.updateCallbacks.push(callback);
  }

  /**
   * Register callback for connection status changes
   */
  public onConnectionChange(callback: ConnectionCallback): void {
    this.connectionCallbacks.push(callback);
  }

  /**
   * Register callback for errors
   */
  public onError(callback: ErrorCallback): void {
    this.errorCallbacks.push(callback);
  }

  /**
   * Remove a callback
   */
  public removeCallback(callback: UpdateCallback | ConnectionCallback | ErrorCallback): void {
    this.updateCallbacks = this.updateCallbacks.filter(cb => cb !== callback);
    this.connectionCallbacks = this.connectionCallbacks.filter(cb => cb !== callback);
    this.errorCallbacks = this.errorCallbacks.filter(cb => cb !== callback);
  }

  /**
   * Notify all update callbacks
   */
  private notifyUpdate(update: GameStateUpdate): void {
    this.updateCallbacks.forEach(callback => {
      try {
        callback(update);
      } catch (error) {
        console.error('[WebSocket] Error in update callback:', error);
      }
    });
  }

  /**
   * Notify all connection callbacks
   */
  private notifyConnection(connected: boolean): void {
    this.connectionCallbacks.forEach(callback => {
      try {
        callback(connected);
      } catch (error) {
        console.error('[WebSocket] Error in connection callback:', error);
      }
    });
  }

  /**
   * Notify all error callbacks
   */
  private notifyError(error: Error): void {
    this.errorCallbacks.forEach(callback => {
      try {
        callback(error);
      } catch (error) {
        console.error('[WebSocket] Error in error callback:', error);
      }
    });
  }

  /**
   * Reset reconnection state (useful for manual reconnect)
   */
  public resetReconnection(): void {
    this.reconnectAttempts = 0;
    this.shouldReconnect = true;
  }
}

// Singleton instance
let wsServiceInstance: WebSocketService | null = null;

/**
 * Get or create the WebSocket service singleton
 */
export function getWebSocketService(): WebSocketService {
  if (!wsServiceInstance) {
    wsServiceInstance = new WebSocketService();
  }
  return wsServiceInstance;
}
