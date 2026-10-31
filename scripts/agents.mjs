#!/usr/bin/env node
// Multi-AI coordination CLI. Works for any AI (Claude, Antigravity, Codex, Cursor, Gemini).
// Usage:
//   node scripts/agents.mjs status
//   node scripts/agents.mjs next <lane>
//   node scripts/agents.mjs claim <slice> <agent> [branch]
//   node scripts/agents.mjs heartbeat <slice> <agent>
//   node scripts/agents.mjs review <slice> <agent> [pr-url]
//   node scripts/agents.mjs done <slice> [pr-url]
//   node scripts/agents.mjs block <slice> <agent> <reason...>
//   node scripts/agents.mjs release-stale [hours=2]
//   node scripts/agents.mjs lock <agent> | unlock <agent>
//   node scripts/agents.mjs say <from> <to> <message...>
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  rmSync,
  existsSync,
  statSync,
} from "node:fs";

const ROOT = new URL("..", import.meta.url).pathname;
const BOARD = process.env.BOARD || `${ROOT}docs/tasks/BOARD.md`;
const INBOX = process.env.INBOX || `${ROOT}docs/tasks/INBOX.md`;
const BOARD_LOCK = `${ROOT}.git/agents-board.lock`;
const GIT_LOCK = `${ROOT}.git/agents-git.lock`;
const now = () => new Date().toISOString().slice(0, 16).replace("T", " ");

function withBoardLock(fn) {
  // mkdir is atomic: only one process can create the directory.
  for (let i = 0; i < 50; i++) {
    try {
      mkdirSync(BOARD_LOCK);
      try {
        return fn();
      } finally {
        rmSync(BOARD_LOCK, { recursive: true, force: true });
      }
    } catch (e) {
      if (e.code !== "EEXIST") throw e;
      if (Date.now() - statSync(BOARD_LOCK).mtimeMs > 30000)
        rmSync(BOARD_LOCK, { recursive: true, force: true });
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100);
    }
  }
  throw new Error("Board is locked by another agent; try again.");
}

const isRow = (l) => /^\|\s*[A-Z0-9]+\.\d+\s*\|/.test(l);
const list = (s) =>
  s && s !== "-"
    ? s
        .split(",")
        .map((x) => x.trim())
        .filter(Boolean)
    : [];
function parse(text) {
  return text.split("\n").map((line) => {
    if (!isRow(line)) return { line };
    const c = line
      .split("|")
      .slice(1, -1)
      .map((s) => s.trim());
    while (c.length < 9) c.push("");
    return {
      line,
      c,
      id: c[0],
      status: c[2].toUpperCase(),
      lane: (c[6] || "any").toLowerCase(),
      depends: list(c[7]),
      owns: list(c[8]),
    };
  });
}
const render = (rows) =>
  rows.map((r) => (r.c ? `| ${r.c.join(" | ")} |` : r.line)).join("\n");
const load = () => parse(readFileSync(BOARD, "utf8"));
const save = (rows) => writeFileSync(BOARD, render(rows));
const slices = (rows) => rows.filter((r) => r.c);
const find = (rows, id) => {
  const r = slices(rows).find((x) => x.id === id);
  if (!r) throw new Error(`No slice ${id} on the board.`);
  return r;
};
const set = (r, status, owner, branch) => {
  r.c[2] = status;
  if (owner !== undefined) r.c[3] = owner;
  if (branch !== undefined) r.c[4] = branch;
  r.c[5] = now();
};
const overlaps = (a, b) => a.startsWith(b) || b.startsWith(a);

function ready(rows) {
  const all = slices(rows);
  const done = new Set(
    all.filter((r) => r.c[2].toUpperCase().startsWith("DONE")).map((r) => r.id),
  );
  const busy = all
    .filter((r) => /^(IN PROGRESS|IN REVIEW)/.test(r.c[2].toUpperCase()))
    .flatMap((r) => r.owns);
  return {
    done,
    list: all.filter(
      (r) =>
        r.c[2].toUpperCase() === "TODO" &&
        r.depends.every((d) => done.has(d)) &&
        !r.owns.some((p) => busy.some((b) => overlaps(p, b))),
    ),
  };
}
function pickFor(rows, lane) {
  const { list: rdy } = ready(rows);
  return (
    rdy.find((r) => r.lane === lane) ||
    rdy.find((r) => r.lane === "any") ||
    rdy[0]
  );
}
function say(from, to, msg) {
  const text = readFileSync(INBOX, "utf8");
  const line = `- [${now()}] ${from} -> ${to}: ${msg}`;
  writeFileSync(
    INBOX,
    text.includes("## Messages\n")
      ? text.replace("## Messages\n", `## Messages\n${line}\n`)
      : `${text}\n${line}\n`,
  );
}

