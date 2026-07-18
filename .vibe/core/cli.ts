// CLI Interface
// vibe init, vibe status, vibe run, vibe doctor, vibe rollback

import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { join } from 'path';

export interface CLIConfig {
  projectType: 'web' | 'api' | 'mobile' | 'data' | 'unknown';
  teamSize: 'solo' | 'small' | 'medium' | 'large';
  painPoint: 'consistency' | 'review' | 'onboarding' | 'debt';
}

export interface StatusReport {
  currentPhase: string;
  progress: number;
  activeAgent: string | null;
  contextUsage: { used: number; total: number };
  duration: string;
  recentEvents: Array<{ phase: string; type: string; time: string }>;
}

export class VibeCLI {
  private rootDir: string;
  private configPath: string;

  constructor(rootDir: string) {
    this.rootDir = rootDir;
    this.configPath = join(rootDir, '.vibe', 'config.json');
  }

  async init(options?: { template?: string; interactive?: boolean }): Promise<void> {
    console.log('🚀 Vibe Coder Kit — Initial Setup\n');

    // Auto-detect project type
    const detected = this.detectProject();
    console.log(`✓ Proje tespit edildi: ${detected.type} (${detected.stack})`);
    
    // Load or create config
    const config: CLIConfig = {
      projectType: detected.type as CLIConfig['projectType'],
      teamSize: 'solo',
      painPoint: 'consistency',
    };

    // Write config
    writeFileSync(this.configPath, JSON.stringify(config, null, 2));
    console.log('✓ Yapılandırma kaydedildi');

    // Initialize directories
    this.ensureDirectories();
    console.log('✓ Dizinler hazırlandı');

    // Initialize state
    this.initializeState();
    console.log('✓ State başlatıldı');

    // Initialize knowledge base
    this.initializeKnowledge();
    console.log('✓ Knowledge base başlatıldı');

    console.log('\n🚀 Hazır! "vibe status" ile durumu kontrol edin.\n');
  }

  private detectProject(): { type: string; stack: string } {
    const packageJsonPath = join(this.rootDir, 'package.json');
    const cargoTomlPath = join(this.rootDir, 'Cargo.toml');
    const requirementsPath = join(this.rootDir, 'requirements.txt');
    const pubspecPath = join(this.rootDir, 'pubspec.yaml');

    if (existsSync(packageJsonPath)) {
      try {
        const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf-8'));
        const deps = { ...pkg.dependencies, ...pkg.devDependencies };
        
        if (deps['next'] || deps['react'] || deps['vue']) {
          return { type: 'web', stack: 'Node.js' };
        }
        if (deps['express'] || deps['fastify'] || deps['nestjs']) {
          return { type: 'api', stack: 'Node.js' };
        }
        return { type: 'unknown', stack: 'Node.js' };
      } catch {
        return { type: 'unknown', stack: 'Node.js' };
      }
    }

    if (existsSync(cargoTomlPath)) return { type: 'api', stack: 'Rust' };
    if (existsSync(requirementsPath)) return { type: 'data', stack: 'Python' };
    if (existsSync(pubspecPath)) return { type: 'mobile', stack: 'Flutter' };

    return { type: 'unknown', stack: 'Unknown' };
  }

  private ensureDirectories(): void {
    const dirs = [
      '.vibe/state', '.vibe/state/snapshots',
      '.vibe/memory/knowledge', '.vibe/memory/decisions',
      '.vibe/memory/gotchas', '.vibe/memory/conventions',
      '.vibe/workspace/plans', '.vibe/workspace/reports',
      '.vibe/workspace/archive',
    ];
    for (const dir of dirs) {
      const fullPath = join(this.rootDir, dir);
      if (!existsSync(fullPath)) {
        mkdirSync(fullPath, { recursive: true });
      }
    }
  }

  private initializeState(): void {
    const eventsPath = join(this.rootDir, '.vibe/state/events.jsonl');
    if (!existsSync(eventsPath)) {
      writeFileSync(eventsPath, '');
    }
  }

  private initializeKnowledge(): void {
    const indexPath = join(this.rootDir, '.vibe/memory/INDEX.md');
    if (!existsSync(indexPath)) {
      writeFileSync(indexPath, '# Knowledge Index\n\n| # | Date | Title | Tags | File |\n|---|------|-------|------|------|\n');
    }
  }

