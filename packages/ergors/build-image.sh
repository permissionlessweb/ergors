#!/usr/bin/env bash
# Build and optionally push ergors Docker image
# Usage: ./build-image.sh [registry] [push]
#   registry: Docker registry (default: ghcr.io/permissionlessweb)
#   push: "push" to push to registry after build

set -euo pipefail

# Configuration
REGISTRY="${1:-ghcr.io/permissionlessweb}"
PUSH_FLAG="${2:-}"
IMAGE_NAME="ergors"

# Get version from git (tag or commit)
GIT_TAG=$(git describe --tags --exact-match 2>/dev/null || echo "")
GIT_COMMIT=$(git rev-parse --short HEAD)
BUILD_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Determine image tag
if [ -n "$GIT_TAG" ]; then
    VERSION="$GIT_TAG"
else
    VERSION="dev-$GIT_COMMIT"
fi

FULL_IMAGE="$REGISTRY/$IMAGE_NAME:$VERSION"
LATEST_IMAGE="$REGISTRY/$IMAGE_NAME:latest"

echo "==============================================="
echo "Building ergors Docker image"
echo "==============================================="
echo "Image:       $FULL_IMAGE"
echo "Latest tag:  $LATEST_IMAGE"
echo "Commit:      $GIT_COMMIT"
echo "Build date:  $BUILD_DATE"
echo "==============================================="

# Build from workspace root (not packages/ergors)
cd "$(git rev-parse --show-toplevel)"

# Build the image
docker build \
    -f packages/ergors/Dockerfile \
    -t "$FULL_IMAGE" \
    -t "$LATEST_IMAGE" \
    --build-arg GIT_COMMIT="$GIT_COMMIT" \
    --build-arg BUILD_DATE="$BUILD_DATE" \
    .

echo "✅ Build complete: $FULL_IMAGE"

# Push if requested
if [ "$PUSH_FLAG" = "push" ]; then
    echo "Pushing to registry..."
    docker push "$FULL_IMAGE"
    docker push "$LATEST_IMAGE"
    echo "✅ Pushed: $FULL_IMAGE"
    echo "✅ Pushed: $LATEST_IMAGE"
else
    echo "ℹ️  Skipping push (use './build-image.sh $REGISTRY push' to push)"
fi

# Output the image tag for use in SDL generation
echo ""
echo "SDL_IMAGE_TAG=$FULL_IMAGE"
