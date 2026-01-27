#!/bin/bash

# Test script for ERGORS storage and querying functionality (macOS compatible)
BASE_URL="http://localhost:8080"

echo "🧪 Testing ERGORS Storage and Query Functionality"
echo "================================================"

# Test 1: Submit a simple prompt
echo ""
echo "📝 Test 1: Simple prompt submission"
response1=$(curl -s -X POST "$BASE_URL/api/prompt" \
  -H "Content-Type: application/json" \
  -d '{"prompt": "What is 2+2?"}')

echo "Response: $response1"
prompt_id1=$(echo "$response1" | grep -o '"id":"[^"]*"' | cut -d'"' -f4)
echo "Extracted ID: $prompt_id1"

# Test 2: Submit a prompt with context
echo ""
echo "📝 Test 2: Prompt with session context"
response2=$(curl -s -X POST "$BASE_URL/api/prompt" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "What is the capital of France?",
    "context": {
      "session_id": "test-session-123",
      "user_id": "test-user"
    }
  }')

echo "Response: $response2"
prompt_id2=$(echo "$response2" | grep -o '"id":"[^"]*"' | cut -d'"' -f4)
echo "Extracted ID: $prompt_id2"

# Test 3: Submit another prompt with different context
echo ""
echo "📝 Test 3: Prompt with different session context"
response3=$(curl -s -X POST "$BASE_URL/api/prompt" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Explain quantum computing",
    "context": {
      "session_id": "test-session-456",
      "user_id": "test-user"
    }
  }')

echo "Response: $response3"

# Wait a moment for storage to complete
sleep 3

# Test 4: Query all prompts
echo ""
echo "🔍 Test 4: Query all prompts (limit 10)"
all_prompts=$(curl -s "$BASE_URL/api/prompts?limit=10")
echo "All prompts: $all_prompts"

# Test 5: Query by session ID
echo ""
echo "🔍 Test 5: Query by session_id=test-session-123"
session_prompts=$(curl -s "$BASE_URL/api/prompts?session_id=test-session-123")
echo "Session prompts: $session_prompts"

# Test 6: Query by user ID
echo ""
echo "🔍 Test 6: Query by user_id=test-user"
user_prompts=$(curl -s "$BASE_URL/api/prompts?user_id=test-user")
echo "User prompts: $user_prompts"

# Test 7: Query with time filter (last hour) - macOS compatible
echo ""
echo "🔍 Test 7: Query with time filter (last hour)"
start_time=$(date -u -v-1H '+%Y-%m-%dT%H:%M:%SZ')
time_prompts=$(curl -s "$BASE_URL/api/prompts?start_time=$start_time&limit=5")
echo "Time filtered prompts: $time_prompts"

# Test 8: Health check
echo ""
echo "🏥 Test 8: Health check"
health=$(curl -s "$BASE_URL/health")
echo "Health: $health"

echo ""
echo "✅ Testing complete!"
echo "Check the server logs for storage debug information."