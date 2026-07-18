// Health Check Chain
// Monitors MCP tools and system components

export interface HealthCheckResult {
  name: string;
  status: 'healthy' | 'degraded' | 'unhealthy';
  latencyMs: number;
  message: string;
  lastChecked: string;
}

export interface HealthCheckConfig {
  name: string;
  check: () => Promise<boolean>;
  timeoutMs?: number;
}

export class HealthCheckChain {
  private checks: HealthCheckConfig[] = [];
  private results: Map<string, HealthCheckResult> = new Map();
  private maxConcurrentChecks = 10;

  addCheck(config: HealthCheckConfig): void {
    this.checks.push(config);
  }

  async runAll(): Promise<HealthCheckResult[]> {
    // Limit concurrent checks
    const checksToRun = this.checks.slice(0, this.maxConcurrentChecks);
    
    const results = await Promise.all(
      checksToRun.map(async (check) => {
        const result = await this.runSingle(check);
        this.results.set(check.name, result);
        return result;
      })
    );
    return results;
  }

  private async runSingle(check: HealthCheckConfig): Promise<HealthCheckResult> {
    const startTime = Date.now();
    const timeoutMs = check.timeoutMs || 5000;

    return new Promise<HealthCheckResult>((resolve) => {
      let resolved = false;
      const timeoutId = setTimeout(() => {
        if (!resolved) {
          resolved = true;
          resolve({
            name: check.name,
            status: 'unhealthy',
            latencyMs: Date.now() - startTime,
            message: `Timeout after ${timeoutMs}ms`,
            lastChecked: new Date().toISOString(),
          });
        }
      }, timeoutMs);

      check.check()
        .then(result => {
          if (!resolved) {
            resolved = true;
            clearTimeout(timeoutId);
            resolve({
              name: check.name,
              status: result ? 'healthy' : 'degraded',
              latencyMs: Date.now() - startTime,
              message: result ? 'OK' : 'Check returned false',
              lastChecked: new Date().toISOString(),
            });
          }
        })
        .catch(error => {
          if (!resolved) {
            resolved = true;
            clearTimeout(timeoutId);
            resolve({
              name: check.name,
              status: 'unhealthy',
              latencyMs: Date.now() - startTime,
              message: (error as Error).message,
              lastChecked: new Date().toISOString(),
            });
          }
        });
    });
  }

  async checkTool(toolName: string): Promise<boolean> {
    const check = this.checks.find(c => c.name === toolName);
    if (!check) return false;
    
    const result = await this.runSingle(check);
    return result.status === 'healthy';
  }

  getOverallHealth(): 'healthy' | 'degraded' | 'unhealthy' | 'unknown' {
    const results = Array.from(this.results.values());
    if (results.length === 0) return 'unknown';
    if (results.every(r => r.status === 'healthy')) return 'healthy';
    if (results.some(r => r.status === 'unhealthy')) return 'unhealthy';
    return 'degraded';
  }

  getLastResults(): HealthCheckResult[] {
    return Array.from(this.results.values());
  }

  clearResults(): void {
    this.results.clear();
  }
}
