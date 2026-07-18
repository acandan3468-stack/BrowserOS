// Output Validator
// Validates subagent outputs before committing to state

export interface ValidationResult {
  valid: boolean;
  score: number;
  issues: ValidationIssue[];
}

export interface ValidationIssue {
  type: 'error' | 'warning' | 'info';
  message: string;
  field?: string;
  severity: 'critical' | 'high' | 'medium' | 'low';
}

export interface ValidationRule {
  name: string;
  description: string;
  validate: (input: unknown) => ValidationIssue[];
}

export class OutputValidator {
  private rules: ValidationRule[] = [];
  private maxInputLength = 100000; // 100KB limit

  constructor() {
    this.registerBuiltinRules();
  }

  private registerBuiltinRules(): void {
    this.addRule({
      name: 'required-fields',
      description: 'Checks for required fields in output',
      validate: (input: unknown) => {
        const issues: ValidationIssue[] = [];
        if (!input || typeof input !== 'object') {
          issues.push({
            type: 'error',
            message: 'Output must be a non-null object',
            severity: 'critical',
          });
          return issues;
        }
        const obj = input as Record<string, unknown>;
        if (!obj.title && !obj.name) {
          issues.push({
            type: 'warning',
            message: 'Output missing title or name field',
            field: 'title|name',
            severity: 'medium',
          });
        }
        return issues;
      },
    });

    this.addRule({
      name: 'no-secrets',
      description: 'Checks for accidental secret exposure',
      validate: (input: unknown) => {
        const issues: ValidationIssue[] = [];
        try {
          let text = JSON.stringify(input);
          
          // Limit input length to prevent ReDoS
          if (text.length > this.maxInputLength) {
            text = text.substring(0, this.maxInputLength);
          }
          
          const secretPatterns = [
            { pattern: /API_KEY\s*[:=]\s*['"][^'"]+['"]/i, name: 'API_KEY' },
            { pattern: /PASSWORD\s*[:=]\s*['"][^'"]+['"]/i, name: 'PASSWORD' },
            { pattern: /SECRET\s*[:=]\s*['"][^'"]+['"]/i, name: 'SECRET' },
            { pattern: /PRIVATE_KEY\s*[:=]/i, name: 'PRIVATE_KEY' },
            { pattern: /-----BEGIN.*PRIVATE KEY-----/, name: 'RSA_PRIVATE_KEY' },
            { pattern: /AKIA[0-9A-Z]{16}/, name: 'AWS_ACCESS_KEY' },
            { pattern: /mysql:\/\/[^:]+:[^@]+@/, name: 'MYSQL_CONNECTION' },
            { pattern: /postgres:\/\/[^:]+:[^@]+@/, name: 'POSTGRES_CONNECTION' },
            { pattern: /mongodb:\/\/[^:]+:[^@]+@/, name: 'MONGODB_CONNECTION' },
          ];
          
          // Use simple string matching instead of complex regex for JWT
          if (text.includes('eyJ') && text.includes('.') && text.split('eyJ').length > 1) {
            // Basic JWT detection without complex regex
            const jwtParts = text.match(/eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]*/g);
            if (jwtParts && jwtParts.length > 0) {
              issues.push({
                type: 'error',
                message: 'Potential JWT token detected',
                severity: 'critical',
              });
            }
          }
          
          for (const { pattern, name } of secretPatterns) {
            if (pattern.test(text)) {
              issues.push({
                type: 'error',
                message: `Potential secret detected: ${name}`,
                severity: 'critical',
              });
            }
          }
        } catch {
          // JSON.stringify failed, skip check
        }
        return issues;
      },
    });

    this.addRule({
      name: 'confidence-check',
      description: 'Validates confidence level if present',
      validate: (input: unknown) => {
        const issues: ValidationIssue[] = [];
        if (!input || typeof input !== 'object') return issues;
        const obj = input as Record<string, unknown>;
        
        if ('confidence' in obj) {
          const confidence = obj.confidence as number;
          if (typeof confidence !== 'number' || confidence < 0 || confidence > 1) {
            issues.push({
              type: 'error',
              message: 'Confidence must be a number between 0 and 1',
              field: 'confidence',
              severity: 'high',
            });
          }
        }
        return issues;
      },
    });
  }

  addRule(rule: ValidationRule): void {
    this.rules.push(rule);
  }

  validate(input: unknown): ValidationResult {
    const allIssues: ValidationIssue[] = [];
    
    for (const rule of this.rules) {
      try {
        const issues = rule.validate(input);
        allIssues.push(...issues);
      } catch (error) {
        allIssues.push({
          type: 'error',
          message: `Validation rule ${rule.name} threw error: ${(error as Error).message}`,
          severity: 'high',
        });
      }
    }

    const criticalCount = allIssues.filter(i => i.severity === 'critical').length;
    const highCount = allIssues.filter(i => i.severity === 'high').length;
    const mediumCount = allIssues.filter(i => i.severity === 'medium').length;
    const lowCount = allIssues.filter(i => i.severity === 'low').length;
    const score = Math.max(0, 100 - (criticalCount * 30) - (highCount * 15) - (mediumCount * 5) - (lowCount * 2));

    return {
      valid: criticalCount === 0,
      score,
      issues: allIssues,
    };
  }
}
