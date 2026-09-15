# Taste

## Communication
- Communicates in Japanese; expects prompts, spec documents, and task plans written in Japanese (including spec artifacts such as 要件/設計/タスク headings and completion conditions). Confidence: 0.85

## Workflow
- Drives development through the Kiro spec workflow via slash commands (`/kiro-discovery`, `/kiro-spec-quick`) with a spec name argument — specs are written before implementation and treated as the source of truth. Confidence: 0.8
- Uses agent task notifications / delegated analysis as part of the working loop: asks the main session to gather codebase contracts and external research before committing to a design. Confidence: 0.6

## Git
- Commits in small explicit steps (bare "commit" after a unit of work), then merges back with a fast-forward into `main` and deletes the feature branch — prefers linear history, no merge commits, no lingering branches. Confidence: 0.7
