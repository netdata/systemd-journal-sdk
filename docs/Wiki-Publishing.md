# Wiki Publishing

The committed wiki source lives in `docs/`. The GitHub Actions workflow copies
that directory to the repository wiki on trusted `master` pushes.

The `documentation/` directory is separate project/internal material and is not
published to the consumer wiki.

## Authentication

GitHub documents wikis as Git repositories that can be cloned with a
`.wiki.git` URL after the wiki has been initialized. GitHub also documents
`GITHUB_TOKEN` as scoped to the repository that contains the workflow.

This repository follows the same GitHub Actions pattern used by the related
Netdata AI Agent repository: the workflow checks out
`${{ github.repository }}.wiki` with `actions/checkout` and
`secrets.GITHUB_TOKEN`, and grants the publish job `contents: write`.

## Required GitHub Setup

1. Enable the repository wiki.
2. Create the first wiki page in GitHub so that
   `https://github.com/netdata/systemd-journal-sdk.wiki.git` exists.

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
