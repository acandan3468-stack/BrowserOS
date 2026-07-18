// Team Configuration & RBAC
// Multi-tenant, role-based access control

import { readFileSync, writeFileSync, existsSync } from 'fs';
import { join } from 'path';

export type Role = 'junior' | 'senior' | 'lead' | 'admin';

export interface TeamMember {
  id: string;
  name: string;
  role: Role;
  teams: string[];
  permissions: string[];
}

export interface TeamConfig {
  version: string;
  teams: Record<string, {
    plugins: string[];
    overrides: Record<string, unknown>;
  }>;
  roles: Record<Role, {
    enforce: 'strict' | 'warn' | 'off';
    suggest: boolean;
    permissions: string[];
  }>;
  governance: {
    approvers: string[];
    requiredReviews: number;
    compliance: string[];
  };
}

const BLOCKED_KEYS = ['__proto__', 'constructor', 'prototype'];

export class TeamManager {
  private config: TeamConfig;
  private members: TeamMember[] = [];
  private configPath: string;

  constructor(rootDir: string) {
    this.configPath = join(rootDir, '.vibe', 'team.json');
    this.config = this.loadConfig();
  }

  private loadConfig(): TeamConfig {
    if (existsSync(this.configPath)) {
      try {
        const content = readFileSync(this.configPath, 'utf-8');
        const parsed = JSON.parse(content);
        return this.sanitizeConfig(parsed);
      } catch (e) {
        console.warn(`[TeamConfig] Failed to parse config: ${(e as Error).message}, using defaults`);
      }
    }

    const defaultConfig: TeamConfig = {
      version: '2.0',
      teams: {
        default: {
          plugins: ['@vibe/core'],
          overrides: {},
        },
      },
      roles: {
        junior: {
          enforce: 'strict',
          suggest: true,
          permissions: ['read', 'suggest'],
        },
        senior: {
          enforce: 'warn',
          suggest: true,
          permissions: ['read', 'write', 'suggest', 'approve'],
        },
        lead: {
          enforce: 'off',
          suggest: true,
          permissions: ['read', 'write', 'suggest', 'approve', 'config'],
        },
        admin: {
          enforce: 'off',
          suggest: true,
          permissions: ['all'],
        },
      },
      governance: {
        approvers: [],
        requiredReviews: 1,
        compliance: [],
      },
    };

    writeFileSync(this.configPath, JSON.stringify(defaultConfig, null, 2));
    return defaultConfig;
  }

  private sanitizeConfig(obj: unknown): unknown {
    if (obj === null || typeof obj !== 'object') {
      return obj;
    }

    if (Array.isArray(obj)) {
      return obj.map(item => this.sanitizeConfig(item));
    }

    const sanitized: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
      if (BLOCKED_KEYS.includes(key)) {
        console.warn(`[TeamConfig] Blocked potentially dangerous key: ${key}`);
        continue;
      }
      sanitized[key] = this.sanitizeConfig(value);
    }
    return sanitized;
  }

  getTeamConfig(teamName: string): TeamConfig['teams'][string] | undefined {
    return this.config.teams[teamName];
  }

  getRoleConfig(role: Role): TeamConfig['roles'][role] | undefined {
    return this.config.roles[role];
  }

  hasPermission(role: Role, permission: string): boolean {
    const roleConfig = this.config.roles[role];
    if (!roleConfig) return false;
    if (roleConfig.permissions.includes('all')) return true;
    return roleConfig.permissions.includes(permission);
  }

  canApprove(role: Role): boolean {
    return this.hasPermission(role, 'approve');
  }

  canDeploy(role: Role): boolean {
    return this.hasPermission(role, 'deploy');
  }

  canEditConfig(role: Role): boolean {
    return this.hasPermission(role, 'config');
  }

  canWriteEvent(role: Role): boolean {
    return this.hasPermission(role, 'write');
  }

  getEnforcementLevel(role: Role): 'strict' | 'warn' | 'off' {
    return this.config.roles[role]?.enforce || 'warn';
  }

  validateRoleAction(role: Role, action: string): { allowed: boolean; reason: string } {
    const roleConfig = this.config.roles[role];
    if (!roleConfig) {
      return { allowed: false, reason: `Unknown role: ${role}` };
    }

    if (roleConfig.permissions.includes('all')) {
      return { allowed: true, reason: 'Admin has all permissions' };
    }

    if (roleConfig.permissions.includes(action)) {
      return { allowed: true, reason: `Role '${role}' has permission '${action}'` };
    }

    return {
      allowed: false,
      reason: `Role '${role}' does not have permission '${action}'`,
    };
  }

  addMember(member: Omit<TeamMember, 'id'>): TeamMember {
    const newMember: TeamMember = {
      ...member,
      id: `member-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`,
    };
    this.members.push(newMember);
    return newMember;
  }

  getMember(id: string): TeamMember | undefined {
    return this.members.find(m => m.id === id);
  }

  getMembersByRole(role: Role): TeamMember[] {
    return this.members.filter(m => m.role === role);
  }

  getMembersByTeam(team: string): TeamMember[] {
    return this.members.filter(m => m.teams.includes(team));
  }

  updateConfig(updates: Partial<TeamConfig>, role: Role): { success: boolean; reason: string } {
    // RBAC check
    if (!this.canEditConfig(role)) {
      return {
        success: false,
        reason: `Role '${role}' does not have permission to edit config`,
      };
    }

    const sanitizedUpdates = this.sanitizeConfig(updates) as Partial<TeamConfig>;
    this.config = this.deepMerge(this.config, sanitizedUpdates);
    writeFileSync(this.configPath, JSON.stringify(this.config, null, 2));
    return { success: true, reason: 'Config updated successfully' };
  }

  private deepMerge<T>(target: T, source: Partial<T>): T {
    const result = { ...target };
    for (const key in source) {
      if (BLOCKED_KEYS.includes(key)) continue;
      
      if (source.hasOwnProperty(key)) {
        const sourceVal = source[key];
        const targetVal = (result as Record<string, unknown>)[key];
        if (sourceVal && typeof sourceVal === 'object' && !Array.isArray(sourceVal) &&
            targetVal && typeof targetVal === 'object' && !Array.isArray(targetVal)) {
          (result as Record<string, unknown>)[key] = this.deepMerge(targetVal, sourceVal);
        } else {
          (result as Record<string, unknown>)[key] = sourceVal;
        }
      }
    }
    return result;
  }

  exportConfig(): string {
    return JSON.stringify(this.config, null, 2);
  }
}
