/**
 * UIScene - Cyberpunk HUD overlay
 */

import Phaser from "phaser";
import type { GameState } from "../types/gameTypes";
import type { NetworkGameState, NetworkNode } from "../types/graphTypes";
import { COLORS } from "../config/visualConstants";
import {
  initiateCapture,
  canAttackEnemyBase,
  launchAttack,
} from "../utils/graphData";

export class UIScene extends Phaser.Scene {
  // UI Elements
  private throughputValueText!: Phaser.GameObjects.Text;
  private nodeCountText!: Phaser.GameObjects.Text;
  private enemyThroughputText!: Phaser.GameObjects.Text;
  private enemyNodeCountText!: Phaser.GameObjects.Text;
  private buildButton!: Phaser.GameObjects.Text;
  private upgradeButton!: Phaser.GameObjects.Text;
  private attackButton!: Phaser.GameObjects.Text;

  // Background panels
  private topBar!: Phaser.GameObjects.Graphics;
  private bottomBar!: Phaser.GameObjects.Graphics;

  // Game state
  private selectedNode: NetworkNode | null = null;
  private gameState: NetworkGameState | null = null;
  private captureMode: boolean = false;

  // Scanline effect
  private scanline!: Phaser.GameObjects.Graphics;
  private scanlineY = 0;

  constructor() {
    super({ key: "UIScene" });
  }

  create() {
    // Draw UI background panels
    this.createBackgroundPanels();

    // Create top bar UI
    this.createTopBar();

    // Create bottom bar UI
    this.createBottomBar();

    // Create scanline effect
    this.createScanlineEffect();

    // Listen for game state updates
    const gameScene = this.scene.get("GraphGameScene");
    gameScene.events.on("gameStateUpdated", this.updateUI, this);
    gameScene.events.on("nodeSelected", this.onNodeSelected, this);
    gameScene.events.on("captureAttempted", this.onCaptureAttempted, this);

    console.log("UIScene initialized");
  }

  private onNodeSelected(node: NetworkNode) {
    this.selectedNode = node;
    this.updateButtonStates();
  }

  private createBackgroundPanels() {
    const { width, height } = this.cameras.main;

    // Top bar background
    this.topBar = this.add.graphics();
    this.topBar.fillStyle(0x000000, 0.7);
    this.topBar.fillRect(0, 0, width, 60);

    // Top bar border
    this.topBar.lineStyle(2, COLORS.UI_PRIMARY, 0.8);
    this.topBar.lineBetween(0, 60, width, 60);

    // Bottom bar background
    this.bottomBar = this.add.graphics();
    this.bottomBar.fillStyle(0x000000, 0.7);
    this.bottomBar.fillRect(0, height - 80, width, 80);

    // Bottom bar border
    this.bottomBar.lineStyle(2, COLORS.UI_PRIMARY, 0.8);
    this.bottomBar.lineBetween(0, height - 80, width, height - 80);

    // Make UI fixed to camera
    this.topBar.setScrollFactor(0);
    this.bottomBar.setScrollFactor(0);
  }

