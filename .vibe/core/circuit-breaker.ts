// Circuit Breaker Pattern
// Protects against cascading failures in agent operations

export type CircuitState = 'CLOSED' | 'OPEN' | 'HALF_OPEN';

export interface CircuitBreakerConfig {
  failureThreshold: number;
  resetTimeoutMs: number;
  halfOpenMaxAttempts: number;
}

export class CircuitBreaker {
  private state: CircuitState = 'CLOSED';
  private failures = 0;
  private lastFailureTime = 0;
  private halfOpenAttempts = 0;
  private config: CircuitBreakerConfig;

  constructor(config?: Partial<CircuitBreakerConfig>) {
    this.config = {
      failureThreshold: config?.failureThreshold ?? 3,
      resetTimeoutMs: config?.resetTimeoutMs ?? 60000,
      halfOpenMaxAttempts: config?.halfOpenMaxAttempts ?? 1,
    };
  }

  getState(): CircuitState {
    return this.state;
  }

  private checkTransition(): void {
    if (this.state === 'OPEN') {
      if (Date.now() - this.lastFailureTime >= this.config.resetTimeoutMs) {
        this.state = 'HALF_OPEN';
        this.halfOpenAttempts = 0;
      }
    }
  }

  async execute<T>(operation: () => Promise<T>): Promise<T> {
    this.checkTransition();
    const currentState = this.getState();
    
    if (currentState === 'OPEN') {
      throw new CircuitOpenError(
        `Circuit breaker is OPEN. Retry after ${this.config.resetTimeoutMs}ms`
      );
    }

    if (currentState === 'HALF_OPEN' && this.halfOpenAttempts >= this.config.halfOpenMaxAttempts) {
      throw new CircuitOpenError(
        `Circuit breaker is HALF_OPEN. Max attempts reached.`
      );
    }

    try {
      const result = await operation();
      this.onSuccess();
      return result;
    } catch (error) {
      this.onFailure();
      throw error;
    }
  }

  private onSuccess(): void {
    if (this.state === 'HALF_OPEN') {
      this.state = 'CLOSED';
      this.failures = 0;
      this.halfOpenAttempts = 0;
      console.log('[CircuitBreaker] State: HALF_OPEN -> CLOSED');
    }
  }

  private onFailure(): void {
    this.failures++;
    this.lastFailureTime = Date.now();

    if (this.state === 'HALF_OPEN') {
      this.halfOpenAttempts++;
      if (this.halfOpenAttempts >= this.config.halfOpenMaxAttempts) {
        this.state = 'OPEN';
        console.log('[CircuitBreaker] State: HALF_OPEN -> OPEN');
      }
    } else if (this.failures >= this.config.failureThreshold) {
      this.state = 'OPEN';
      console.log(`[CircuitBreaker] State: CLOSED -> OPEN (${this.failures} failures)`);
    }
  }

  reset(): void {
    this.state = 'CLOSED';
    this.failures = 0;
    this.halfOpenAttempts = 0;
  }

  getStats(): { state: CircuitState; failures: number; lastFailure: string | null } {
    return {
      state: this.getState(),
      failures: this.failures,
      lastFailure: this.lastFailureTime
        ? new Date(this.lastFailureTime).toISOString()
        : null,
    };
  }
}

export class CircuitOpenError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'CircuitOpenError';
  }
}
