// Saga Pattern
// Compensating transactions for workflow rollback

export interface SagaStep<T = unknown> {
  name: string;
  execute: () => Promise<T>;
  compensate: (result: T) => Promise<void>;
}

export interface SagaConfig {
  maxRetries: number;
  retryDelayMs: number;
  maxRetryDelayMs: number;
  stepTimeoutMs: number;
}

export interface SagaResult {
  success: boolean;
  completedSteps: string[];
  failedCompensations: string[];
  error?: string;
}

export class Saga {
  private steps: SagaStep[] = [];
  private config: SagaConfig;

  constructor(config?: Partial<SagaConfig>) {
    this.config = {
      maxRetries: Math.min(config?.maxRetries ?? 3, 10), // Cap at 10
      retryDelayMs: config?.retryDelayMs ?? 1000,
      maxRetryDelayMs: config?.maxRetryDelayMs ?? 30000, // 30s max
      stepTimeoutMs: config?.stepTimeoutMs ?? 60000, // 60s per step
    };
  }

  addStep<T>(step: SagaStep<T>): void {
    this.steps.push(step as SagaStep);
  }

  async execute(): Promise<SagaResult> {
    const completedSteps: Array<{ name: string; result: unknown }> = [];

    for (const step of this.steps) {
      let retries = 0;
      let lastError: Error | undefined;

      while (retries < this.config.maxRetries) {
        try {
          console.log(`[Saga] Executing: ${step.name} (attempt ${retries + 1})`);
          
          // Add timeout to step execution
          const result = await this.executeWithTimeout(
            () => step.execute(),
            this.config.stepTimeoutMs
          );
          
          completedSteps.push({ name: step.name, result });
          break;
        } catch (error) {
          lastError = error as Error;
          retries++;
          
          if (retries < this.config.maxRetries) {
            const delayMs = Math.min(
              this.config.retryDelayMs * retries,
              this.config.maxRetryDelayMs
            );
            console.log(`[Saga] Retrying ${step.name} (${retries}/${this.config.maxRetries}) after ${delayMs}ms`);
            await this.delay(delayMs);
          }
        }
      }

      if (lastError && retries >= this.config.maxRetries) {
        console.log(`[Saga] Step ${step.name} failed after ${retries} retries`);
        const failedCompensations = await this.compensate([...completedSteps].reverse());
        return {
          success: false,
          completedSteps: completedSteps.map(s => s.name),
          failedCompensations,
          error: `Step ${step.name} failed: ${lastError.message}`,
        };
      }
    }

    return {
      success: true,
      completedSteps: completedSteps.map(s => s.name),
      failedCompensations: [],
    };
  }

  private async executeWithTimeout<T>(
    operation: () => Promise<T>,
    timeoutMs: number
  ): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        reject(new Error(`Operation timed out after ${timeoutMs}ms`));
      }, timeoutMs);

      operation()
        .then(result => {
          clearTimeout(timeoutId);
          resolve(result);
        })
        .catch(error => {
          clearTimeout(timeoutId);
          reject(error);
        });
    });
  }

  private async compensate(completedSteps: Array<{ name: string; result: unknown }>): Promise<string[]> {
    console.log('[Saga] Compensating completed steps...');
    const failedCompensations: string[] = [];
    
    for (const { name, result } of completedSteps) {
      const step = this.steps.find(s => s.name === name);
      if (step) {
        try {
          console.log(`[Saga] Compensating: ${name}`);
          await this.executeWithTimeout(
            () => step.compensate(result),
            this.config.stepTimeoutMs
          );
        } catch (error) {
          console.error(`[Saga] Compensation failed for ${name}:`, (error as Error).message);
          failedCompensations.push(name);
        }
      }
    }
    return failedCompensations;
  }

  private delay(ms: number): Promise<void> {
    const delayMs = Math.max(0, Math.min(ms, this.config.maxRetryDelayMs));
    return new Promise(resolve => setTimeout(resolve, delayMs));
  }
}
