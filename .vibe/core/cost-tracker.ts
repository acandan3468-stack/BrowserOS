// Cost Tracker
// Tracks token usage and estimated costs

import { randomUUID } from 'crypto';

export interface TokenUsage {
  input: number;
  output: number;
  total: number;
}

export interface CostEstimate {
  model: string;
  inputCost: number;
  outputCost: number;
  totalCost: number;
  currency: string;
}

export interface SessionCost {
  sessionId: string;
  startTime: string;
  usage: TokenUsage;
  estimatedCost: CostEstimate;
  phaseBreakdown: Record<string, TokenUsage>;
  modelUsage: Record<string, TokenUsage>;
}

export class CostTracker {
  private sessions: Map<string, SessionCost> = new Map();
  private currentSessionId: string;
  private costPerToken: Record<string, { input: number; output: number }> = {
    'claude-sonnet': { input: 0.003 / 1000, output: 0.015 / 1000 },
    'claude-haiku': { input: 0.00025 / 1000, output: 0.00125 / 1000 },
    'gpt-4': { input: 0.03 / 1000, output: 0.06 / 1000 },
    'gpt-4o': { input: 0.005 / 1000, output: 0.015 / 1000 },
  };

  constructor() {
    this.currentSessionId = randomUUID();
    this.sessions.set(this.currentSessionId, {
      sessionId: this.currentSessionId,
      startTime: new Date().toISOString(),
      usage: { input: 0, output: 0, total: 0 },
      estimatedCost: { model: 'unknown', inputCost: 0, outputCost: 0, totalCost: 0, currency: 'USD' },
      phaseBreakdown: {},
      modelUsage: {},
    });
  }

  recordUsage(model: string, inputTokens: number, outputTokens: number, phase?: string): void {
    const session = this.sessions.get(this.currentSessionId);
    if (!session) return;

    session.usage.input += inputTokens;
    session.usage.output += outputTokens;
    session.usage.total += inputTokens + outputTokens;

    if (phase) {
      if (!session.phaseBreakdown[phase]) {
        session.phaseBreakdown[phase] = { input: 0, output: 0, total: 0 };
      }
      session.phaseBreakdown[phase].input += inputTokens;
      session.phaseBreakdown[phase].output += outputTokens;
      session.phaseBreakdown[phase].total += inputTokens + outputTokens;
    }

    if (!session.modelUsage[model]) {
      session.modelUsage[model] = { input: 0, output: 0, total: 0 };
    }
    session.modelUsage[model].input += inputTokens;
    session.modelUsage[model].output += outputTokens;
    session.modelUsage[model].total += inputTokens + outputTokens;

    const rates = this.costPerToken[model] || this.costPerToken['gpt-4o'];
    if (!this.costPerToken[model]) {
      console.warn(`[CostTracker] Unknown model '${model}', using gpt-4o rates`);
    }
    const inputCost = inputTokens * rates.input;
    const outputCost = outputTokens * rates.output;

    session.estimatedCost = {
      model,
      inputCost: session.estimatedCost.inputCost + inputCost,
      outputCost: session.estimatedCost.outputCost + outputCost,
      totalCost: session.estimatedCost.totalCost + inputCost + outputCost,
      currency: 'USD',
    };
  }

  getCurrentSession(): SessionCost | undefined {
    return this.sessions.get(this.currentSessionId);
  }

  getSessionCost(): CostEstimate | undefined {
    return this.sessions.get(this.currentSessionId)?.estimatedCost;
  }

  checkBudget(maxTokens: number): { within: boolean; used: number; remaining: number; percentage: number } {
    const session = this.sessions.get(this.currentSessionId);
    const used = session?.usage.total || 0;
    return {
      within: used <= maxTokens,
      used,
      remaining: Math.max(0, maxTokens - used),
      percentage: Math.min(100, (used / maxTokens) * 100),
    };
  }

  getSummary(): {
    totalSessions: number;
    totalTokens: number;
    totalCost: number;
    averagePerSession: number;
  } {
    const allSessions = Array.from(this.sessions.values());
    const totalTokens = allSessions.reduce((sum, s) => sum + s.usage.total, 0);
    const totalCost = allSessions.reduce((sum, s) => sum + s.estimatedCost.totalCost, 0);

    return {
      totalSessions: allSessions.length,
      totalTokens,
      totalCost,
      averagePerSession: allSessions.length > 0 ? totalCost / allSessions.length : 0,
    };
  }

  exportReport(): string {
    const session = this.sessions.get(this.currentSessionId);
    if (!session) return 'No active session';

    let report = `# Cost Report — Session ${this.currentSessionId}\n\n`;
    report += `**Started:** ${session.startTime}\n`;
    report += `**Model:** ${session.estimatedCost.model}\n\n`;
    report += `## Token Usage\n`;
    report += `| Type | Tokens |\n|------|--------|\n`;
    report += `| Input | ${session.usage.input.toLocaleString()} |\n`;
    report += `| Output | ${session.usage.output.toLocaleString()} |\n`;
    report += `| **Total** | **${session.usage.total.toLocaleString()}** |\n\n`;
    report += `## Estimated Cost\n`;
    report += `- Input: $${session.estimatedCost.inputCost.toFixed(4)}\n`;
    report += `- Output: $${session.estimatedCost.outputCost.toFixed(4)}\n`;
    report += `- **Total: $${session.estimatedCost.totalCost.toFixed(4)}**\n\n`;

    if (Object.keys(session.phaseBreakdown).length > 0) {
      report += `## Phase Breakdown\n`;
      report += `| Phase | Tokens | Est. Cost |\n|-------|--------|----------|\n`;
      for (const [phase, usage] of Object.entries(session.phaseBreakdown)) {
        const rates = this.costPerToken[session.estimatedCost.model] || this.costPerToken['gpt-4o'];
        const cost = (usage.input * rates.input) + (usage.output * rates.output);
        report += `| ${phase} | ${usage.total.toLocaleString()} | $${cost.toFixed(4)} |\n`;
      }
    }

    return report;
  }
}
