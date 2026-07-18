// Telemetry & Observability
// Structured logging, metrics, and tracing

import { randomUUID } from 'crypto';

export interface TelemetryEvent {
  name: string;
  attributes: Record<string, string | number | boolean>;
  timestamp: string;
  traceId?: string;
  spanId?: string;
}

export interface Metric {
  name: string;
  value: number;
  unit: string;
  tags: Record<string, string>;
  timestamp: string;
}

export class Telemetry {
  private events: TelemetryEvent[] = [];
  private metrics: Metric[] = [];
  private traceId: string;

  constructor() {
    this.traceId = randomUUID();
  }

  // Span management
  startSpan(name: string): Span {
    const spanId = randomUUID();
    return new Span(name, spanId, this.traceId, this);
  }

  // Event recording
  recordEvent(name: string, attributes: Record<string, string | number | boolean> = {}): void {
    this.events.push({
      name,
      attributes,
      timestamp: new Date().toISOString(),
      traceId: this.traceId,
    });
  }

  // Metric recording
  recordMetric(name: string, value: number, unit: string, tags: Record<string, string> = {}): void {
    this.metrics.push({
      name,
      value,
      unit,
      tags,
      timestamp: new Date().toISOString(),
    });
  }

  // Counter increment
  incrementCounter(name: string, tags: Record<string, string> = {}): void {
    const existing = this.metrics.find(m => m.name === name && JSON.stringify(m.tags) === JSON.stringify(tags));
    if (existing) {
      existing.value++;
    } else {
      this.recordMetric(name, 1, 'count', tags);
    }
  }

  // Get all data
  getEvents(): TelemetryEvent[] {
    return [...this.events];
  }

  getMetrics(): Metric[] {
    return [...this.metrics];
  }

  // Export for storage
  export(): { events: TelemetryEvent[]; metrics: Metric[] } {
    return {
      events: [...this.events],
      metrics: [...this.metrics],
    };
  }

  // Summary
  getSummary(): {
    totalEvents: number;
    totalMetrics: number;
    phaseMetrics: Record<string, number>;
  } {
    const phaseMetrics: Record<string, number> = {};
    
    for (const metric of this.metrics) {
      if (metric.name === 'phase_duration') {
        const phase = metric.tags.phase || 'unknown';
        phaseMetrics[phase] = (phaseMetrics[phase] || 0) + metric.value;
      }
    }

    return {
      totalEvents: this.events.length,
      totalMetrics: this.metrics.length,
      phaseMetrics,
    };
  }
}

export class Span {
  private name: string;
  private spanId: string;
  private traceId: string;
  private telemetry: Telemetry;
  private startTime: number;
  private attributes: Record<string, string | number | boolean> = {};
  private status: 'OK' | 'ERROR' = 'OK';
  private error?: Error;

  constructor(name: string, spanId: string, traceId: string, telemetry: Telemetry) {
    this.name = name;
    this.spanId = spanId;
    this.traceId = traceId;
    this.telemetry = telemetry;
    this.startTime = Date.now();
  }

  setAttribute(key: string, value: string | number | boolean): void {
    this.attributes[key] = value;
  }

  setStatus(status: 'OK' | 'ERROR', error?: Error): void {
    this.status = status;
    this.error = error;
  }

  recordException(error: Error): void {
    this.error = error;
    this.status = 'ERROR';
  }

  end(): void {
    const duration = Date.now() - this.startTime;
    this.attributes['duration_ms'] = duration;
    this.attributes['status'] = this.status;
    
    if (this.error) {
      this.attributes['error.message'] = this.error.message;
    }

    this.telemetry.recordEvent(`span.${this.name}`, this.attributes);
    this.telemetry.recordMetric(`${this.name}.duration`, duration, 'ms', {
      status: this.status,
    });
  }
}
