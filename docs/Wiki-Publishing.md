# Wiki Publishing

The committed wiki source lives in `docs/`. The GitHub Actions workflow copies
that directory to the repository wiki on trusted `master` pushes.

The `documentation/` directory is separate project/internal material and is not
published to the consumer wiki.

## Authentication

GitHub documents wikis as Git repositories that can be cloned with a
`.wiki.git` URL after the wiki has been initialized. GitHub also documents
`GITHUB_TOKEN` as scoped to the repository that contains the workflow.

This repository follows the same authentication model used by the related
Netdata AI Agent repository: the workflow uses `secrets.GITHUB_TOKEN`, grants
the publish job `contents: write`, and passes the token through an ephemeral
Git authorization header rather than storing credentials in the checked-out
remote configuration.

## Required GitHub Setup

1. Enable the repository wiki.
2. Create the first wiki page from the GitHub Wiki UI once, so that the
   backing `.wiki.git` repository exists.

GitHub only exposes the wiki Git repository after the first wiki page exists.
The publish workflow checks this before cloning and fails with a setup error if
the wiki Git repository has not been initialized yet.

No custom wiki token is required. Do not commit token values or put token values
in docs, SOWs, logs, or workflow summaries.

## Workflow Behavior

The workflow:

- validates pull requests that affect `docs/**`, the wiki validator, or the
  wiki workflow;
- publishes only on `master` pushes affecting `docs/**` or the wiki workflow,
  plus manual dispatch;
- checks out the repository read-only;
- validates internal wiki links locally;
- verifies that the backing wiki Git repository exists;
- clones the wiki into runner temporary storage;
- replaces the wiki working tree with `docs/`;
- commits only when content changed;
- pushes to the wiki branch that was cloned.

Pull requests do not publish the wiki.

## Local Validation

Run:

```sh
python3 tests/docs/check_wiki_docs.py
```

The validator checks required wiki files, local Markdown links, wiki-style
links, and accidental local/private path leakage.

Optional private terms can be checked without hardcoding them into repository
files:

```sh
DOCS_FORBIDDEN_TERMS="term1,term2" python3 tests/docs/check_wiki_docs.py
```

## Internal Links

Internal wiki navigation must use GitHub wiki page links:

```markdown
[[API-Overview|API Overview]]
```

Do not link internal wiki pages as Markdown files:

```markdown
[API Overview](API-Overview.md)
```

The validator rejects internal `*.md` links because they can render as
repository file/raw-style links in the published GitHub wiki instead of wiki
page links.
