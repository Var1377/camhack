/**
 * LobbyScene - Game discovery and join screen
 */

import Phaser from 'phaser';
import { discoverGames, joinGameLobby, checkJoinStatus } from '../services/backendApi';
import type { GameInfo } from '../services/backendApi';
import { COLORS } from '../config/visualConstants';

export class LobbyScene extends Phaser.Scene {
  private games: GameInfo[] = [];
  private selectedGameId: string = '';
  private playerName: string = '';
  private statusText!: Phaser.GameObjects.Text;
  private gamesListContainer!: Phaser.GameObjects.Container;
  private refreshTimer?: Phaser.Time.TimerEvent;

  constructor() {
    super({ key: 'LobbyScene' });
  }

  create() {
    const centerX = this.cameras.main.centerX;
    const centerY = this.cameras.main.centerY;

    // Title
    this.add.text(centerX, 50, 'CAMHACK - PACKET ROYALE', {
      fontSize: '48px',
      color: COLORS.GRID_LINE,
      fontFamily: 'monospace',
    }).setOrigin(0.5);

    // Subtitle
    this.add.text(centerX, 110, 'Distributed Network Flooding Game', {
      fontSize: '18px',
      color: COLORS.GRID_LINE,
      fontFamily: 'monospace',
    }).setOrigin(0.5);

    // Status text
    this.statusText = this.add.text(centerX, 160, 'Connecting to backend...', {
      fontSize: '16px',
      color: '#00ff00',
      fontFamily: 'monospace',
    }).setOrigin(0.5);

    // Create input fields
    this.createPlayerNameInput(centerX, 220);

    // Create game list container
    this.gamesListContainer = this.add.container(centerX - 300, 300);

    // Buttons
    this.createRefreshButton(centerX - 120, 500);
    this.createCreateGameButton(centerX + 120, 500);

    // Instructions
    this.add.text(centerX, centerY + 250,
      'Select a game from the list or create a new one\nEnter your player name and click Join', {
      fontSize: '14px',
      color: '#888888',
      fontFamily: 'monospace',
      align: 'center',
    }).setOrigin(0.5);

    // Check join status and load games
    this.checkStatus();
    this.refreshGamesList();

    // Auto-refresh every 5 seconds
    this.refreshTimer = this.time.addEvent({
      delay: 5000,
      callback: this.refreshGamesList,
      callbackScope: this,
      loop: true,
    });
  }

  private createPlayerNameInput(x: number, y: number) {
    // Label
    this.add.text(x - 200, y, 'Player Name:', {
      fontSize: '18px',
      color: COLORS.GRID_LINE,
      fontFamily: 'monospace',
    });

    // Create HTML input element
    const inputElement = document.createElement('input');
    inputElement.type = 'text';
    inputElement.placeholder = 'Enter your name';
    inputElement.style.position = 'absolute';
    inputElement.style.left = `${x - 50}px`;
    inputElement.style.top = `${y - 15}px`;
    inputElement.style.width = '200px';
    inputElement.style.padding = '8px';
    inputElement.style.fontSize = '16px';
    inputElement.style.fontFamily = 'monospace';
    inputElement.style.backgroundColor = '#1a1a1a';
    inputElement.style.color = COLORS.GRID_LINE;
    inputElement.style.border = `2px solid ${COLORS.GRID_LINE}`;
    inputElement.style.borderRadius = '4px';
    inputElement.value = this.playerName;

    inputElement.addEventListener('input', (e) => {
      this.playerName = (e.target as HTMLInputElement).value;
    });

    document.body.appendChild(inputElement);

    // Clean up on scene shutdown
    this.events.once('shutdown', () => {
      inputElement.remove();
    });
  }

  private createRefreshButton(x: number, y: number) {
    const button = this.add.rectangle(x, y, 200, 50, 0x003366)
      .setInteractive({ useHandCursor: true })
      .on('pointerover', () => button.setFillStyle(0x004488))
      .on('pointerout', () => button.setFillStyle(0x003366))
      .on('pointerdown', () => this.refreshGamesList());

    this.add.text(x, y, 'Refresh Games', {
      fontSize: '18px',
      color: COLORS.GRID_LINE,
      fontFamily: 'monospace',
    }).setOrigin(0.5);
  }

  private createCreateGameButton(x: number, y: number) {
    const button = this.add.rectangle(x, y, 200, 50, 0x336600)
      .setInteractive({ useHandCursor: true })
      .on('pointerover', () => button.setFillStyle(0x448800))
      .on('pointerout', () => button.setFillStyle(0x336600))
      .on('pointerdown', () => this.createNewGame());

    this.add.text(x, y, 'Create New Game', {
      fontSize: '18px',
      color: COLORS.GRID_LINE,
      fontFamily: 'monospace',
    }).setOrigin(0.5);
  }

  private async checkStatus() {
    try {
      const status = await checkJoinStatus();
      if (status.joined) {
        this.statusText.setText(`Already joined game: ${status.game_id || 'unknown'}`);
        this.statusText.setColor('#00ff00');

        console.log('Auto-joining with player_id:', status.player_id);

        // Auto-transition to game scene after 2 seconds with player_id
        this.time.delayedCall(2000, () => {
          this.scene.start('GraphGameScene', { playerId: status.player_id });
        });
      } else {
        this.statusText.setText('Not joined to any game - select or create one below');
        this.statusText.setColor('#ffaa00');
      }
    } catch (error) {
      this.statusText.setText('Backend not reachable - check client is running');
      this.statusText.setColor('#ff0000');
      console.error('Failed to check join status:', error);
    }
  }

