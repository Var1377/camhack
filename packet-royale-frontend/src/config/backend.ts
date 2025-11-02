/**
 * Backend Configuration
 * Settings for connecting to CamHack backend
 */

export interface BackendConfig {
  // Backend server URL
  url: string;

  // Enable/disable backend connection (fallback to dummy data if false)
  enabled: boolean;

  // Polling interval in milliseconds (how often to fetch game state)
  pollingInterval: number;

  // Request timeout in milliseconds
  requestTimeout: number;

  // Number of retry attempts for failed requests
  retryAttempts: number;

  // Delay between retry attempts in milliseconds
  retryDelay: number;

  // Enable debug logging
  debug: boolean;
}

// Default configuration
const DEFAULT_CONFIG: BackendConfig = {
  url: import.meta.env.VITE_BACKEND_URL || 'http://localhost:8080',
  enabled: true,
  pollingInterval: 500, // Update every 500ms (2 Hz)
  requestTimeout: 5000, // 5 second timeout
  retryAttempts: 3,
  retryDelay: 1000, // 1 second between retries
  debug: import.meta.env.DEV || false, // Enable debug in dev mode
};

// Current active configuration (can be modified at runtime)
let activeConfig: BackendConfig = { ...DEFAULT_CONFIG };

/**
 * Get current backend configuration
 */
export function getBackendConfig(): Readonly<BackendConfig> {
  return { ...activeConfig };
}

/**
 * Update backend configuration
 */
export function updateBackendConfig(
  updates: Partial<BackendConfig>
): BackendConfig {
  activeConfig = {
    ...activeConfig,
    ...updates,
  };
  return { ...activeConfig };
}

/**
 * Reset configuration to defaults
 */
export function resetBackendConfig(): BackendConfig {
  activeConfig = { ...DEFAULT_CONFIG };
  return { ...activeConfig };
}

/**
 * Get backend URL
 */
export function getBackendUrl(): string {
  return activeConfig.url;
}

/**
 * Check if backend is enabled
 */
export function isBackendEnabled(): boolean {
  return activeConfig.enabled;
}

/**
 * Enable backend connection
 */
export function enableBackend(): void {
  activeConfig.enabled = true;
}

/**
 * Disable backend connection (fallback to dummy data)
 */
export function disableBackend(): void {
  activeConfig.enabled = false;
}

/**
 * Log debug message if debug mode is enabled
 */
export function debugLog(message: string, ...args: unknown[]): void {
  if (activeConfig.debug) {
    console.log(`[Backend] ${message}`, ...args);
  }
}

/**
 * Log error message
 */
export function errorLog(message: string, error?: unknown): void {
  console.error(`[Backend Error] ${message}`, error);
}
