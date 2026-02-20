# [WARNING] Still buggy and under development

# [DRAFT] `git-pluck`

Create a separate Git branch containing a mapped subset of a repository's files, with full commit history preserved.

`git-pluck` takes a configuration file that defines source-to-destination path mappings, then rewrites the repository's commit history so that each "pluck" commit contains only the files selected by those mappings, placed at their mapped destinations.
Original blob objects are reused (no content duplication), commit metadata is preserved by default, and parent ancestry is reconstructed so the pluck has a contiguous, valid history.
`git-pluck` has similarities with https://github.com/xoofx/git-rocket-filter and https://github.com/jasonwhite/git-subset yet goes a step further.

## Example Use-Cases for `git-pluck`

- **Component extraction/exclusion**: Pluck just the desired component(s) into a branch deployable to a different site
- **Monorepo to multi-repo**: Extract a subproject from a monorepo into its own branch with full history
- **Vendor inclusion**: Include third-party code as a subtree with preserved history
- **Privacy**: Replace author/committer info for commits from external contributors before sharing
- **Submodule replacement**: Replace git-submodule workflows with a cleaner or just different branch-based approach

## Installation

### From Source

Requires Rust 1.70+ and Cargo:

```bash
cargo install --git <repository-url> git-pluck
```

Or build locally:

```bash
git clone <repository-url>
cd git-pluck
cargo build --release
# Binary at target/release/git-pluck
```

### Binary Location

After installation, ensure `git-pluck` is on your `PATH`, or place it in a directory like `~/.local/bin/`.

## Quick Start

1. **Create a config file** defining what to pluck:

```ini
# pluckname.cfg
[forward.from "src"]
    to = (Mirror)

[forward.from "README.md"]
    to = (Mirror)

[forward.from "tests"]
    to = (Remove)
```

2. **Run git-pluck**:

```bash
git-pluck -c pluckname.cfg
```

3. **Result**: A new branch `refs/heads/pluck/pluckname` contains only `src/` and `README.md` (excluding `tests/`) with full commit history.

## Configuration Files

A pluck branch name and the configuration file are resolved in this priority order:

1. **Explicit file**: Path provided via `-c` / `--config` CLI flag
2. **Embedded in tree**: `.gitpluck/<pluckname>/config.cfg` at the start reference (HEAD by default)
3. **Working tree**: `.gitpluck/<pluckname>/config.cfg` in the working directory

The Pluck branch name defaults to the config file's basename.

### Config File Format

Config files use standard Git INI format.
They have any suffix, though `.ini` or `.cfg` make them easy to spot.
They have two categories of settings:

```ini
# Tree mappings define what goes into the pluck
[forward.from "<source-path>"]
    to = <destination>

# Behavior settings control how plucking works
[pluck]
    <key> = <value>
```

## Tree Mappings

Tree mappings define which paths to include in the pluck and where to place them.

### Basic Syntax

```ini
[forward.from "<source-path>"]
    to = <destination>
```

### Destination Values

| Value | Name | Description |
|-------|------|-------------|
| `(Mirror)` | Mirror | Include the path at the same location in the pluck |
| `(Remove)` | Remove | Remove the path and all descendants from the pluck |
| `.` | Unpack | Unpack the tree's contents into the pluck root |
| Any path string | Move | Place the source at this destination path |
| `(Copy)<path>` | Copy | Duplicate the source to an additional destination |

### Examples

```ini
# Mirror: include src/ at the same path
[forward.from "src"]
    to = (Mirror)

# Move: rename src/ to lib/ in the pluck
[forward.from "src"]
    to = lib

# Remove: completely remove vendor/ from the pluck
[forward.from "vendor"]
    to = (Remove)

# Unpack: flatten build/artifacts/ contents to pluck root
[forward.from "build/artifacts"]
    to = .

# Copy: move to dest AND duplicate to backup/
[forward.from "config"]
    to = dest
    to = +backup
```

