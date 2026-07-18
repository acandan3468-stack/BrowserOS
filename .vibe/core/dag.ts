// DAG Workflow Engine
// Replaces linear 00-99 phase numbering with dependency graph

import { readFileSync, existsSync, writeFileSync } from 'fs';
import { join } from 'path';

export interface PhaseNode {
  id: string;
  name: string;
  depends: string[];
  next: string[];
  entryCriteria: string[];
  exitCriteria: string[];
  optional: boolean;
  parallelGroup?: string;
}

export interface PhaseGraph {
  phases: Record<string, PhaseNode>;
  parallelGroups: string[][];
}

export class DAGWorkflow {
  private graph: PhaseGraph;
  private graphFile: string;

  constructor(configDir: string) {
    this.graphFile = join(configDir, 'phase-graph.json');
    this.graph = this.loadGraph();
    
    // Validate graph on load
    const validation = this.validateGraph();
    if (!validation.valid) {
      console.error(`[DAG] Graph validation failed: ${validation.errors.join(', ')}`);
    }
  }

  private loadGraph(): PhaseGraph {
    if (!existsSync(this.graphFile)) {
      return this.createDefaultGraph();
    }
    
    try {
      const content = readFileSync(this.graphFile, 'utf-8');
      const graph = JSON.parse(content) as PhaseGraph;
      
      // Validate loaded graph
      const validation = this.validateGraphStructure(graph);
      if (!validation.valid) {
        console.error(`[DAG] Invalid graph structure: ${validation.errors.join(', ')}`);
        return this.createDefaultGraph();
      }
      
      return graph;
    } catch (e) {
      console.error(`[DAG] Failed to parse graph file: ${(e as Error).message}`);
      return this.createDefaultGraph();
    }
  }

  private validateGraphStructure(graph: PhaseGraph): { valid: boolean; errors: string[] } {
    const errors: string[] = [];
    
    // Check for missing phase references
    for (const [id, phase] of Object.entries(graph.phases)) {
      for (const dep of phase.depends) {
        if (!graph.phases[dep]) {
          errors.push(`Phase '${id}' depends on non-existent phase '${dep}'`);
        }
      }
      for (const next of phase.next) {
        if (!graph.phases[next]) {
          errors.push(`Phase '${id}' references non-existent next phase '${next}'`);
        }
      }
    }
    
    return { valid: errors.length === 0, errors };
  }

  private createDefaultGraph(): PhaseGraph {
    const defaultGraph: PhaseGraph = {
      phases: {
        init: {
          id: 'init',
          name: 'Project Initialization',
          depends: [],
          next: ['clarify'],
          entryCriteria: [],
          exitCriteria: ['context.md populated', 'git initialized'],
          optional: false,
        },
        clarify: {
          id: 'clarify',
          name: 'Requirements Clarification',
          depends: ['init'],
          next: ['brainstorm', 'plan'],
          entryCriteria: ['task defined'],
          exitCriteria: ['no open questions', 'scope defined'],
          optional: false,
        },
        brainstorm: {
          id: 'brainstorm',
          name: 'Research & Alternatives',
          depends: ['clarify'],
          next: ['plan'],
          entryCriteria: ['ambiguity exists'],
          exitCriteria: ['alternatives evaluated', 'approach selected'],
          optional: true,
          parallelGroup: 'planning',
        },
        plan: {
          id: 'plan',
          name: 'Planning',
          depends: ['clarify'],
          next: ['approve'],
          entryCriteria: ['scope clear', 'approach selected'],
          exitCriteria: ['task list created', 'test strategy defined'],
          optional: false,
          parallelGroup: 'planning',
        },
        approve: {
          id: 'approve',
          name: 'Architecture Review & Approval',
          depends: ['plan'],
          next: ['code'],
          entryCriteria: ['plan ready'],
          exitCriteria: ['architecture approved', 'user approved'],
          optional: false,
        },
        code: {
          id: 'code',
          name: 'Implementation',
          depends: ['approve'],
          next: ['review'],
          entryCriteria: ['approval received'],
          exitCriteria: ['all tasks completed', 'tests passing'],
          optional: false,
          parallelGroup: 'development',
        },
        review: {
          id: 'review',
          name: 'Code Review & QA',
          depends: ['code'],
          next: ['learn', 'deploy'],
          entryCriteria: ['code complete'],
          exitCriteria: ['review approved', 'no critical bugs'],
          optional: false,
        },
        fix: {
          id: 'fix',
          name: 'Bug Fixes',
          depends: ['review'],
          next: ['code'],
          entryCriteria: ['bugs found'],
          exitCriteria: ['all bugs fixed', 'tests passing'],
          optional: true,
          parallelGroup: 'development',
        },
        learn: {
          id: 'learn',
          name: 'Knowledge Capture',
          depends: ['review'],
          next: ['done'],
          entryCriteria: ['review complete'],
          exitCriteria: ['knowledge recorded', 'changelog updated'],
          optional: false,
        },
        deploy: {
          id: 'deploy',
          name: 'Deployment',
          depends: ['review'],
          next: ['learn'],
          entryCriteria: ['review approved', 'deploy needed'],
          exitCriteria: ['deploy successful', 'smoke test passed'],
          optional: true,
        },
        done: {
          id: 'done',
          name: 'Workflow Complete',
          depends: ['learn'],
          next: [],
          entryCriteria: ['knowledge captured'],
          exitCriteria: ['state reset'],
          optional: false,
        },
      },
      parallelGroups: [
        ['brainstorm', 'plan'],
        ['code', 'fix'],
      ],
    };
    
    writeFileSync(this.graphFile, JSON.stringify(defaultGraph, null, 2));
    return defaultGraph;
  }

