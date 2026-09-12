# @suiflex/fluxguard

The npm launcher for FluxGuard.

Install the Rust runtime and the npm command:

```bash
cargo install fluxguard
npm install --global @suiflex/fluxguard
```

Then connect a client to the local MCP server:

```bash
fluxguard install
```

The launcher uses `FLUXGUARD_BIN` when set, otherwise it finds `fluxguard` on
`PATH`. It never reads credentials or sends usage data anywhere.

This package is a launcher; the publishable Rust runtime is the `fluxguard`
crate on crates.io.
