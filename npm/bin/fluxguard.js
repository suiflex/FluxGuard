#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

function executableCandidates() {
  const candidates = [];
  if (process.env.FLUXGUARD_BIN) candidates.push(process.env.FLUXGUARD_BIN);
  const local = path.join(__dirname, "..", "vendor", process.platform, process.arch, "fluxguard");
  candidates.push(process.platform === "win32" ? `${local}.exe` : local);
  return candidates;
}

function existingPath(command) {
  if (!command) return null;
  try {
    fs.accessSync(command, fs.constants.X_OK);
    return command;
  } catch {
    return null;
  }
}

function pathBinary() {
  const lookup = process.platform === "win32" ? "where" : "which";
  const result = spawnSync(lookup, ["fluxguard"], { encoding: "utf8" });
  if (result.status !== 0) return null;
  return result.stdout
    .split(/\r?\n/)
    .map((entry) => entry.trim())
    .filter(Boolean)
    .find((entry) => path.resolve(entry) !== path.resolve(process.argv[1])) || null;
}

const binary = executableCandidates().map(existingPath).find(Boolean) || pathBinary();
if (!binary) {
  console.error("FluxGuard binary was not found.");
  console.error("Install it with: cargo install fluxguard");
  console.error("Or set FLUXGUARD_BIN to an existing FluxGuard binary.");
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`Could not start FluxGuard: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
