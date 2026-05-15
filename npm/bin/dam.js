#!/usr/bin/env node
"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const ROOT = path.resolve(__dirname, "..", "..");
const NATIVE_DIR = path.join(ROOT, "npm", "native");
const TRIAL_COMMANDS = new Set();
const WRAPPED_BINARIES = new Set([
  "dam",
  "damctl",
  "dam-web",
  "dam-proxy",
  "dam-mcp",
  "dam-tray",
]);

function main() {
  const rawArgs = process.argv.slice(2);
  const invoked = invokedBinaryName();
  if (invoked !== "dam") {
    runNative(invoked, rawArgs);
    return;
  }

  const command = rawArgs[0];

  if (!command || command === "-h" || command === "--help" || command === "help") {
    runNative("dam", rawArgs);
    return;
  }

  if (command === "package-doctor") {
    doctor(rawArgs.slice(1));
    return;
  }

  if (command === "doctor" && rawArgs.includes("--package")) {
    doctor(rawArgs.slice(1).filter((arg) => arg !== "--package"));
    return;
  }

  if (command === "web") {
    runNative("dam-web", rawArgs.slice(1));
    return;
  }

  const flags = splitWrapperFlags(rawArgs);
  if (TRIAL_COMMANDS.has(command) && shouldRunTrial(flags)) {
    runTrial(command, flags.args.slice(1), flags.keep);
    return;
  }

  runNative("dam", flags.args);
}

function invokedBinaryName() {
  if (process.env.DAM_WRAPPER_NAME && WRAPPED_BINARIES.has(process.env.DAM_WRAPPER_NAME)) {
    return process.env.DAM_WRAPPER_NAME;
  }
  const script = path.basename(process.argv[1] || "dam");
  const withoutExtension = script.replace(/\.(?:js|cmd|ps1|exe)$/i, "");
  if (WRAPPED_BINARIES.has(withoutExtension)) {
    return withoutExtension;
  }
  return "dam";
}

function splitWrapperFlags(args) {
  const separator = args.indexOf("--");
  const beforeToolArgs = separator === -1 ? args : args.slice(0, separator);
  const afterToolArgs = separator === -1 ? [] : args.slice(separator);
  const stripped = [];
  let trial = false;
  let persist = false;
  let keep = false;

  for (const arg of beforeToolArgs) {
    if (arg === "--trial") {
      trial = true;
    } else if (arg === "--persist") {
      persist = true;
    } else if (arg === "--keep") {
      keep = true;
    } else {
      stripped.push(arg);
    }
  }

  return {
    args: stripped.concat(afterToolArgs),
    trial,
    persist,
    keep,
  };
}

function shouldRunTrial(flags) {
  if (flags.persist) {
    return false;
  }
  return flags.trial || invokedThroughNpx();
}

function invokedThroughNpx() {
  const probe = `${__dirname}${path.delimiter}${process.argv[1] || ""}`;
  return /(^|[\\/])_npx([\\/]|$)/.test(probe) || /[\\/]npm-cache[\\/]_npx[\\/]/.test(probe);
}

function runTrial(command, args, keep) {
  const trialDir = fs.mkdtempSync(path.join(os.tmpdir(), "dam-trial-"));
  const vaultPath = path.join(trialDir, "vault.db");
  const logPath = path.join(trialDir, "log.db");
  const consentPath = path.join(trialDir, "consent.db");
  const launchArgs = buildTrialArgs(command, args, vaultPath, logPath, consentPath);

  process.stderr.write("DAM trial mode\n\n");
  process.stderr.write(`✓ Vault: ${vaultPath}\n`);
  process.stderr.write(`✓ Logs: ${logPath}\n`);
  process.stderr.write(`✓ Consents: ${consentPath}\n`);
  process.stderr.write(`✓ Keep data: ${keep ? "yes" : "no"}\n\n`);
  process.stderr.write(`Launching ${command} through DAM...\n`);

  const env = {
    ...process.env,
    DAM_CONSENT_PATH: consentPath,
    DAM_CONSENT_SQLITE_PATH: consentPath,
  };
  const result = spawnNative("dam", launchArgs, { env });

  if (!keep) {
    fs.rmSync(trialDir, { force: true, recursive: true });
  } else {
    process.stderr.write(`\nDAM trial data kept at ${trialDir}\n`);
  }

  process.exit(result.status ?? 1);
}

