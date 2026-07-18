// Idempotency Manager
// Ensures operations are safely retryable

import { createHash, randomUUID } from 'crypto';

export interface IdempotentOperation {
  id: string;
  key: string;
  timestamp: string;
  result?: unknown;
  status: 'pending' | 'completed' | 'failed';
}

export class IdempotencyManager {
  private operations: Map<string, IdempotentOperation> = new Map();
  private pendingTimeoutMs: number;

  constructor(pendingTimeoutMs: number = 60000) {
    this.pendingTimeoutMs = pendingTimeoutMs;
  }

  generateKey(...args: unknown[]): string {
    const content = JSON.stringify(args);
    return createHash('sha256')
      .update(content)
      .digest('hex')
      .substring(0, 16);
  }

  async execute<T>(
    key: string,
    operation: () => Promise<T>
  ): Promise<{ result: T; fromCache: boolean }> {
    const existing = this.operations.get(key);
    
    if (existing?.status === 'completed') {
      console.log(`[Idempotency] Cache hit for key: ${key}`);
      return { result: existing.result as T, fromCache: true };
    }

    if (existing?.status === 'pending') {
      const pendingTime = new Date(existing.timestamp).getTime();
      if (Date.now() - pendingTime > this.pendingTimeoutMs) {
        console.log(`[Idempotency] Pending operation timed out, allowing retry: ${key}`);
      } else {
        console.log(`[Idempotency] Operation in progress: ${key}`);
        throw new Error(`Operation ${key} is already in progress`);
      }
    }

    const op: IdempotentOperation = {
      id: randomUUID(),
      key,
      timestamp: new Date().toISOString(),
      status: 'pending',
    };
    this.operations.set(key, op);

    try {
      const result = await operation();
      op.status = 'completed';
      op.result = result;
      return { result, fromCache: false };
    } catch (error) {
      op.status = 'failed';
      throw error;
    }
  }

  isCompleted(key: string): boolean {
    return this.operations.get(key)?.status === 'completed';
  }

  getResult<T>(key: string): T | undefined {
    const op = this.operations.get(key);
    return op?.status === 'completed' ? (op.result as T) : undefined;
  }

  clear(): void {
    this.operations.clear();
  }

  getStats(): { total: number; completed: number; pending: number; failed: number } {
    const ops = Array.from(this.operations.values());
    return {
      total: ops.length,
      completed: ops.filter(o => o.status === 'completed').length,
      pending: ops.filter(o => o.status === 'pending').length,
      failed: ops.filter(o => o.status === 'failed').length,
    };
  }
}
