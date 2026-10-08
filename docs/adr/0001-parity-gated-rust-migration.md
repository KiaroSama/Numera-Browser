# Parity-gated Rust engine migration

Accepted2026-10-07: migrate the application engine, CLI/API/MCP, storage/vault and process ownership to Rust, retain Tauri, and type the existing React UI with TypeScript. Browser SDK-specific adapters may temporarily use Python/Node because official Playwright has no Rust binding and stable Camoufox compatibility is not established by its prerelease TypeScript SDK.

Keep the original implementation deployable until explicit owner approval to remove it. Do not change default execution until every required capability is implemented and parity, real Windows/Linux four-engine behavior, source-fixture drift, representative replay, no measurable time/memory regression and rollback are verified. This deliberately costs a temporary second implementation; a whole-engine Python forwarding facade, partial port or successful static build is not a completed Rust migration.
