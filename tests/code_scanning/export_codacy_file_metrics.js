#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");

function usage() {
  console.error(`usage: export_codacy_file_metrics.js --output PATH [options]

Options:
  --provider VALUE       Git provider, default: gh
  --organization VALUE   Organization, default: netdata
  --repository VALUE     Repository, default: systemd-journal-sdk
  --branch VALUE         Codacy branch, default: master
  --search VALUE         Path search prefix, repeatable, default: go/ and rust/
  --limit VALUE          Page size, default: 100
	`);
}

function defaultArgs() {
  return {
    provider: "gh",
    organization: "netdata",
    repository: "systemd-journal-sdk",
    branch: "master",
    searches: [],
    limit: 100,
    output: "",
  };
}

function applyOption(args, flag, value) {
  switch (flag) {
    case "--provider":
      args.provider = value;
      break;
    case "--organization":
      args.organization = value;
      break;
    case "--repository":
      args.repository = value;
      break;
    case "--branch":
      args.branch = value;
      break;
    case "--search":
      args.searches.push(value);
      break;
    case "--limit":
      args.limit = Number.parseInt(value, 10);
      if (!Number.isSafeInteger(args.limit) || args.limit < 1 || args.limit > 1000) {
        throw new Error("--limit must be an integer from 1 to 1000");
      }
      break;
    case "--output":
      args.output = value;
      break;
    default:
      usage();
      process.exit(2);
  }
}

function parseArgs(argv) {
  const args = defaultArgs();

  for (let i = 2; i < argv.length; i += 1) {
    const flag = argv[i];
    const value = argv[i + 1];
    if (!flag.startsWith("--") || value === undefined) {
      usage();
      process.exit(2);
    }
    i += 1;
    applyOption(args, flag, value);
  }

  if (!args.output) {
    usage();
    process.exit(2);
  }
  if (args.searches.length === 0) {
    args.searches = ["go/", "rust/"];
  }
  return args;
}

function findOnPath(name) {
  const paths = (process.env.PATH || "").split(path.delimiter);
  for (const directory of paths) {
    const candidate = path.join(directory, name);
    if (fs.existsSync(candidate)) return candidate;
  }
  return "";
}

function codacyPackageRoot() {
  if (process.env.CODACY_CLOUD_CLI_ROOT) {
    return process.env.CODACY_CLOUD_CLI_ROOT;
  }

  const executable = findOnPath("codacy");
  if (!executable) {
    throw new Error("codacy executable not found on PATH");
  }

  const realPath = fs.realpathSync(executable);
  const distDir = path.dirname(realPath);
  const root = path.dirname(distDir);
  if (!fs.existsSync(path.join(root, "dist", "api", "client", "index.js"))) {
    throw new Error(`could not locate Codacy Cloud CLI package root from ${realPath}`);
  }
  return root;
}

function loadCodacyClient() {
  const root = codacyPackageRoot();
  const client = require(path.join(root, "dist", "api", "client"));
  const auth = require(path.join(root, "dist", "utils", "auth"));
  try {
    auth.checkApiToken();
  } catch (error) {
    // Public repositories can be queried without authentication. Keep going so
    // the script also works in read-only environments without stored secrets.
  }
  return client;
}

function sanitizeFile(item, search) {
  return {
    search,
    path: item.path,
    grade: item.grade,
    gradeLetter: item.gradeLetter,
    totalIssues: item.totalIssues,
    complexity: item.complexity,
    coverage: item.coverage,
    coverageWithDecimals: item.coverageWithDecimals,
    duplication: item.duplication,
    linesOfCode: item.linesOfCode,
    sourceLinesOfCode: item.sourceLinesOfCode,
    numberOfMethods: item.numberOfMethods,
    numberOfClones: item.numberOfClones,
  };
}

async function listFilesForSearch(service, args, search) {
  const files = [];
  let cursor = undefined;
  const seenCursors = new Set();

  for (;;) {
    const response = await service.listFiles(
      args.provider,
      args.organization,
      args.repository,
      args.branch,
      search,
      "filename",
      "asc",
      cursor,
      args.limit,
    );

    const pageFiles = Array.isArray(response.data) ? response.data : [];
    files.push(...pageFiles.map((item) => sanitizeFile(item, search)));

    const nextCursor = response.pagination && response.pagination.cursor;
    if (!nextCursor || seenCursors.has(nextCursor)) {
      break;
    }
    seenCursors.add(nextCursor);
    cursor = nextCursor;
  }

  return files;
}

async function main() {
  const args = parseArgs(process.argv);
  const { RepositoryService } = loadCodacyClient();
  const byPath = new Map();

  for (const search of args.searches) {
    const files = await listFilesForSearch(RepositoryService, args, search);
    for (const file of files) {
      byPath.set(file.path, file);
    }
  }

  const payload = {
    provider: args.provider,
    organization: args.organization,
    repository: args.repository,
    branch: args.branch,
    searches: args.searches,
    fetchedAt: new Date().toISOString(),
    count: byPath.size,
    files: Array.from(byPath.values()).sort((a, b) => a.path.localeCompare(b.path)),
  };

  fs.mkdirSync(path.dirname(args.output), { recursive: true });
  fs.writeFileSync(args.output, `${JSON.stringify(payload, null, 2)}\n`);
}

main().catch((error) => {
  console.error(error && error.message ? error.message : error);
  process.exit(1);
});