  private createTopBar() {
    const { width } = this.cameras.main;

    // Title/Logo
    const titleText = this.add.text(20, 15, "⚡ PACKET ROYALE", {
      fontSize: "24px",
      fontFamily: "monospace",
      color: "#00ffff",
      fontStyle: "bold",
    });
    titleText.setScrollFactor(0);

    // Player 1 (You) - Left side
    const throughputLabel = this.add.text(400, 20, "THROUGHPUT:", {
      fontSize: "16px",
      fontFamily: "monospace",
      color: "#99ccff",
    });
    throughputLabel.setScrollFactor(0);

    this.throughputValueText = this.add.text(520, 18, "0.0 Gbps", {
      fontSize: "20px",
      fontFamily: "monospace",
      color: "#00ff88",
      fontStyle: "bold",
    });
    this.throughputValueText.setScrollFactor(0);

    this.nodeCountText = this.add.text(400, 38, "NODES: 0/10", {
      fontSize: "14px",
      fontFamily: "monospace",
      color: "#99ccff",
    });
    this.nodeCountText.setScrollFactor(0);

    const playerIndicator = this.add.graphics();
    playerIndicator.fillStyle(COLORS.PLAYER_1, 1);
    playerIndicator.fillCircle(330, 30, 8);
    playerIndicator.lineStyle(2, COLORS.PLAYER_1, 0.5);
    playerIndicator.strokeCircle(330, 30, 12);
    playerIndicator.setScrollFactor(0);

    const playerLabel = this.add.text(350, 20, "YOU", {
      fontSize: "16px",
      fontFamily: "monospace",
      color: "#00ffff",
    });
    playerLabel.setScrollFactor(0);

    // Enemy Player - Right side
    const enemyIndicator = this.add.graphics();
    enemyIndicator.fillStyle(COLORS.PLAYER_2, 1);
    enemyIndicator.fillCircle(width - 420, 30, 8);
    enemyIndicator.lineStyle(2, COLORS.PLAYER_2, 0.5);
    enemyIndicator.strokeCircle(width - 420, 30, 12);
    enemyIndicator.setScrollFactor(0);

    const enemyLabel = this.add.text(width - 400, 20, "ENEMY", {
      fontSize: "16px",
      fontFamily: "monospace",
      color: "#ff006e",
    });
    enemyLabel.setScrollFactor(0);

    const enemyThroughputLabel = this.add.text(width - 330, 20, "THROUGHPUT:", {
      fontSize: "16px",
      fontFamily: "monospace",
      color: "#ff99bb",
    });
    enemyThroughputLabel.setScrollFactor(0);

    this.enemyThroughputText = this.add.text(width - 210, 18, "0.0 Gbps", {
      fontSize: "20px",
      fontFamily: "monospace",
      color: "#ff6699",
      fontStyle: "bold",
    });
    this.enemyThroughputText.setScrollFactor(0);

    this.enemyNodeCountText = this.add.text(width - 330, 38, "NODES: 0/10", {
      fontSize: "14px",
      fontFamily: "monospace",
      color: "#ff99bb",
    });
    this.enemyNodeCountText.setScrollFactor(0);
  }

  private createBottomBar() {
    const { height } = this.cameras.main;
    const buttonY = height - 50;

    // Build Stream button (capture adjacent neutral nodes)
    this.buildButton = this.createButton(
      50,
      buttonY,
      "[ CAPTURE NODE ]",
      () => {
        this.handleCaptureNode();
      }
    );

    // Upgrade Node button (future feature)
    this.upgradeButton = this.createButton(
      300,
      buttonY,
      "[ UPGRADE NODE ]",
      () => {
        console.log("Upgrade Node clicked (not yet implemented)");
        this.flashButton(this.upgradeButton);
      }
    );
    this.upgradeButton.setAlpha(0.5); // Disabled for now

    // Attack button
    this.attackButton = this.createButton(
      550,
      buttonY,
      "[ LAUNCH ATTACK ]",
      () => {
        this.handleLaunchAttack();
      }
    );
    this.attackButton.setAlpha(0.5); // Disabled initially

    // Instructions text
    const instructions = this.add.text(
      850,
      buttonY - 10,
      "DRAG: Pan | +/-: Zoom | F: Toggle Fog",
      {
        fontSize: "12px",
        fontFamily: "monospace",
        color: "#666699",
      }
    );
    instructions.setScrollFactor(0);
  }

