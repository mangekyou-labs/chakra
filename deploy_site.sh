#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/packages/frontend"
npm run build
# npx wrangler pages project create Chakra
npx wrangler pages deploy out --project-name=Chakra --commit-dirty=true
