# FND-001 — Repository and source-control ownership inspection

**Status:** Inspected  
**Inspection date:** 2026-08-08

## Findings

### Canonical project root

The roadmap and the user-directed directory layout identify this path as the project root:

```text
/Users/superuser/Desktop/soccer/socket
```

The root currently contains the roadmap, accepted decision records, the initial Cargo workspace, and the small domain state-machine increment. It has no `.git` directory and no configured remote.

### Existing neighboring repository

There is a separate Git repository at:

```text
/Users/superuser/Desktop/soccer/soccer
```

Evidence from read-only inspection:

```text
remote: https://github.com/WIKKIwk/soccer.git
tracked file: ARCHITECTURE_ROADMAP.md
HEAD: f18e9d8 Add football platform architecture roadmap
status: D ARCHITECTURE_ROADMAP.md
```

The roadmap was moved into `socket/` by the requested directory change. The deletion in the neighboring repository is preserved as an existing worktree change and has not been committed, pushed, or reverted.

## Ownership decision

`socket/` is the canonical implementation root and must not import the neighboring repository’s `.git` metadata or silently mix its source-control boundary. The neighboring `soccer/` repository remains a separate repository until an explicit publication or repository-migration decision is made.

Local Git initialization for `socket/` is a separate source-control action. It must happen at the project-root setup step, with generated build output ignored and remote/commit behavior kept separate from local implementation. No commit, branch operation, remote write, or push is part of FND-001.

## Scope implications

- Commands and tests for this project run from `socket/`.
- The neighboring repository must not be used to infer implementation files or dependencies; it currently contains only its own roadmap history.
- `target/` is generated build output and must not become a tracked source artifact when source control is initialized.
- The roadmap’s stated `socket/` structure is consistent with the selected implementation root.

## Acceptance evidence

FND-001 is complete: the project root, neighboring repository, remote ownership, current worktree change, and pre-initialization source-control boundary were inspected without destructive Git operations.

