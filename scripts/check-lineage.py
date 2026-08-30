#!/usr/bin/env python3
"""
Lineage Regression Checker for Chakra (Arc-Only Architecture).

Scans tracked git paths, file contents, package manifests, documentation,
and git commit history for forbidden legacy/lineage terminology.

To prevent self-matching, forbidden terms are reconstructed dynamically at runtime.
"""

import os
import sys
import subprocess
import re
import base64
from pathlib import Path

# Encoded term table. Keep the source free of the guarded vocabulary.
ENCODED_TERMS = [
    "bHVtYWdn",
    "bHVtLWFnZw==",
    "bHVtX2FnZw==",
    "c3RlbGxhcg==",
    "c2RleA==",
    "c3Ryb29w",
    "c29yb2Jhbg==",
    "c29yb3N3YXA=",
    "YXF1YXJpdXM=",
    "cGhvZW5peA==",
    "Y29tZXQ=",
    "ZnJlaWdodGVy",
    "cHJlZmVyX3Nvcm9iYW4=",
]

ENCODED_WORD_TERMS = [
    "eGxt",
]

def decode_terms():
    terms = [base64.b64decode(item).decode("utf-8") for item in ENCODED_TERMS]
    word_terms = [base64.b64decode(item).decode("utf-8") for item in ENCODED_WORD_TERMS]
    return terms, word_terms

TERMS, WORD_TERMS = decode_terms()

# Regex compiled dynamically
SUBSTRING_PATTERN = re.compile(r"(" + "|".join(re.escape(t) for t in TERMS) + r")", re.IGNORECASE)
WORD_PATTERN = re.compile(r"\b(" + "|".join(re.escape(t) for t in WORD_TERMS) + r")\b", re.IGNORECASE)

# The checker must not report its own encoded implementation as a product violation.
IGNORED_PATHS = {
    "scripts/check-lineage.py",
    "scripts/check-lineage.sh",
}

GENERATED_ROOTS = ("packages/sdk/dist", "packages/frontend/.next")
GENERATED_VENDOR_DIRS = {"cache"}
GENERATED_VENDOR_SUFFIXES = {".js", ".map", ".pack", ".tsbuildinfo"}

def find_violations(text: str, source_label: str) -> list:
    violations = []
    for line_num, line in enumerate(text.splitlines(), 1):
        m_sub = SUBSTRING_PATTERN.search(line)
        if m_sub:
            violations.append((source_label, line_num, m_sub.group(0), line.strip()[:120]))
            continue
        m_word = WORD_PATTERN.search(line)
        if m_word:
            violations.append((source_label, line_num, m_word.group(0), line.strip()[:120]))
    return violations

def check_tracked_files(repo_root: Path) -> list:
    violations = []
    try:
        res = subprocess.run(
            ["git", "ls-files"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=True
        )
        files = [f.strip() for f in res.stdout.splitlines() if f.strip()]
    except Exception as e:
        print(f"Error getting git files: {e}", file=sys.stderr)
        return [("git", 0, "git ls-files failed", str(e))]

    for file_rel in files:
        if file_rel in IGNORED_PATHS:
            continue

        # Check path name itself
        m_sub = SUBSTRING_PATTERN.search(file_rel)
        if m_sub:
            violations.append((f"path:{file_rel}", 0, m_sub.group(0), file_rel))

        full_path = repo_root / file_rel
        if not full_path.is_file():
            continue

        try:
            content = full_path.read_bytes().decode("utf-8", errors="ignore")
            v = find_violations(content, file_rel)
            violations.extend(v)
        except Exception as e:
            violations.append((file_rel, 0, "read_error", str(e)))

    tracked = set(files)
    for generated_root in GENERATED_ROOTS:
        root = repo_root / generated_root
        if not root.is_dir():
            continue
        for generated_file in root.rglob("*"):
            if generated_file.is_file():
                rel = generated_file.relative_to(repo_root).as_posix()
                if rel not in tracked:
                    generated_parts = generated_file.relative_to(root).parts
                    if root.name == ".next" and (
                        generated_parts[0] in GENERATED_VENDOR_DIRS
                        or generated_file.name == "trace"
                        or generated_file.suffix in GENERATED_VENDOR_SUFFIXES
                    ):
                        # Bundler caches, traces, and compiled dependency chunks are
                        # not first-party artifacts. Rendered HTML/RSC/CSS/SVG and
                        # the SDK's distributable files remain fully scanned.
                        continue
                    try:
                        content = generated_file.read_bytes().decode("utf-8", errors="ignore")
                        violations.extend(find_violations(content, rel))
                    except Exception as e:
                        violations.append((rel, 0, "read_error", str(e)))

    return violations

def check_commit_metadata(repo_root: Path) -> list:
    violations = []
    try:
        res = subprocess.run(
            ["git", "log", "--format=%H%x09%an%x09%ae%x09%s%x09%b", "--all"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=True
        )
        for line in res.stdout.splitlines():
            if not line.strip():
                continue
            parts = line.split("\t")
            commit_hash = parts[0] if parts else "unknown"
            commit_text = " ".join(parts[1:])
            m_sub = SUBSTRING_PATTERN.search(commit_text)
            if m_sub:
                violations.append((f"commit:{commit_hash[:8]}", 0, m_sub.group(0), commit_text[:100]))
            else:
                m_word = WORD_PATTERN.search(commit_text)
                if m_word:
                    violations.append((f"commit:{commit_hash[:8]}", 0, m_word.group(0), commit_text[:100]))
    except Exception as e:
        print(f"Error checking git log: {e}", file=sys.stderr)
    return violations

def main():
    repo_root = Path(__file__).resolve().parent.parent
    mode = sys.argv[1] if len(sys.argv) > 1 else "all"

    violations = []

    if mode in ("all", "files"):
        violations.extend(check_tracked_files(repo_root))

    if mode in ("all", "commits"):
        violations.extend(check_commit_metadata(repo_root))

    if violations:
        print(f"\n[FAIL] Found {len(violations)} lineage violation(s):")
        for src, line, term, context in violations[:50]:
            if line > 0:
                print(f"  - {src}:{line} (matched '{term}'): {context}")
            else:
                print(f"  - {src} (matched '{term}'): {context}")
        if len(violations) > 50:
            print(f"  ... and {len(violations) - 50} more.")
        sys.exit(1)
    else:
        print("\n[PASS] Lineage check clean: 0 legacy terms found.")
        sys.exit(0)

if __name__ == "__main__":
    main()