  validateGraph(): { valid: boolean; errors: string[] } {
    const errors: string[] = [];
    const visited = new Set<string>();
    const recursionStack = new Set<string>();

    // Check for cycles using DFS
    const hasCycle = (nodeId: string): boolean => {
      visited.add(nodeId);
      recursionStack.add(nodeId);

      const node = this.graph.phases[nodeId];
      if (!node) return false;

      for (const nextId of node.next) {
        if (!visited.has(nextId)) {
          if (hasCycle(nextId)) return true;
        } else if (recursionStack.has(nextId)) {
          return true;
        }
      }

      recursionStack.delete(nodeId);
      return false;
    };

    // Check each phase for cycles
    for (const nodeId of Object.keys(this.graph.phases)) {
      if (!visited.has(nodeId)) {
        if (hasCycle(nodeId)) {
          errors.push(`Circular dependency detected involving phase '${nodeId}'`);
        }
      }
    }

    // Check for missing dependencies
    for (const [id, phase] of Object.entries(this.graph.phases)) {
      for (const dep of phase.depends) {
        if (!this.graph.phases[dep]) {
          errors.push(`Phase '${id}' depends on non-existent phase '${dep}'`);
        }
      }
    }

    return { valid: errors.length === 0, errors };
  }

  getPhase(id: string): PhaseNode | undefined {
    return this.graph.phases[id];
  }

  getValidNextPhases(currentPhase: string): PhaseNode[] {
    const current = this.graph.phases[currentPhase];
    if (!current) return [];
    return current.next
      .map(id => this.graph.phases[id])
      .filter(Boolean);
  }

  canTransition(from: string, to: string): boolean {
    const phase = this.graph.phases[from];
    if (!phase) return false;
    return phase.next.includes(to);
  }

  validateTransition(from: string, to: string): { valid: boolean; reason: string } {
    if (!this.canTransition(from, to)) {
      return {
        valid: false,
        reason: `No transition from ${from} to ${to}`,
      };
    }

    const targetPhase = this.graph.phases[to];
    if (!targetPhase) {
      return { valid: false, reason: `Phase ${to} not found` };
    }

    // Check if required dependencies are met
    const completedPhases = new Set<string>(); // This should be passed in from state
    for (const dep of targetPhase.depends) {
      if (!completedPhases.has(dep) && dep !== from) {
        // Allow if dependency is optional
        const depPhase = this.graph.phases[dep];
        if (depPhase && !depPhase.optional) {
          return { valid: false, reason: `Required dependency '${dep}' not completed` };
        }
      }
    }

    return { valid: true, reason: 'Transition allowed' };
  }

  getParallelGroup(phaseId: string): string | undefined {
    const phase = this.graph.phases[phaseId];
    return phase?.parallelGroup;
  }

  areParallel(phase1: string, phase2: string): boolean {
    const group1 = this.getParallelGroup(phase1);
    const group2 = this.getParallelGroup(phase2);
    return group1 !== undefined && group1 === group2;
  }

  getEntryCriteria(phaseId: string): string[] {
    return this.graph.phases[phaseId]?.entryCriteria || [];
  }

  getExitCriteria(phaseId: string): string[] {
    return this.graph.phases[phaseId]?.exitCriteria || [];
  }

  listPhases(): PhaseNode[] {
    return Object.values(this.graph.phases);
  }

  getOptionalPhases(): PhaseNode[] {
    return this.listPhases().filter(p => p.optional);
  }

  getRequiredPhases(): PhaseNode[] {
    return this.listPhases().filter(p => !p.optional);
  }
}