function buildTrialArgs(command, args, vaultPath, logPath, consentPath) {
  const separator = args.indexOf("--");
  const damArgs = separator === -1 ? args.slice() : args.slice(0, separator);
  const toolArgs = separator === -1 ? [] : args.slice(separator);

  ensureOption(damArgs, "--db", vaultPath);
  if (!damArgs.includes("--no-log")) {
    ensureOption(damArgs, "--log", logPath);
  }
  ensureOption(damArgs, "--consent-db", consentPath);

  return [command, ...damArgs, ...toolArgs];
}

function ensureOption(args, option, value) {
  if (args.includes(option)) {
    return;
  }
  args.push(option, value);
}

function doctor(args = []) {
  const json = args.includes("--json");
  const rows = [];
  rows.push(checkBinary("dam"));
  rows.push(checkBinary("damctl"));
  rows.push(checkBinary("dam-proxy"));
  rows.push(checkBinary("dam-web"));
  rows.push(checkBinary("dam-mcp"));
  rows.push(checkBinary("dam-tray"));
  rows.push(checkOnPath("claude", "Claude Code", false));
  rows.push(checkOnPath("codex", "Codex", false));
  const state = rows.every((row) => row.ok || row.required === false)
    ? "ready"
    : "missing_requirements";

  if (json) {
    process.stdout.write(`${JSON.stringify({
      state,
      package: packageInfo(),
      binaries: rows,
    }, null, 2)}\n`);
    process.exit(state === "ready" ? 0 : 1);
  }

  process.stdout.write("DAM doctor\n\n");
  for (const row of rows) {
    process.stdout.write(`${row.ok ? "✓" : "!"} ${row.label}${row.detail ? `: ${row.detail}` : ""}\n`);
  }

  process.exit(state === "ready" ? 0 : 1);
}

function checkBinary(name) {
  try {
    return { ok: true, required: true, name, label: `${name} binary`, detail: resolveNative(name) };
  } catch (error) {
    return { ok: false, required: true, name, label: `${name} binary`, detail: error.message };
  }
}

function checkOnPath(name, label, required = true) {
  const found = findOnPath(nativeName(name));
  return {
    ok: Boolean(found),
    required,
    name,
    label,
    detail: found || "not found on PATH",
  };
}

function packageInfo() {
  let manifest = {};
  try {
    manifest = JSON.parse(fs.readFileSync(path.join(ROOT, "package.json"), "utf8"));
  } catch {
    manifest = {};
  }
  return {
    name: manifest.name || "@rpblc/dam",
    version: manifest.version || "0.0.0",
    platform: process.platform,
    arch: process.arch,
    platform_dir: `${process.platform}-${process.arch}`,
    native_dir: NATIVE_DIR,
  };
}

function runNative(name, args) {
  const result = spawnNative(name, args, { env: process.env });
  process.exit(result.status ?? 1);
}

function spawnNative(name, args, options) {
  const binary = resolveNative(name);
  return spawnSync(binary, args, {
    env: options.env,
    stdio: "inherit",
  });
}

function resolveNative(name) {
  const envKey = `DAM_NATIVE_${name.toUpperCase().replace(/-/g, "_")}`;
  if (process.env[envKey]) {
    return process.env[envKey];
  }

  const platformDir = `${process.platform}-${process.arch}`;
  const bundled = path.join(NATIVE_DIR, platformDir, nativeName(name));
  if (fs.existsSync(bundled)) {
    return bundled;
  }

  for (const buildDir of ["release", "debug"]) {
    const devBinary = path.join(ROOT, "target", buildDir, nativeName(name));
    if (fs.existsSync(devBinary)) {
      return devBinary;
    }
  }

  const pathBinary = findOnPath(nativeName(name));
  if (pathBinary && !isCurrentScript(pathBinary)) {
    return pathBinary;
  }

  throw new Error(
    `missing ${name} native binary for ${platformDir}; expected ${bundled} or set ${envKey}`
  );
}

function nativeName(name) {
  return process.platform === "win32" ? `${name}.exe` : name;
}

function findOnPath(name) {
  const pathValue = process.env.PATH || "";
  for (const dir of pathValue.split(path.delimiter)) {
    if (!dir) {
      continue;
    }
    const candidate = path.join(dir, name);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
}

function isCurrentScript(candidate) {
  try {
    const current = process.argv[1] ? fs.realpathSync(process.argv[1]) : "";
    return fs.realpathSync(candidate) === current;
  } catch {
    return path.resolve(candidate) === path.resolve(process.argv[1] || "");
  }
}

main();
