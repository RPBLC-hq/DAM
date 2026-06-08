#!/usr/bin/env node
"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const ROOT = path.resolve(__dirname, "..", "..");
const WRAPPER = path.join(ROOT, "npm", "bin", "dam.js");
const BINARIES = ["dam", "damctl", "dam-proxy", "dam-web", "dam-mcp", "dam-tray"];

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "dam-npm-smoke-"));
const nativeDir = path.join(tmp, "native");
fs.mkdirSync(nativeDir);
try {
  const env = { ...process.env };
  for (const name of BINARIES) {
    env[`DAM_NATIVE_${name.toUpperCase().replace(/-/g, "_")}`] = fakeNative(name);
  }

  const doctor = run(process.execPath, [WRAPPER, "package-doctor", "--json"], env);
  assertEqual(doctor.status, 0, doctor.stderr || doctor.stdout);
  const report = JSON.parse(doctor.stdout);
  assertEqual(report.state, "ready", doctor.stdout);
  assertEqual(report.package.name, "@rpblc/dam", doctor.stdout);
  for (const name of BINARIES) {
    const row = report.binaries.find((entry) => entry.name === name);
    assert(row && row.ok, `doctor did not find ${name}: ${doctor.stdout}`);
  }

  const dam = run(process.execPath, [WRAPPER, "status", "--json"], env);
  assertEqual(dam.status, 0, dam.stderr || dam.stdout);
  assert(JSON.parse(dam.stdout).binary === "dam", dam.stdout);

  const damctlWrapper = path.join(ROOT, "npm", "bin", "damctl.js");
  const damctl = run(process.execPath, [damctlWrapper, "setup", "plan", "--json"], env);
  assertEqual(damctl.status, 0, damctl.stderr || damctl.stdout);
  const damctlPayload = JSON.parse(damctl.stdout);
  assertEqual(damctlPayload.binary, "damctl", damctl.stdout);
  assertEqual(damctlPayload.args[0], "setup", damctl.stdout);

  process.stdout.write("npm package smoke passed\n");
} finally {
  fs.rmSync(tmp, { recursive: true, force: true });
}

function fakeNative(name) {
  const file = path.join(nativeDir, process.platform === "win32" ? `${name}.cmd` : name);
  if (process.platform === "win32") {
    fs.writeFileSync(
      file,
      `@echo off\r\nnode -e "console.log(JSON.stringify({binary: '${name}', args: process.argv.slice(1)}))" %*\r\n`
    );
  } else {
    fs.writeFileSync(
      file,
      `#!/usr/bin/env node\nconsole.log(JSON.stringify({ binary: ${JSON.stringify(name)}, args: process.argv.slice(2) }));\n`
    );
    fs.chmodSync(file, 0o755);
  }
  return file;
}

function run(command, args, env) {
  return spawnSync(command, args, {
    cwd: ROOT,
    env,
    encoding: "utf8",
  });
}

function assert(value, message) {
  if (!value) {
    throw new Error(message);
  }
}

function assertEqual(actual, expected, message) {
  if (actual !== expected) {
    throw new Error(`${message}\nexpected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}
