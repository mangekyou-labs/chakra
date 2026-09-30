#!/usr/bin/env node
// Export the current application text rather than a second hardcoded proposal.
// Requires pandoc.
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
const dir = path.dirname(fileURLToPath(import.meta.url));
const result = spawnSync("pandoc", [path.join(dir, "circle-developer-grant-proposal.md"), "-o", path.join(dir, "Chakra-Circle-Grant-Proposal.docx")], { stdio: "inherit" });
if (result.error) throw result.error;
process.exit(result.status ?? 1);
