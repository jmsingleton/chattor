#!/bin/bash
set -e

echo "=== Two-Instance Local Testing ==="
echo ""

# Clean up old test data
echo "Cleaning up old test data..."
rm -rf /tmp/chattor-alice /tmp/chattor-bob

# Build release binary
echo "Building release binary..."
cargo build --release

# Start Alice in background
echo "Starting Alice..."
./target/release/chattor --config-dir /tmp/chattor-alice --debug &
ALICE_PID=$!

# Wait a moment
sleep 2

# Start Bob in background
echo "Starting Bob..."
./target/release/chattor --config-dir /tmp/chattor-bob --debug &
BOB_PID=$!

echo ""
echo "=== Both instances running ==="
echo "Alice PID: $ALICE_PID"
echo "Bob PID: $BOB_PID"
echo ""
echo "Alice data: /tmp/chattor-alice"
echo "Bob data: /tmp/chattor-bob"
echo ""
echo "Press Ctrl+C to stop both instances"
echo ""

# Wait for user interrupt
trap "echo ''; echo 'Stopping instances...'; kill $ALICE_PID $BOB_PID 2>/dev/null; exit 0" INT

# Wait for both processes
wait
