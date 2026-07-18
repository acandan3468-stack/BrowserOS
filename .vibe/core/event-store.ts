// Event Store - Append-only state management
// Replaces mutable STATE.md with immutable event log

import { createHash } from 'crypto';
import { readFileSync, appendFileSync, existsSync, writeFileSync, renameSync, unlinkSync, mkdirSync } from 'fs';
import { join, dirname } from 'path';
import { v4 as uuidv4 } from 'uuid';

export interface PhaseEvent {
  id: string;
  phase: string;
  type: 'STARTED' | 'COMPLETED' | 'FAILED' | 'ROLLED_BACK' | 'SKIPPED';
  payload: Record<string, unknown>;
  timestamp: string;
  checksum: string;
  agent?: string;
  idempotencyKey: string;
  roleId?: string;
}

export interface WorkflowState {
  currentPhase: string;
  phaseHistory: PhaseEvent[];
  task: {
    title: string;
    description: string;
    priority: 'low' | 'medium' | 'high' | 'critical';
  };
  decisions: Array<{ date: string; decision: string; reason: string }>;
  openQuestions: Array<{ question: string; status: 'wait' | 'answered' }>;
  scope: {
    inScope: string[];
    outScope: string[];
    decidedLater: string[];
  };
  blockerLog: Array<{ date: string; blocker: string; resolution: string }>;
  health: {
    lastUpdated: string;
    phaseDurationMinutes: number;
    openBlockers: number;
  };
}

export interface RBACConfig {
  requiredRole?: string;
  requiredPermission?: string;
}

export class EventStore {
  private eventsFile: string;
  private derivedStateFile: string;
  private events: PhaseEvent[] = [];
  private writeLock = false;
  private maxFileSize = 10 * 1024 * 1024; // 10MB limit

  constructor(stateDir: string) {
    if (!existsSync(stateDir)) {
      mkdirSync(stateDir, { recursive: true });
    }
    this.eventsFile = join(stateDir, 'events.jsonl');
    this.derivedStateFile = join(stateDir, 'derived-state.json');
    this.loadEvents();
  }

  private loadEvents(): void {
    if (!existsSync(this.eventsFile)) return;
    
    try {
      const content = readFileSync(this.eventsFile, 'utf-8');
      this.events = content
        .split('\n')
        .filter(line => line.trim())
        .map(line => {
          try {
            return JSON.parse(line) as PhaseEvent;
          } catch (e) {
            console.error(`[EventStore] Failed to parse event line: ${line.substring(0, 50)}...`);
            return null;
          }
        })
        .filter((e): e is PhaseEvent => e !== null);
    } catch (e) {
      console.error(`[EventStore] Failed to read events file: ${(e as Error).message}`);
      this.events = [];
    }
  }

  private calculateChecksum(event: Omit<PhaseEvent, 'checksum'>): string {
    const eventWithoutChecksum = { ...event, checksum: '' };
    return createHash('sha256')
      .update(JSON.stringify(eventWithoutChecksum))
      .digest('hex');
  }

  verifyChecksum(event: PhaseEvent): boolean {
    const { checksum, ...eventWithoutChecksum } = event;
    const expectedChecksum = createHash('sha256')
      .update(JSON.stringify(eventWithoutChecksum))
      .digest('hex');
    return checksum === expectedChecksum;
  }

  private acquireLock(): void {
    while (this.writeLock) {
      // Busy wait with small delay
      const start = Date.now();
      while (Date.now() - start < 10) {
        // Spin wait
      }
    }
    this.writeLock = true;
  }

  private releaseLock(): void {
    this.writeLock = false;
  }

  append(
    event: Omit<PhaseEvent, 'id' | 'checksum' | 'timestamp'>,
    rbacConfig?: RBACConfig,
    checkPermission?: (permission: string) => boolean
  ): PhaseEvent {
    // RBAC check
    if (rbacConfig?.requiredPermission && checkPermission) {
      if (!checkPermission(rbacConfig.requiredPermission)) {
        throw new Error(`RBAC: Missing required permission: ${rbacConfig.requiredPermission}`);
      }
    }

    this.acquireLock();
    
    try {
      // Idempotency check
      const existing = this.events.find(e => e.idempotencyKey === event.idempotencyKey);
      if (existing) {
        console.log(`[EventStore] Duplicate idempotency key detected: ${event.idempotencyKey}`);
        return existing;
      }

      // File size check
      if (existsSync(this.eventsFile)) {
        const stats = require('fs').statSync(this.eventsFile);
        if (stats.size > this.maxFileSize) {
          throw new Error(`Events file exceeds maximum size of ${this.maxFileSize} bytes`);
        }
      }

      const fullEvent: PhaseEvent = {
        ...event,
        id: uuidv4(),
        timestamp: new Date().toISOString(),
        checksum: '',
      };
      fullEvent.checksum = this.calculateChecksum(fullEvent);

      // Atomic write: write to temp file, then rename
      const tempFile = `${this.eventsFile}.tmp.${Date.now()}`;
      const eventLine = JSON.stringify(fullEvent) + '\n';
      
      try {
        if (existsSync(this.eventsFile)) {
          const existingContent = readFileSync(this.eventsFile, 'utf-8');
          writeFileSync(tempFile, existingContent + eventLine);
        } else {
          writeFileSync(tempFile, eventLine);
        }
        renameSync(tempFile, this.eventsFile);
      } catch (e) {
        // Cleanup temp file on error
        if (existsSync(tempFile)) {
          try { unlinkSync(tempFile); } catch {}
        }
        throw e;
      }

      this.events.push(fullEvent);
      this.updateDerivedState();
      
      return fullEvent;
    } finally {
      this.releaseLock();
    }
  }