const [cmd, ...a] = process.argv.slice(2);
try {
  switch (cmd) {
    case "status": {
      const rows = load();
      for (const r of slices(rows))
        console.log(
          `${r.id.padEnd(5)} ${r.c[2].padEnd(12)} ${(r.c[3] || "-").padEnd(24)} ${r.c[5] || ""}  ${r.c[1]}`,
        );
      console.log(
        existsSync(GIT_LOCK)
          ? `GIT LOCK held by ${readFileSync(`${GIT_LOCK}/owner`, "utf8")}`
          : "GIT LOCK free",
      );
      break;
    }
    case "next": {
      const lane = (a[0] || "any").toLowerCase();
      const rows = load();
      const p = pickFor(rows, lane);
      if (!p) {
        const { done } = ready(rows);
        const waiting = slices(rows)
          .filter((r) => r.c[2].toUpperCase() === "TODO")
          .map(
            (r) =>
              `${r.id} (waits on ${r.depends.filter((d) => !done.has(d)).join(", ") || "file overlap"})`,
          );
        console.log(
          `IDLE: nothing ready for "${lane}". Waiting: ${waiting.join("; ") || "none"}`,
        );
        process.exit(2);
      }
      console.log(
        `NEXT ${p.id}: ${p.c[1]}\nOwns: ${p.owns.join(", ")}\nRun: node scripts/agents.mjs claim ${p.id} <your-agent-name>`,
      );
      break;
    }
    case "claim": {
      const [id, agent, branch] = a;
      withBoardLock(() => {
        const rows = load();
        const r = find(rows, id);
        if (r.c[2].toUpperCase() !== "TODO")
          throw new Error(
            `${id} is ${r.c[2]} (owner ${r.c[3]}). Run "next" again.`,
          );
        if (!ready(rows).list.includes(r))
          throw new Error(`${id} is not ready (dependencies or file overlap).`);
        set(
          r,
          "IN PROGRESS",
          agent,
          branch || `phase-${id.split(".")[0]}/${id}`,
        );
        save(rows);
      });
      say(agent, "all", `claimed ${id}`);
      console.log(
        `CLAIMED ${id} for ${agent}. Send "heartbeat" at least every hour while working.`,
      );
      break;
    }
    case "heartbeat": {
      const [id, agent] = a;
      withBoardLock(() => {
        const rows = load();
        const r = find(rows, id);
        if (r.c[3] !== agent)
          throw new Error(`${id} is owned by ${r.c[3]}, not ${agent}.`);
        r.c[5] = now();
        save(rows);
      });
      console.log(`OK ${id}`);
      break;
    }
    case "review":
    case "done":
    case "block": {
      const [id, agentOrPr, ...rest] = a;
      withBoardLock(() => {
        const rows = load();
        const r = find(rows, id);
        if (cmd === "review")
          set(r, `IN REVIEW${rest[0] ? ` ${rest[0]}` : ""}`);
        if (cmd === "done") set(r, `DONE${agentOrPr ? ` ${agentOrPr}` : ""}`);
        if (cmd === "block") set(r, `BLOCKED (${rest.join(" ")})`);
        save(rows);
      });
      say(
        cmd === "done" ? "main" : agentOrPr,
        "all",
        `${id} ${cmd.toUpperCase()}${rest.length ? `: ${rest.join(" ")}` : ""}`,
      );
      console.log(`${id} -> ${cmd.toUpperCase()}`);
      break;
    }
    case "release-stale": {
      const hours = Number(a[0] || 2);
      const released = [];
      withBoardLock(() => {
        const rows = load();
        for (const r of slices(rows)) {
          const t = Date.parse(`${r.c[5]?.replace(" ", "T")}Z`);
          if (
            r.c[2].toUpperCase().startsWith("IN PROGRESS") &&
            !Number.isNaN(t) &&
            Date.now() - t > hours * 3600e3
          ) {
            released.push(`${r.id} (was ${r.c[3]})`);
            set(r, "TODO", "", "");
          }
        }
        save(rows);
      });
      if (released.length)
        say("main", "all", `released stale claims: ${released.join(", ")}`);
      console.log(
        released.length
          ? `RELEASED ${released.join(", ")}`
          : "No stale claims.",
      );
      break;
    }
    case "lock": {
      try {
        mkdirSync(GIT_LOCK);
        writeFileSync(`${GIT_LOCK}/owner`, `${a[0]} at ${now()}`);
        console.log(
          "GIT LOCK acquired. Run unlock when your checkout/pull/stash is finished.",
        );
      } catch {
        console.log(
          `GIT LOCK busy: ${readFileSync(`${GIT_LOCK}/owner`, "utf8")}`,
        );
        process.exit(3);
      }
      break;
    }
    case "unlock": {
      if (
        existsSync(GIT_LOCK) &&
        !readFileSync(`${GIT_LOCK}/owner`, "utf8").startsWith(a[0])
      )
        throw new Error("Lock is held by another agent.");
      rmSync(GIT_LOCK, { recursive: true, force: true });
      console.log("GIT LOCK released.");
      break;
    }
    case "say": {
      const [from, to, ...msg] = a;
      say(from, to, msg.join(" "));
      console.log("Posted.");
      break;
    }
    default:
      console.log(
        readFileSync(new URL(import.meta.url), "utf8")
          .split("\n")
          .slice(1, 14)
          .join("\n"),
      );
  }
} catch (e) {
  console.error(`ERROR: ${e.message}`);
  process.exit(1);
}
