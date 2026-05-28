---
name: project-release-tagging
description: "Use when creating, checking, or pushing release tags for this repository, especially Go module release tags."
---
# Project Release Tagging

## Purpose

Ensure published releases are consumable by all SDK users, including Go users
whose module lives under the `go/` subdirectory.

## Scope

Use this skill when:

- creating a release tag;
- checking whether a release tag exists locally or remotely;
- pushing a release tag;
- preparing a release that should be consumed by `go get`.

## Mandatory Knowledge

- This repository has a root release tag, for example `v0.2.0`.
- The Go module path is declared in `go/go.mod` as
  `github.com/netdata/systemd-journal-sdk/go`.
- Because the Go module is in the repository subdirectory `go/`, each Go
  module release must also have a tag prefixed with that subdirectory, for
  example `go/v0.2.0`.
- The root release tag and the Go submodule release tag must point to the same
  commit unless a SOW records a deliberate split release decision.
- Go's module reference defines this rule: if a module is defined in a
  repository subdirectory, the semantic version tag name is prefixed with the
  module subdirectory followed by `/`.
- Once pushed, release tags must not be moved or deleted without explicit user
  approval. Go module proxies and checksum databases may cache the old tag
  target.

## Workflow

1. Confirm the release version, for example `v0.2.0`.
2. Verify the worktree is clean:

   ```bash
   git status --short --branch
   ```

3. Verify the Go module path:

   ```bash
   sed -n '1,20p' go/go.mod
   ```

4. Check local and remote tags before creating anything:

   ```bash
   git tag -l 'v0.2.0' 'go/v0.2.0'
   git ls-remote --tags origin refs/tags/v0.2.0 refs/tags/v0.2.0^{} refs/tags/go/v0.2.0 refs/tags/go/v0.2.0^{}
   ```

5. If either tag exists at a different commit, stop and ask the user. Do not
   move, delete, force-push, or recreate tags without explicit approval.
6. Create annotated tags on the intended commit:

   ```bash
   git tag -a v0.2.0 <commit> -m 'v0.2.0'
   git tag -a go/v0.2.0 <commit> -m 'go/v0.2.0'
   ```

7. Push the branch first, then push both tags:

   ```bash
   git push origin <branch>
   git push origin v0.2.0 go/v0.2.0
   ```

8. Verify remote tag targets:

   ```bash
   git ls-remote --tags origin refs/tags/v0.2.0 refs/tags/v0.2.0^{} refs/tags/go/v0.2.0 refs/tags/go/v0.2.0^{}
   ```

9. Report the peeled tag commit hashes to the user.

## Validation Checklist

- Root tag exists locally and remotely.
- Go submodule tag exists locally and remotely.
- Both peeled tag targets are the same commit.
- The branch containing that commit is pushed.
- `git status --short --branch` is clean after release work.

## Evidence

- `go/go.mod`: Go module path.
- Go Modules Reference, "Mapping versions to commits":
  `https://go.dev/ref/mod#vcs-version`.