### Root Mappings

The special source `.` refers to the entire repository root:

```ini
# Include everything under a "backend/" prefix
[forward.from "."]
    to = backend

# Include everything as-is (mirror for all files)
[forward.from "."]
    to = (Mirror)
```

Root mappings act as catch-alls. More-specific mappings override the root:

```ini
# Everything goes under "backend/" EXCEPT docs/ which stays at "docs/"
[forward.from "."]
    to = backend

[forward.from "docs"]
    to = docs
```

## Behavior Settings

Behavior settings go under the `[pluck]` section:

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `debug` | int | `0` | Debug output level (0=none, 1=single-line, 2=multiline) |
| `force` | bool | `false` | Bypass sanity checks and validation errors |
| `allowUnchangedTree` | bool | `false` | Force new pluck commit even when tree is unchanged |
| `mirrorMap` | bool | `false` | Replace all mappings with mirror (to = (Mirror)), strip copies |
| `startRef` | string | `HEAD` | Source history reference to start plucking from |
| `allowMissingPath` | bool | `false` | Don't error when a mapped source path doesn't exist |
| `allowNestedMap` | bool | `false` | Allow nested mappings |
| `allowIncompleteAncestry` | bool | `false` | Allow incomplete parent ancestry |
| `quiet` | bool | `false` | Suppress progress output |
| `recursive` | string | `""` | Process commits recursively (non-empty = enabled) |
| `autoReverseMap` | bool | `false` | Auto reverse all src:dst path mappings |
| `logMessage` | bool | `false` | Add sha of the plucked commit to the pluck commit messsage |
| `logBranch` | bool | `true` | Use log branch for incremental plucking |
| `skipDedupAncestry` | bool | `false` | Skip transitive parent deduplication |
| `ignorantPluck` | string | `""` | Ignore history and add pluck commit on top of specified ref. |

## Usage Examples

### Extract a Subproject

Create a branch containing only the `packages/web/` directory as the pluck root:

```ini
# web-content
[forward.from "packages/web"]
    to = (Mirror)

[forward.from "packages/shared"]
    to = shared

[forward.from ".github"]
    to = (Remove)

[forward.from "LICENSE"]
    to = (Mirror)
```

```bash
git-pluck -c web_content.cfg
```

### Monorepo Backend/Frontend Split

```ini
# backend-config
[forward.from "."]
    to = backend

[forward.from "apps/web"]
    to = (Remove)

[forward.from "packages/ui"]
    to = (Remove)

[forward.from "README.md"]
    to = README.md

[forward.from "packages/shared"]
    to = packages/shared
```

```bash
git-pluck -c backend-config
```

### Explicit pluck branch name

Create a pluck branch name that differs from the config filename.

```bash
git-pluck -c backend_config -- other_pluck_name
```

### Documentation Site

Extract documentation and deploy assets:

```ini
# docs-standalone
[forward.from "docs"]
    to = .

[forward.from "static"]
    to = static

[forward.from "docs-config.yaml"]
    to = config.yaml
```

```bash
git-pluck -c docs-standalone
```

### Recursive History

Pluck the full history of changes to a specific directory:

```bash
git-pluck -c mysubproject.cfg --recursive --log-branch
```

Limit to the last 10 commits:

```bash
git-pluck -c mycustomer.cfg --recursive=max-count:10 --log-branch
```

### Incremental Plucking

Run git-pluck multiple times. Only new commits since the last run are processed:

```bash
# First run: processes all commits up to HEAD
git-pluck -c mynewcustomer.cfg --recursive --log-branch

# Later: only processes new commits
git-pluck -c mynewcustomer.cfg --recursive --log-branch
```

### Author Anonymization

Replace author info for all commits from external contributors:

```bash
git-pluck -c compliance_correct.cfg --log-branch \
    --rep-author-regex=".*@external\.com:" \
    --rep-author-name="External Contributor" \
    --rep-author-email="contributor@company.com"
```