  private handleCaptureNode() {
    // Toggle capture mode
    this.captureMode = !this.captureMode;

    if (this.captureMode) {
      console.log("🎯 Capture mode ENABLED - Click on a node to capture it");
      this.buildButton.setStyle({
        backgroundColor: "#006633",
        color: "#00ff88",
      });
      this.buildButton.setText("[ CAPTURE MODE: ON ]");

      // Emit event from UIScene
      console.log("📤 UIScene emitting captureModeChanged: true");
      this.events.emit("captureModeChanged", true);
    } else {
      console.log("🎯 Capture mode DISABLED");
      this.buildButton.setStyle({
        backgroundColor: "#001a33",
        color: "#00ffff",
      });
      this.buildButton.setText("[ CAPTURE NODE ]");

      // Emit event from UIScene
      console.log("📤 UIScene emitting captureModeChanged: false");
      this.events.emit("captureModeChanged", false);
    }
  }

  private onCaptureAttempted(result: { success: boolean; message: string }) {
    // Keep capture mode active - don't auto-disable
    // User must manually toggle the button to exit capture mode

    // Show feedback
    if (result.success) {
      console.log(`✅ ${result.message}`);
      this.flashButton(this.buildButton);
    } else {
      console.log(`⚠ ${result.message}`);
    }
  }

  private handleLaunchAttack() {
    if (!this.gameState) return;

    const playerId = this.gameState.currentPlayerId;

    if (!canAttackEnemyBase(this.gameState, playerId)) {
      console.log(
        "⚠ Cannot attack yet - you must control nodes adjacent to enemy base"
      );
      return;
    }

    this.showAttackConfirmation();
  }

  private createButton(
    x: number,
    y: number,
    text: string,
    onClick: () => void
  ): Phaser.GameObjects.Text {
    const button = this.add.text(x, y, text, {
      fontSize: "18px",
      fontFamily: "monospace",
      color: "#00ffff",
      backgroundColor: "#001a33",
      padding: { x: 15, y: 10 },
    });

    button.setScrollFactor(0);
    button.setInteractive({ useHandCursor: true });

    // Hover effect
    button.on("pointerover", () => {
      button.setStyle({
        backgroundColor: "#003366",
        color: "#ffffff",
      });
    });

    button.on("pointerout", () => {
      button.setStyle({
        backgroundColor: "#001a33",
        color: "#00ffff",
      });
    });

    button.on("pointerdown", onClick);

    return button;
  }

  private flashButton(button: Phaser.GameObjects.Text) {
    this.tweens.add({
      targets: button,
      alpha: 0.3,
      duration: 100,
      yoyo: true,
      repeat: 1,
    });
  }

  private createScanlineEffect() {
    const { width, height } = this.cameras.main;

    this.scanline = this.add.graphics();
    this.scanline.setScrollFactor(0);
    this.scanline.setAlpha(0.15);

    // Animate scanline
    this.time.addEvent({
      delay: 16, // ~60fps
      callback: () => {
        this.scanlineY = (this.scanlineY + 2) % height;

        this.scanline.clear();
        this.scanline.lineStyle(2, COLORS.UI_PRIMARY, 1);
        this.scanline.lineBetween(0, this.scanlineY, width, this.scanlineY);

        // Add gradient effect
        this.scanline.lineStyle(1, COLORS.UI_PRIMARY, 0.5);
        this.scanline.lineBetween(
          0,
          this.scanlineY - 1,
          width,
          this.scanlineY - 1
        );
        this.scanline.lineBetween(
          0,
          this.scanlineY + 1,
          width,
          this.scanlineY + 1
        );
      },
      loop: true,
    });
  }

  private updateUI(gameState: GameState | NetworkGameState) {
    this.gameState = gameState as NetworkGameState;

    // Update player stats
    const player = gameState.players[gameState.currentPlayerId];
    if (player) {
      this.throughputValueText.setText(
        `${player.totalThroughput.toFixed(1)} Gbps`
      );
      this.nodeCountText.setText(
        `NODES: ${player.nodesOwned}/${player.maxNodes}`
      );
    }

    // Update enemy stats (Player 2)
    const enemy = gameState.players[1];
    if (enemy) {
      this.enemyThroughputText.setText(
        `${enemy.totalThroughput.toFixed(1)} Gbps`
      );
      this.enemyNodeCountText.setText(
        `NODES: ${enemy.nodesOwned}/${enemy.maxNodes}`
      );
    }

    // Update button states
    this.updateButtonStates();
  }

