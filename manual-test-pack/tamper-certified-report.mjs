#!/usr/bin/env node

import { readFile, writeFile } from "node:fs/promises";
import { basename, dirname, extname, join } from "node:path";

const inputPath = process.argv[2];
if (!inputPath) {
  throw new Error('Usage: node manual-test-pack/tamper-certified-report.mjs "signed-report.json"');
}

const report = JSON.parse(await readFile(inputPath, "utf8"));
const signature = report?.signature?.signature_hex;
if (typeof signature !== "string" || !/^[a-fA-F0-9]{128}$/.test(signature)) {
  throw new Error("Input is not a certified report JSON with a 128-character signature_hex field.");
}

const changedFirstNibble = signature[0] === "0" ? "1" : "0";
report.signature.signature_hex = changedFirstNibble + signature.slice(1);

const extension = extname(inputPath) || ".json";
const outputPath = join(
  dirname(inputPath),
  `${basename(inputPath, extname(inputPath))}.tampered${extension}`,
);
await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`, { flag: "wx" });
process.stdout.write(`Tampered copy written to: ${outputPath}\n`);