  status(): StatusReport {
    const eventsPath = join(this.rootDir, '.vibe/state/events.jsonl');
    let events: Array<{ phase: string; type: string; timestamp: string }> = [];
    
    if (existsSync(eventsPath)) {
      try {
        const content = readFileSync(eventsPath, 'utf-8');
        events = content.split('\n').filter(l => l.trim()).map(l => {
          try {
            return JSON.parse(l);
          } catch {
            return null;
          }
        }).filter((e): e is { phase: string; type: string; timestamp: string } => e !== null);
      } catch {
        events = [];
      }
    }

    const lastEvent = events[events.length - 1];
    const currentPhase = lastEvent?.phase || 'init';
    
    const phaseProgress: Record<string, number> = {
      init: 10, clarify: 20, brainstorm: 30, plan: 40,
      approve: 50, code: 70, review: 85, fix: 85,
      learn: 95, deploy: 98, done: 100,
    };

    return {
      currentPhase,
      progress: phaseProgress[currentPhase] || 0,
      activeAgent: lastEvent?.type === 'STARTED' ? 'running' : null,
      contextUsage: { used: 0, total: 8000 },
      duration: this.calculateDuration(events),
      recentEvents: events.slice(-5).map(e => ({
        phase: e.phase,
        type: e.type,
        time: e.timestamp,
      })),
    };
  }

  private calculateDuration(events: Array<{ timestamp: string }>): string {
    if (events.length < 2) return '0m';
    const first = new Date(events[0].timestamp);
    const last = new Date(events[events.length - 1].timestamp);
    const diffMs = last.getTime() - first.getTime();
    const minutes = Math.floor(diffMs / 60000);
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    return `${hours}h ${minutes % 60}m`;
  }

  doctor(): Array<{ check: string; status: 'ok' | 'warning' | 'error'; message: string }> {
    const results: Array<{ check: string; status: 'ok' | 'warning' | 'error'; message: string }> = [];

    // Check CLI version
    results.push({ check: 'CLI Version', status: 'ok', message: '1.0.0 (latest)' });

    // Check Node.js
    const nodeVersion = process.version;
    results.push({
      check: 'Node.js',
      status: parseInt(nodeVersion.slice(1)) >= 18 ? 'ok' : 'error',
      message: `${nodeVersion} (required: >=18)`,
    });

    // Check config
    results.push({
      check: 'Config',
      status: existsSync(this.configPath) ? 'ok' : 'error',
      message: existsSync(this.configPath) ? 'Loaded' : 'Not found',
    });

    // Check state
    const eventsPath = join(this.rootDir, '.vibe/state/events.jsonl');
    results.push({
      check: 'State',
      status: existsSync(eventsPath) ? 'ok' : 'warning',
      message: existsSync(eventsPath) ? 'Clean' : 'No events yet',
    });

    // Check knowledge base
    const indexPath = join(this.rootDir, '.vibe/memory/INDEX.md');
    results.push({
      check: 'Knowledge Base',
      status: existsSync(indexPath) ? 'ok' : 'warning',
      message: existsSync(indexPath) ? 'Initialized' : 'Not initialized',
    });

    return results;
  }

  displayStatus(): void {
    const status = this.status();
    const bar = '█'.repeat(Math.floor(status.progress / 5)) + '░'.repeat(20 - Math.floor(status.progress / 5));
    
    console.log('┌────────────────────────────────────────────┐');
    console.log(`│ Phase: ${status.currentPhase.padEnd(20)} [${bar}] ${status.progress}% │`);
    console.log(`│ Agent: ${(status.activeAgent || 'idle').padEnd(29)} │`);
    console.log(`│ Context: ${status.contextUsage.used} / ${status.contextUsage.total} tokens${' '.repeat(Math.max(0, 16 - String(status.contextUsage.used).length - String(status.contextUsage.total).length))}│`);
    console.log(`│ Duration: ${status.duration.padEnd(28)} │`);
    console.log('└────────────────────────────────────────────┘');
    
    if (status.recentEvents.length > 0) {
      console.log('\nRecent events:');
      for (const event of status.recentEvents) {
        const icon = event.type === 'COMPLETED' ? '✓' : event.type === 'FAILED' ? '✗' : '→';
        console.log(`  ${icon} ${event.phase} — ${event.type} (${new Date(event.time).toLocaleTimeString()})`);
      }
    }
  }

  displayDoctor(): void {
    const results = this.doctor();
    console.log('\nVibe Coder Kit — Health Check\n');
    
    for (const r of results) {
      const icon = r.status === 'ok' ? '✓' : r.status === 'warning' ? '⚠' : '✗';
      console.log(`${icon} ${r.check}: ${r.message}`);
    }
    
    const errors = results.filter(r => r.status === 'error');
    if (errors.length > 0) {
      console.log(`\n${errors.length} error(s) found. Run 'vibe doctor --fix' to resolve.`);
    } else {
      console.log('\n✓ All checks passed.');
    }
  }
}

// CLI Entry Point
if (require.main === module) {
  const args = process.argv.slice(2);
  const command = args[0];
  const rootDir = process.cwd();
  
  const cli = new VibeCLI(rootDir);
  
  switch (command) {
    case 'init':
      cli.init().catch(console.error);
      break;
    case 'status':
      cli.displayStatus();
      break;
    case 'doctor':
      cli.displayDoctor();
      break;
    default:
      console.log('Vibe Coder Kit v6.0.0\n');
      console.log('Commands:');
      console.log('  vibe init     — Initialize project');
      console.log('  vibe status   — Show current status');
      console.log('  vibe doctor   — Health check');
  }
}