  deriveState(): WorkflowState {
    const initialState: WorkflowState = {
      currentPhase: 'init',
      phaseHistory: [],
      task: { title: '', description: '', priority: 'medium' },
      decisions: [],
      openQuestions: [],
      scope: { inScope: [], outScope: [], decidedLater: [] },
      blockerLog: [],
      health: {
        lastUpdated: new Date().toISOString(),
        phaseDurationMinutes: 0,
        openBlockers: 0,
      },
    };

    return this.events.reduce((state, event) => {
      return this.applyEvent(state, event);
    }, initialState);
  }

  private applyEvent(state: WorkflowState, event: PhaseEvent): WorkflowState {
    // Deep clone to prevent mutable state issues
    const newState: WorkflowState = {
      ...state,
      phaseHistory: [...state.phaseHistory],
      task: { ...state.task },
      decisions: [...state.decisions],
      openQuestions: [...state.openQuestions],
      scope: {
        inScope: [...state.scope.inScope],
        outScope: [...state.scope.outScope],
        decidedLater: [...state.scope.decidedLater],
      },
      blockerLog: [...state.blockerLog],
      health: { ...state.health },
    };
    
    switch (event.type) {
      case 'STARTED':
        newState.currentPhase = event.phase;
        newState.health.lastUpdated = event.timestamp;
        break;
      case 'COMPLETED':
        newState.phaseHistory.push(event);
        break;
      case 'FAILED':
        newState.blockerLog.push({
          date: event.timestamp,
          blocker: `Phase ${event.phase} failed`,
          resolution: (event.payload.reason as string) || 'Pending',
        });
        newState.health.openBlockers++;
        break;
      case 'ROLLED_BACK':
        // Find the specific event to remove by its ID
        const originalEventId = event.payload.originalEventId as string;
        if (originalEventId) {
          newState.phaseHistory = newState.phaseHistory.filter(
            e => e.id !== originalEventId
          );
        } else {
          // Fallback: remove last event for this phase
          const lastIndex = newState.phaseHistory.findLastIndex(e => e.phase === event.phase);
          if (lastIndex !== -1) {
            newState.phaseHistory.splice(lastIndex, 1);
          }
        }
        break;
      case 'SKIPPED':
        // SKIPPED events are tracked but don't modify state
        break;
    }

    return newState;
  }

  private updateDerivedState(): void {
    const state = this.deriveState();
    const tempFile = `${this.derivedStateFile}.tmp.${Date.now()}`;
    
    try {
      writeFileSync(tempFile, JSON.stringify(state, null, 2));
      renameSync(tempFile, this.derivedStateFile);
    } catch (e) {
      if (existsSync(tempFile)) {
        try { unlinkSync(tempFile); } catch {}
      }
      console.error(`[EventStore] Failed to update derived state: ${(e as Error).message}`);
    }
  }

  getEvents(): PhaseEvent[] {
    return [...this.events];
  }

  getLastEvent(): PhaseEvent | undefined {
    return this.events[this.events.length - 1];
  }

  rollback(eventId: string, reason: string, roleId?: string): PhaseEvent {
    const event = this.events.find(e => e.id === eventId);
    if (!event) throw new Error(`Event not found: ${eventId}`);
    
    return this.append({
      phase: event.phase,
      type: 'ROLLED_BACK',
      payload: { originalEventId: eventId, reason },
      agent: 'system',
      idempotencyKey: `rollback-${eventId}-${uuidv4()}`,
      roleId,
    });
  }

  // Cleanup old events (for persistence management)
  pruneOldEvents(keepLastN: number): void {
    if (this.events.length <= keepLastN) return;
    
    const eventsToKeep = this.events.slice(-keepLastN);
    const tempFile = `${this.eventsFile}.tmp.${Date.now()}`;
    
    try {
      const content = eventsToKeep.map(e => JSON.stringify(e)).join('\n') + '\n';
      writeFileSync(tempFile, content);
      renameSync(tempFile, this.eventsFile);
      this.events = eventsToKeep;
      this.updateDerivedState();
    } catch (e) {
      if (existsSync(tempFile)) {
        try { unlinkSync(tempFile); } catch {}
      }
    }
  }
}
