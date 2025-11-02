/**
 * Packet Royale - Network Warfare Game
 * Main entry point
 */

import Phaser from 'phaser';
import { GameScene } from './scenes/GameScene';
import { GraphGameScene } from './scenes/GraphGameScene';
import { UIScene } from './scenes/UIScene';
import './style.css';

// Choose which game scene to use based on environment or toggle
const USE_HEX_GRID = false; // Set to false to use graph-based scene

const config: Phaser.Types.Core.GameConfig = {
  type: Phaser.AUTO,
  width: 1280,
  height: 720,
  backgroundColor: '#0a0a1a',
  parent: 'app',
  scene: USE_HEX_GRID ? [GameScene] : [GraphGameScene, UIScene],
  physics: {
    default: 'arcade',
    arcade: {
      debug: false,
    },
  },
  render: {
    antialias: true,
    pixelArt: false,
  },
  scale: {
    mode: Phaser.Scale.FIT,
    autoCenter: Phaser.Scale.CENTER_BOTH,
  },
};

new Phaser.Game(config);

console.log('🎮 Packet Royale initialized (mode:', USE_HEX_GRID ? 'HEX GRID' : 'GRAPH', ')');
console.log('🔌 Backend integration:', import.meta.env.VITE_BACKEND_URL || 'localhost:8080');
