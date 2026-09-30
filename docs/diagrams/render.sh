#!/usr/bin/env bash
# Render docs/diagrams/*.puml to SVG.
# Prefers a PlantUML jar (PLANTUML_JAR or plantuml.jar next to this script),
# then a plantuml CLI, then Docker. Expand the glob on the host: docker does
# not run a shell, so /data/*.puml would be a literal filename.
set -euo pipefail
dir="$(cd "$(dirname "$0")" && pwd)"
shopt -s nullglob
files=("$dir"/*.puml)
if ((${#files[@]} == 0)); then
  echo "no .puml files in $dir" >&2
  exit 1
fi

render_java() {
  local jar="$1"
  java -jar "$jar" -tsvg -charset UTF-8 "${files[@]}"
}

if [[ -n "${PLANTUML_JAR:-}" && -f "${PLANTUML_JAR}" ]]; then
  render_java "$PLANTUML_JAR"
elif [[ -f "$dir/plantuml.jar" ]]; then
  render_java "$dir/plantuml.jar"
elif command -v plantuml >/dev/null 2>&1; then
  plantuml -tsvg -charset UTF-8 "${files[@]}"
elif command -v docker >/dev/null 2>&1; then
  image="${PLANTUML_IMAGE:-plantuml/plantuml:latest}"
  args=()
  for f in "${files[@]}"; do
    args+=("/data/$(basename "$f")")
  done
  docker run --rm \
    -u "$(id -u):$(id -g)" \
    -v "$dir:/data" \
    "$image" \
    -tsvg -charset UTF-8 "${args[@]}"
else
  echo "need PLANTUML_JAR, plantuml CLI, or docker" >&2
  exit 1
fi

echo "rendered ${#files[@]} diagrams in $dir"
for svg in "$dir"/*.svg; do
  if grep -q 'Syntax Error' "$svg"; then
    echo "PlantUML syntax error in $svg" >&2
    exit 1
  fi
done
