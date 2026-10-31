#!/usr/bin/env node
// Picks the next slice for an agent from docs/tasks/BOARD.md.
// Usage: node scripts/next-task.mjs <agent-lane>   (e.g. claude, antigravity)
import { readFileSync } from "node:fs";

const lane = (process.argv[2] || "any").toLowerCase();
const board = readFileSync(
  new URL("../docs/tasks/BOARD.md", import.meta.url),
  "utf8",
);

const rows = board
  .split("\n")
  .filter((l) => /^\|\s*[A-Z0-9]+\.\d+\s*\|/.test(l))
  .map((l) => {
    const c = l
      .split("|")
      .slice(1, -1)
      .map((s) => s.trim());
    const list = (s) =>
      s && s !== "-"
        ? s
            .split(",")
            .map((x) => x.trim())
            .filter(Boolean)
        : [];
    return {
      id: c[0],
      title: c[1],
      status: c[2].toUpperCase(),
      lane: (c[6] || "any").toLowerCase(),
      depends: list(c[7]),
      owns: list(c[8]),
    };
  });

const done = new Set(
  rows.filter((r) => r.status.startsWith("DONE")).map((r) => r.id),
);
const busyPaths = rows
  .filter(
    (r) =>
      r.status.startsWith("IN PROGRESS") || r.status.startsWith("IN REVIEW"),
  )
  .flatMap((r) => r.owns);
const overlaps = (a, b) => a.startsWith(b) || b.startsWith(a);

const ready = rows.filter(
  (r) =>
    r.status === "TODO" &&
    r.depends.every((d) => done.has(d)) &&
    !r.owns.some((p) => busyPaths.some((b) => overlaps(p, b))),
);

const pick =
  ready.find((r) => r.lane === lane) ||
  ready.find((r) => r.lane === "any") ||
  ready[0];
if (!pick) {
  const waiting = rows
    .filter((r) => r.status === "TODO")
    .map(
      (r) =>
        `${r.id} (waits on ${r.depends.filter((d) => !done.has(d)).join(", ") || "file overlap"})`,
    );
  console.log(
    `No slice ready for "${lane}". Waiting: ${waiting.join("; ") || "none"}`,
  );
  process.exit(1);
}
console.log(
  `NEXT ${pick.id}: ${pick.title}\nLane: ${pick.lane}\nOwns: ${pick.owns.join(", ")}\nClaim it on BOARD.md (status IN PROGRESS, your name, branch phase-${pick.id.split(".")[0]}/${pick.id}-<short>), post in INBOX.md, then start.`,
);
