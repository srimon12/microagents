#!/bin/bash
CRATE=$1
VERSION=$2
MAX_ATTEMPTS=20

for i in $(seq 1 $MAX_ATTEMPTS); do
  STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "User-Agent: microagents-ci/1.0" \
    "https://crates.io/api/v1/crates/$CRATE/$VERSION")

  if [ "$STATUS" = "200" ]; then
    echo "✓ $CRATE $VERSION is live"
    exit 0
  fi

  echo "Attempt $i/$MAX_ATTEMPTS — not yet indexed, waiting 10s..."
  sleep 10
done

echo "✗ Timed out waiting for $CRATE $VERSION"
exit 1
