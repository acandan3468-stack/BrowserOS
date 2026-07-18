# 00-init — Project Initialization

## Entry Criteria
- New project or first session
- User invokes `vibe init`

## Steps

### 1. Detect Project
- Scan root directory for package.json, Cargo.toml, requirements.txt, pubspec.yaml
- Auto-detect technology stack
- Output: Project type and stack

### 2. Create Configuration
- Generate `.vibe/config.json` with detected settings
- Create `.vibe/state/events.jsonl`
- Initialize knowledge base

### 3. Initialize Workspace
- Create workspace directories (plans, reports, archive, incidents)
- Create initial CONTEXT.md template

### 4. Update State
- Append STARTED event for init phase
- Append COMPLETED event

## Exit Criteria
- [ ] Config file created
- [ ] State initialized
- [ ] Knowledge base ready
- [ ] Workspace directories created

## Duration
Expected: 1-2 minutes
