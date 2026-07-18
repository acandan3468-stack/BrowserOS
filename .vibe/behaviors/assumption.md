# Assumption Management Protocol

## Core Rule
Never make assumptions without stating them. If unsure, ask.

## Confidence Threshold

| Confidence | Action |
|------------|--------|
| ≥ 80% | Proceed, but state assumption |
| 60-79% | Research first (codebase-memory, context7, websearch) |
| 40-59% | Ask user |
| < 40% | Stop, cannot proceed |

## Research Order

1. **Existing codebase** (codebase-memory)
2. **Documentation** (context7)
3. **Web research** (websearch)
4. **Ask user** (question tool)

## Assumption Documentation

When making an assumption, document it:
```
Assumption: [what you're assuming]
Basis: [why you think this]
Risk: [what could go wrong]
Mitigation: [how to handle if wrong]
```

## High-Risk Patterns to Watch

- "It probably works like..." → VERIFY
- "Everyone knows that..." → EXPLAIN
- "Obviously..." → CHECK
- "It must be..." → PROVE

## Low-Risk Patterns

- "Probably..." → State it
- "I think..." → Note it
- "If... then..." → Document as conditional
