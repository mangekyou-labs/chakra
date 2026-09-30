// Regenerate the current four-milestone Chakra deck.
// Requires python3 and python-pptx; outputs editable PowerPoint.
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const result = spawnSync("python3", [path.join(__dirname, "build-deck.py")], { stdio: "inherit" });
if (result.error) throw result.error;
process.exit(result.status ?? 1);
