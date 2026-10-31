#!/usr/bin/env node
// Kept for older instructions. Same as: node scripts/agents.mjs next <lane>
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const cli = fileURLToPath(new URL("./agents.mjs", import.meta.url));
const r = spawnSync(process.execPath, [cli, "next", ...process.argv.slice(2)], {
  stdio: "inherit",
});
process.exit(r.status ?? 1);
