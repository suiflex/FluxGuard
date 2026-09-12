#!/usr/bin/env python3
"""Render the Scoop manifest for suiflex/scoop-bucket, printed to stdout.

Run from the repository root by .github/workflows/release-build.yml, with the
release's Windows archives already downloaded into the working directory.

A manifest is JSON, so it is built rather than string-templated: that makes an
unsubstituted placeholder or a dangling comma from an omitted optional stanza
impossible, and those are exactly the mistakes that stay invisible until a user
runs `scoop install`.

`TAG` and `REPO` come from the workflow environment.
"""

import hashlib
import json
import os
import sys

REPO = os.environ.get("REPO", "suiflex/FluxGuard")
TAG = os.environ["TAG"]
BASE = f"https://github.com/{REPO}/releases/download/{TAG}"

ARCHES = {
    "64bit": "fluxguard-windows-x86_64.zip",
}


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main():
    architecture = {}
    for arch, asset in ARCHES.items():
        if os.path.exists(asset):
            architecture[arch] = {"url": f"{BASE}/{asset}", "hash": sha256(asset)}

    if "64bit" not in architecture:
        sys.exit("64bit archive is missing; refusing to render a manifest without it")

    json.dump(
        {
            "version": TAG.lstrip("v"),
            "description": "Provider-agnostic resource awareness layer for AI coding agents",
            "homepage": f"https://github.com/{REPO}",
            "license": "Apache-2.0",
            "architecture": architecture,
            "bin": "fluxguard.exe",
            "checkver": "github",
            "autoupdate": {
                "architecture": {
                    arch: {"url": f"https://github.com/{REPO}/releases/download/v$version/{asset}"}
                    for arch, asset in ARCHES.items()
                }
            },
        },
        sys.stdout,
        indent=2,
    )
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