  private async refreshGamesList() {
    try {
      const response = await discoverGames();
      this.games = response.games;
      this.displayGamesList();

      if (this.games.length === 0) {
        this.statusText.setText('No active games found - create a new one!');
        this.statusText.setColor('#ffaa00');
      }
    } catch (error) {
      console.error('Failed to fetch games:', error);
      this.statusText.setText('Failed to fetch games list');
      this.statusText.setColor('#ff0000');
    }
  }

  private displayGamesList() {
    // Clear existing list
    this.gamesListContainer.removeAll(true);

    if (this.games.length === 0) {
      const emptyText = this.add.text(300, 0, 'No active games', {
        fontSize: '16px',
        color: '#888888',
        fontFamily: 'monospace',
      }).setOrigin(0.5, 0);
      this.gamesListContainer.add(emptyText);
      return;
    }

    // Header
    const header = this.add.text(0, 0, 'ACTIVE GAMES:', {
      fontSize: '20px',
      color: COLORS.GRID_LINE,
      fontFamily: 'monospace',
    });
    this.gamesListContainer.add(header);

    // List games
    this.games.forEach((game, index) => {
      const yPos = 40 + (index * 60);

      // Game container background
      const isSelected = this.selectedGameId === game.game_id;
      const bgColor = isSelected ? 0x224466 : 0x1a1a1a;
      const bg = this.add.rectangle(300, yPos, 580, 50, bgColor)
        .setInteractive({ useHandCursor: true })
        .on('pointerover', function(this: Phaser.GameObjects.Rectangle) {
          if (!isSelected) this.setFillStyle(0x2a2a2a);
        })
        .on('pointerout', function(this: Phaser.GameObjects.Rectangle) {
          if (!isSelected) this.setFillStyle(0x1a1a1a);
        })
        .on('pointerdown', () => this.selectGame(game.game_id));

      // Game info text
      const ageMinutes = Math.floor((Date.now() / 1000 - game.created_at_secs) / 60);
      const ageText = ageMinutes < 1 ? 'just now' : `${ageMinutes}m ago`;

      const gameText = this.add.text(20, yPos,
        `${game.game_id}  |  ${game.worker_count} workers  |  Created ${ageText}`, {
        fontSize: '16px',
        color: COLORS.GRID_LINE,
        fontFamily: 'monospace',
      }).setOrigin(0, 0.5);

      // Join button
      const joinBtn = this.add.rectangle(500, yPos, 80, 35, 0x006633)
        .setInteractive({ useHandCursor: true })
        .on('pointerover', function(this: Phaser.GameObjects.Rectangle) {
          this.setFillStyle(0x008844);
        })
        .on('pointerout', function(this: Phaser.GameObjects.Rectangle) {
          this.setFillStyle(0x006633);
        })
        .on('pointerdown', () => this.joinGame(game.game_id));

      const joinText = this.add.text(500, yPos, 'Join', {
        fontSize: '14px',
        color: '#ffffff',
        fontFamily: 'monospace',
      }).setOrigin(0.5);

      this.gamesListContainer.add([bg, gameText, joinBtn, joinText]);
    });
  }

  private selectGame(gameId: string) {
    this.selectedGameId = gameId;
    this.displayGamesList(); // Redraw to show selection
  }

  private async createNewGame() {
    if (!this.playerName.trim()) {
      this.statusText.setText('Please enter your player name first!');
      this.statusText.setColor('#ff0000');
      return;
    }

    // Generate random game ID
    const gameId = `game-${Date.now()}`;
    await this.joinGame(gameId);
  }

  private async joinGame(gameId: string) {
    if (!this.playerName.trim()) {
      this.statusText.setText('Please enter your player name first!');
      this.statusText.setColor('#ff0000');
      return;
    }

    try {
      this.statusText.setText(`Joining ${gameId}...`);
      this.statusText.setColor('#ffaa00');

      const result = await joinGameLobby(this.playerName, gameId);

      this.statusText.setText(`Joined successfully! Getting player info...`);
      this.statusText.setColor('#00ff00');

      console.log('Join result:', result);

      // Get player_id from status endpoint
      const status = await checkJoinStatus();
      if (!status.joined || !status.player_id) {
        throw new Error('Failed to get player_id after join');
      }

      console.log('Player ID:', status.player_id);

      // Stop auto-refresh
      if (this.refreshTimer) {
        this.refreshTimer.remove();
      }

      // Transition to game scene with player_id
      this.time.delayedCall(1000, () => {
        this.scene.start('GraphGameScene', { playerId: status.player_id });
      });
    } catch (error) {
      this.statusText.setText(`Failed to join: ${error}`);
      this.statusText.setColor('#ff0000');
      console.error('Join error:', error);
    }
  }

  shutdown() {
    if (this.refreshTimer) {
      this.refreshTimer.remove();
    }
  }
}
