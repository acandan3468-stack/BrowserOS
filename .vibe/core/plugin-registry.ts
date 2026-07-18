// Plugin Registry
// Hot-reloadable plugin system for rules and hooks

import { readFileSync, readdirSync, existsSync, watchFile } from 'fs';
import { join, extname } from 'path';
import { parse as parseYaml } from 'yaml';

export interface Rule {
  id: string;
  name: string;
  pattern?: string;
  action: 'block' | 'warn' | 'info';
  message: string;
  severity: 'critical' | 'high' | 'medium' | 'low';
  category: string;
}

export interface Hook {
  phase: string;
  action: string;
  priority: number;
  timing: 'pre' | 'post';
}

export interface Plugin {
  name: string;
  version: string;
  description: string;
  author: string;
  rules: Rule[];
  hooks: Hook[];
  config: Record<string, unknown>;
  enabled: boolean;
}

export class PluginRegistry {
  private plugins: Map<string, Plugin> = new Map();
  private pluginsDir: string;

  constructor(pluginsDir: string) {
    this.pluginsDir = pluginsDir;
    this.loadPlugins();
  }

  private loadPlugins(): void {
    const coreDir = join(this.pluginsDir, 'core');
    const customDir = join(this.pluginsDir, 'custom');

    if (existsSync(coreDir)) this.loadFromDir(coreDir);
    if (existsSync(customDir)) this.loadFromDir(customDir);
  }

  private loadFromDir(dir: string): void {
    const entries = readdirSync(dir, { withFileTypes: true });
    
    for (const entry of entries) {
      if (entry.isDirectory()) {
        const pluginDir = join(dir, entry.name);
        const manifestPath = join(pluginDir, 'plugin.yaml');
        
        if (existsSync(manifestPath)) {
          try {
            const content = readFileSync(manifestPath, 'utf-8');
            const manifest = parseYaml(content);
            const rules = this.loadRules(pluginDir);
            const hooks = this.loadHooks(pluginDir);
            
            const plugin: Plugin = {
              name: manifest.name || entry.name,
              version: manifest.version || '1.0.0',
              description: manifest.description || '',
              author: manifest.author || 'unknown',
              rules,
              hooks,
              config: manifest.config || {},
              enabled: manifest.enabled !== false,
            };
            
            this.plugins.set(plugin.name, plugin);
            console.log(`[PluginRegistry] Loaded: ${plugin.name} v${plugin.version} (${rules.length} rules, ${hooks.length} hooks)`);
          } catch (err) {
            console.error(`[PluginRegistry] Failed to load ${entry.name}:`, err);
          }
        }
      }
    }
  }

  private loadRules(pluginDir: string): Rule[] {
    const rulesFile = join(pluginDir, 'rules.yaml');
    if (!existsSync(rulesFile)) return [];
    
    try {
      const content = readFileSync(rulesFile, 'utf-8');
      const data = parseYaml(content);
      return data.rules || [];
    } catch {
      return [];
    }
  }

  private loadHooks(pluginDir: string): Hook[] {
    const hooksFile = join(pluginDir, 'hooks.yaml');
    if (!existsSync(hooksFile)) return [];
    
    try {
      const content = readFileSync(hooksFile, 'utf-8');
      const data = parseYaml(content);
      return data.hooks || [];
    } catch {
      return [];
    }
  }

  getPlugin(name: string): Plugin | undefined {
    return this.plugins.get(name);
  }

  getAllPlugins(): Plugin[] {
    return Array.from(this.plugins.values());
  }

  getEnabledPlugins(): Plugin[] {
    return this.getAllPlugins().filter(p => p.enabled);
  }

  getAllRules(): Rule[] {
    return this.getEnabledPlugins().flatMap(p => p.rules);
  }

  getHooksForPhase(phase: string, timing: 'pre' | 'post'): Hook[] {
    return this.getEnabledPlugins()
      .flatMap(p => p.hooks)
      .filter(h => h.phase === phase && h.timing === timing)
      .sort((a, b) => a.priority - b.priority);
  }

  validateRule(code: string): Array<{ rule: Rule; matched: boolean; line?: number }> {
    const rules = this.getAllRules();
    const results: Array<{ rule: Rule; matched: boolean; line?: number }> = [];
    
    for (const rule of rules) {
      if (rule.pattern) {
        try {
          const regex = new RegExp(rule.pattern, 'gm');
          const match = regex.exec(code);
          results.push({
            rule,
            matched: match !== null,
            line: match ? this.getLineNumber(code, match.index) : undefined,
          });
        } catch {
          results.push({ rule, matched: false });
        }
      }
    }
    
    return results;
  }

  private getLineNumber(code: string, index: number): number {
    return code.substring(0, index).split('\n').length;
  }

  enablePlugin(name: string): void {
    const plugin = this.plugins.get(name);
    if (plugin) plugin.enabled = true;
  }

  disablePlugin(name: string): void {
    const plugin = this.plugins.get(name);
    if (plugin) plugin.enabled = false;
  }
}