The regex format is `allow_pattern:deny_pattern`.
Emails matching the allow pattern are protected from replacement.
The deny pattern (after `:`) overrides the allow: if an email matches the deny pattern, it is replaced regardless.

Replace ALL authors unconditionally:

```bash
git-pluck -c data_privacy.cfg --log-branch \
    --rep-author-name="Anonymous" \
    --rep-author-email="anon@company.com"
```

### Message Sanitization

Replace all commit messages:

```bash
git-pluck -c useless_commit_messages.cfg --log-branch \
    --rep-message="Cherry-picked from monorepo"
```

Remove lines matching a pattern (e.g., remove internal ticket references):

```bash
git-pluck -c my.cfg --log-branch \
    --rep-message-filter="PROJ-[0-9]+"
```

### Ignore Pluck History and add Pluck Commit on Arbitrary Branch

Instead of creating `refs/heads/pluck/<pluckname>`, add pluck commit to existing branch with no regard commit history:

```bash
git-pluck -c other.cfg --log-branch --ignorant-pluck=refs/heads/my-branch
```

### Log Message Mode

Source SHAs logged in pluck commit messages (`Plucked from: <SHA>` trailer) instead of a log branch:

```bash
git-pluck -c my.cfg --log-message --no-log-branch
```

### Map Management

Add a mapping to an existing config:

```bash
git-pluck -c my.cfg --add-map="new/path:destination"
git-pluck -c my.cfg --add-map="just/include"       # mirror map
git-pluck -c my.cfg --add-map="remove/this:(Remove)"  # remove
```

Remove a mapping:

```bash
git-pluck -c my.cfg --remove="path/to/remove"
```

Validate the current map:

```bash
git-pluck -c my.cfg --check-config
```

List sources and destinations:

```bash
git-pluck -c my.cfg --show-src-paths
git-pluck -c my.cfg --show-dst-paths
```

### Query Mappings

Find the source SHA for a pluck SHA:

```bash
git-pluck -c my.cfg --find-source-sha=<pluck-sha>
```

Find the pluck SHA for an source SHA:

```bash
git-pluck -c my.cfg --find-pluck-sha=<source-sha>
```

## CLI Reference

```
git-pluck [OPTIONS] [PLUCKNAME]
```

*Note: Empty config keys and flags are not allowed!*

### General Options

| Flag | Config Key | Description |
|------|-----------|-------------|
| `-c FILE`, `--config=FILE` | - | Config file path |
| `-d`, `--debug[=N]` | `debug` | Debug output (N=1 or 2) |
| `-f`, `--force` | `force` | Bypass checks |
| `-q`, `--quiet` | `quiet` | Suppress output |
| `-t`, `--timer` | `timer` | Show timing info |

### Pluck History Options

| Flag | Config Key | Description |
|------|-----------|-------------|
| `--allow-unchanged-tree` | `allowUnchangedTree` | Force new pluck commit even if tree is unchanged |
| `-s REF`, `--start-ref=REF` | `startRef` | Start plucking from this reference |
| `-p`, `--allow-incomplete-ancestry` | `allowIncompleteAncestry` | Allow incomplete ancestry |
| `-r`, `--recursive[=OPTS]` | `recursive` | Process full/limited history |
| `--log-branch` | `logBranch` | Create log branch (default: on) |
| `--skip-dedup-ancestry` | `skipDedupAncestry` | Skip parent deduplication |
| `--ignorant-pluck=REF` | `ignorantPluck` | Add pluck commit to ref without considering history |

### Pluck Map Options