  private updateButtonStates() {
    if (!this.gameState) return;

    // Enable attack button if we can attack enemy base
    const canAttack = canAttackEnemyBase(
      this.gameState,
      this.gameState.currentPlayerId
    );
    this.attackButton.setAlpha(canAttack ? 1 : 0.5);
    this.attackButton.setInteractive(canAttack);
  }

  private showVictoryScreen() {
    const { width, height } = this.cameras.main;

    // Create victory overlay
    const overlay = this.add.graphics();
    overlay.fillStyle(0x000000, 0.9);
    overlay.fillRect(0, 0, width, height);
    overlay.setScrollFactor(0);

    // Victory text
    const victoryText = this.add.text(
      width / 2,
      height / 2 - 50,
      "🎯 VICTORY! 🎯",
      {
        fontSize: "48px",
        fontFamily: "monospace",
        color: "#00ff88",
        fontStyle: "bold",
      }
    );
    victoryText.setOrigin(0.5);
    victoryText.setScrollFactor(0);

    // Subtitle
    const subtitle = this.add.text(
      width / 2,
      height / 2 + 20,
      "Enemy base successfully compromised!",
      {
        fontSize: "24px",
        fontFamily: "monospace",
        color: "#00ffff",
      }
    );
    subtitle.setOrigin(0.5);
    subtitle.setScrollFactor(0);

    // Pulse effect
    this.tweens.add({
      targets: victoryText,
      scale: 1.1,
      duration: 800,
      yoyo: true,
      repeat: -1,
    });
  }

  private showAttackConfirmation() {
    const { width, height } = this.cameras.main;

    // Create modal overlay
    const overlay = this.add.graphics();
    overlay.fillStyle(0x000000, 0.8);
    overlay.fillRect(0, 0, width, height);
    overlay.setScrollFactor(0);

    // Warning text
    const warningText = this.add.text(
      width / 2,
      height / 2 - 100,
      "⚠ CONNECTION ESTABLISHED ⚠",
      {
        fontSize: "32px",
        fontFamily: "monospace",
        color: "#ff0044",
        fontStyle: "bold",
      }
    );
    warningText.setOrigin(0.5);
    warningText.setScrollFactor(0);

    // Subtitle
    const subtitle = this.add.text(
      width / 2,
      height / 2 - 50,
      "TARGET ACQUIRED",
      {
        fontSize: "20px",
        fontFamily: "monospace",
        color: "#00ffff",
      }
    );
    subtitle.setOrigin(0.5);
    subtitle.setScrollFactor(0);

    // Initiate button
    const initiateButton = this.createButton(
      width / 2 - 100,
      height / 2 + 20,
      "[ INITIATE ATTACK ]",
      () => {
        if (this.gameState) {
          const success = launchAttack(
            this.gameState,
            this.gameState.currentPlayerId
          );
          if (success) {
            console.log("💥 DDoS Attack successfully launched!");
            this.showVictoryScreen();
          }
        }
        overlay.destroy();
        warningText.destroy();
        subtitle.destroy();
        initiateButton.destroy();
        cancelButton.destroy();
      }
    );

    // Cancel button
    const cancelButton = this.createButton(
      width / 2 - 80,
      height / 2 + 80,
      "[ CANCEL ]",
      () => {
        overlay.destroy();
        warningText.destroy();
        subtitle.destroy();
        initiateButton.destroy();
        cancelButton.destroy();
      }
    );

    // Pulse effect
    this.tweens.add({
      targets: warningText,
      alpha: 0.5,
      duration: 500,
      yoyo: true,
      repeat: -1,
    });
  }

  update() {
    // Any per-frame UI updates
  }
}
