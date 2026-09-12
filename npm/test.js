"use strict";

const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");

const manifest = JSON.parse(
  fs.readFileSync(path.join(__dirname, "package.json"), "utf8"),
);
assert.strictEqual(manifest.name, "@suiflex/fluxguard");
assert.strictEqual(manifest.bin.fluxguard, "bin/fluxguard.js");
assert.strictEqual(manifest.license, "Apache-2.0");
assert.ok(fs.existsSync(path.join(__dirname, "LICENSE")));
console.log("npm package metadata: ok");
