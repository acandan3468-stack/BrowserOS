// Knowledge Store
// Semantic search and knowledge management

import { readFileSync, writeFileSync, existsSync, readdirSync } from 'fs';
import { join, resolve, normalize } from 'path';

export interface KnowledgeEntry {
  id: string;
  title: string;
  date: string;
  tags: string[];
  context: string;
  solution: string;
  phase?: string;
  relatedEntries?: string[];
  embedding?: number[];
}

export interface SearchResult {
  entry: KnowledgeEntry;
  score: number;
}

export class KnowledgeStore {
  private knowledgeDir: string;
  private indexPath: string;
  private entries: KnowledgeEntry[] = [];

  constructor(knowledgeDir: string) {
    // Normalize and validate path to prevent path traversal
    this.knowledgeDir = resolve(normalize(knowledgeDir));
    this.indexPath = join(this.knowledgeDir, 'INDEX.md');
    this.loadEntries();
  }

  private isPathSafe(filePath: string): boolean {
    const resolved = resolve(normalize(filePath));
    return resolved.startsWith(this.knowledgeDir);
  }

  private loadEntries(): void {
    if (!existsSync(this.knowledgeDir)) {
      console.warn(`[KnowledgeStore] Directory not found: ${this.knowledgeDir}`);
      return;
    }
    
    try {
      const files = readdirSync(this.knowledgeDir).filter(f => f.endsWith('.md') && f !== 'INDEX.md');
      
      for (const file of files) {
        const filePath = join(this.knowledgeDir, file);
        
        // Path traversal check
        if (!this.isPathSafe(filePath)) {
          console.warn(`[KnowledgeStore] Skipping potentially unsafe path: ${file}`);
          continue;
        }
        
        try {
          const content = readFileSync(filePath, 'utf-8');
          const entry = this.parseEntry(content, file);
          if (entry) this.entries.push(entry);
        } catch (e) {
          console.error(`[KnowledgeStore] Failed to read ${file}: ${(e as Error).message}`);
        }
      }
    } catch (e) {
      console.error(`[KnowledgeStore] Failed to load entries: ${(e as Error).message}`);
    }
  }

  private parseEntry(content: string, filename: string): KnowledgeEntry | null {
    const titleMatch = content.match(/^#\s+(.+)/m);
    const dateMatch = content.match(/\*\*Date:\*\*\s*(.+)/);
    const tagsMatch = content.match(/\*\*Tags:\*\*\s*(.+)/);
    const contextMatch = content.match(/## Context\n([\s\S]*?)(?=\n##|$)/);
    const solutionMatch = content.match(/## Solution\n([\s\S]*?)(?=\n##|$)/);
    const phaseMatch = content.match(/\*\*Phase:\*\*\s*(.+)/);

    if (!titleMatch) return null;

    return {
      id: filename.replace('.md', ''),
      title: titleMatch[1].trim(),
      date: dateMatch?.[1]?.trim() || new Date().toISOString().split('T')[0],
      tags: tagsMatch?.[1]?.split(',').map(t => t.trim()) || [],
      context: contextMatch?.[1]?.trim() || '',
      solution: solutionMatch?.[1]?.trim() || '',
      phase: phaseMatch?.[1]?.trim(),
    };
  }

  addEntry(entry: Omit<KnowledgeEntry, 'id'>): KnowledgeEntry {
    const id = `knowledge-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
    const newEntry: KnowledgeEntry = { ...entry, id };
    this.entries.push(newEntry);
    this.saveEntry(newEntry);
    this.updateIndex();
    return newEntry;
  }

  private saveEntry(entry: KnowledgeEntry): void {
    const filePath = join(this.knowledgeDir, `${entry.id}.md`);
    
    // Path traversal check
    if (!this.isPathSafe(filePath)) {
      throw new Error(`Invalid file path: ${entry.id}.md`);
    }
    
    const content = `# ${entry.title}

**Date:** ${entry.date}
**Tags:** ${entry.tags.join(', ')}
${entry.phase ? `**Phase:** ${entry.phase}` : ''}

## Context

${entry.context}

## Solution

${entry.solution}

${entry.relatedEntries?.length ? `## Related\n\n${entry.relatedEntries.map(r => `- ${r}`).join('\n')}` : ''}
`;
    writeFileSync(filePath, content);
  }

  private updateIndex(): void {
    let index = '# Knowledge Index\n\n';
    index += '| # | Date | Title | Tags | File |\n';
    index += '|---|------|-------|------|------|\n';
    
    this.entries
      .sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime())
      .forEach((entry, i) => {
        index += `| ${i + 1} | ${entry.date} | ${entry.title} | ${entry.tags.join(', ')} | ${entry.id}.md |\n`;
      });
    
    writeFileSync(this.indexPath, index);
  }

  search(query: string): SearchResult[] {
    // Limit query length to prevent ReDoS
    const sanitizedQuery = query.substring(0, 100);
    const queryLower = sanitizedQuery.toLowerCase();
    const queryWords = queryLower.split(/\s+/).filter(w => w.length > 0);

    return this.entries
      .map(entry => {
        let score = 0;
        const entryText = `${entry.title} ${entry.tags.join(' ')} ${entry.context} ${entry.solution}`.toLowerCase();

        for (const word of queryWords) {
          // Escape regex special characters
          const escapedWord = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
          const regex = new RegExp(escapedWord, 'gi');
          
          if (entryText.includes(word)) score += 1;
          if (entry.title.toLowerCase().includes(word)) score += 2;
          if (entry.tags.some(t => t.toLowerCase().includes(word))) score += 1.5;
        }

        return { entry, score };
      })
      .filter(r => r.score > 0)
      .sort((a, b) => b.score - a.score);
  }

  getByTag(tag: string): KnowledgeEntry[] {
    return this.entries.filter(e => e.tags.includes(tag));
  }

  getByPhase(phase: string): KnowledgeEntry[] {
    return this.entries.filter(e => e.phase === phase);
  }

  getRecent(count: number = 10): KnowledgeEntry[] {
    return [...this.entries]
      .sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime())
      .slice(0, count);
  }

  getStats(): { total: number; byTag: Record<string, number>; byPhase: Record<string, number> } {
    const byTag: Record<string, number> = {};
    const byPhase: Record<string, number> = {};

    for (const entry of this.entries) {
      for (const tag of entry.tags) {
        byTag[tag] = (byTag[tag] || 0) + 1;
      }
      if (entry.phase) {
        byPhase[entry.phase] = (byPhase[entry.phase] || 0) + 1;
      }
    }

    return { total: this.entries.length, byTag, byPhase };
  }
}