| Flag | Config Key | Description |
|------|-----------|-------------|
| `--add-map=SRC:DST` | - | Add mapping to config file |
| `--mirror-map` | `mirrorMap` | Replace all mappings with mirror (to = (Mirror)), strip copies |
| `--show-src-paths` | - | List all src-paths |
| `--show-dst-paths` | - | List all dst-paths |
| `--check-config` | - | Validate the map |
| `--allow-missing-path` | `allowMissingPath` | Allow missing src-paths |
| `--allow-nested-map` | `allowNestedMap` | Allow nested mappings |
| `--remove=SRC` | - | Remove mapping from config |
| `--auto-reverse-map` | `autoReverseMap` | Reverse source-to-destination mappings |

### Commit Modification Options

| Flag | Config Key | Description |
|------|-----------|-------------|
| `--rep-author-name=NAME` | `repAuthorName` | Replace author name |
| `--rep-author-email=EMAIL` | `repAuthorEmail` | Replace author email |
| `--rep-author-regex=REGEX` | `repAuthorRegex` | Conditional author replacement |
| `--rep-committer-name=NAME` | `repCommitterName` | Replace committer name |
| `--rep-committer-email=EMAIL` | `repCommitterEmail` | Replace committer email |
| `--rep-committer-regex=REGEX` | `repCommitterRegex` | Conditional committer replacement |
| `--rep-message=MSG` | `repMessage` | Replace commit message |
| `--rep-message-filter=REGEX` | `repMessageFilter` | Filter message lines |
| `--log-message` | `logMessage` | Add source SHA to pluck commit message |

### Relationship Queries

| Flag | Description |
|------|-------------|
| `--find-source-sha=REF` | Find source SHA for a pluck ref |
| `--find-pluck-sha=REF` | Find pluck SHA for an source ref |

### Negation Flags

Every boolean and string flag has a `--no-X` variant that explicitly disables or clears the setting, overriding any config file value.
For example: `--no-log-branch`, `--no-rep-author-name`, `--no-recursive`.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Plucking error (no revisions, tree issues, ref update failure) |
| 2 | Configuration error (missing config, invalid settings) |
| 3 | Internal error (ancestry resolution, inconsistency) |

Set `PLUCK_NO_RETURN_ERROR=1` to make all errors return exit code 0.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PLUCK_NO_RETURN_ERROR` | `0` | If non-zero, all errors return exit code 0 |
| `PLUCK_TEST_MODE` | `0` | If non-zero, exit before plucking (for test sourcing) |

## Ref Layout

git-pluck creates refs within the repository:

| Ref | Purpose |
|-----|---------|
| `refs/heads/pluck/<pluckname>` | The pluck branch (mapped subset) |
| `refs/heads/pluck/log/<pluckname>` | Log branch (logs source to pluck commits) |
| `refs/heads/pluck/recovery/<pluckname>` | Progress ref (crash recovery, deleted on success) |

## Example Config files

A comprehensive example using most available options: [pluck_examples.cfg](pluck_config_examples).

## FAQ

**Q: Does git-pluck duplicate blob objects?**
No. The pluck reuses the same blob OIDs from the source repository. Only tree and commit objects are new.

**Q: Can I push the pluck branch to a separate remote?**
Yes. The pluck branch is a regular Git branch. You can push it to any remote:
```bash
git push origin pluck/mypluckname:master
```

**Q: What happens when I run git-pluck twice?**
If no new commits exist since the last run, no new pluck commits are created (idempotent).
Use `--allow-unchanged-tree` to force a new pluck commit regardless.

**Q: Can I pluck into a different repository?**
git-pluck operates within a single repository. To pluck into a separate repo, push the pluck branch to the target repo after creation.

**Q: How do merge commits work?**
Merge commits preserve their parent structure.
Each source parent is resolved to its pluck equivalent, maintaining the merge topology in the pluck history.

**Q: What if a file path changes between commits?**
git-pluck processes each commit independently.
If a file exists in one commit but not another, it appears in the corresponding pluck commit but not the other.
This preserves the natural file lifecycle.

## Use of Coding Agents

`git-pluck` was developed under extensive use of Claude Code along with Qwen3.6-27b.
