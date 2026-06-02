#!/usr/bin/env bash
# Demonstration: replay the proteon SASA release-tier claim end-to-end.
#
# Prerequisites:
#  1. proteon's source checked out at /scratch/TMAlign/proteon
#  2. proteon's docker image built locally:
#       docker build -f /scratch/TMAlign/proteon/evident/Dockerfile \
#                    -t proteon-evident:latest \
#                    /scratch/TMAlign/proteon
#  3. typed-trust binary built (cargo build in typed-trust/)
#  4. evident-agent installed (pip install -e .)
#
# What this script does:
#  - Invokes evident-agent's replay subcommand against one claim.
#  - The agent calls `docker run proteon-evident:latest replay <claim>`,
#    which runs the claim's evidence.command inside the container.
#  - On success, the agent runs proteon's claim_scoring.py locally on
#    the artifact to extract the primary observed value.
#  - Writes a sidecar entry in workflow/evident.py's last_verified.json
#    convention.
#  - Re-invokes typed-trust with --last-verified-sidecar to render an
#    HTML report with the populated observation.

set -euo pipefail

MANIFEST="/scratch/TMAlign/proteon/evident/claims/sasa.yaml"
CLAIM="proteon-sasa-vs-biopython-ci"   # CI tier, ~12s, deterministic
SIDECAR="/scratch/TMAlign/proteon/evident/last_verified.json"
REPORT="${OUTPUT_DIR:-/tmp}/sasa-ci-report.html"

evident-agent replay \
    --manifest "$MANIFEST" \
    --claim "$CLAIM" \
    --image proteon-evident:latest \
    --sidecar "$SIDECAR" \
    --budget 120 \
    --render html \
    > "$REPORT"

echo
echo "Report written to: $REPORT"
echo "Sidecar written to: $SIDECAR"
echo
echo "Open the report in a browser:"
echo "  xdg-open $REPORT      # Linux"
echo "  open $REPORT          # macOS"
