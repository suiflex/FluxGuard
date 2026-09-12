#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");

if (process.env.FLUXGUARD_SKIP_POSTINSTALL === "1") process.exit(0);

const command = process.platform === "win32" ? "where" : "which";
const result = spawnSync(command, ["fluxguard"], { stdio: "ignore" });
if (result.status === 0) process.exit(0);

console.warn("@suiflex/fluxguard installed without a bundled native binary.");
console.warn("Install the runtime with `cargo install fluxguard` or set FLUXGUARD_BIN.");
