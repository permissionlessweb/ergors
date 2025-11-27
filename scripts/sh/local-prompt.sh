#!/bin/bash

# Usage: ./script.sh <path_to_txt_file> [prompt]
# Reads a text file and sends its content as context to the API

if [ $# -lt 1 ]; then
    echo "Usage: $0 <text_file> [prompt]"
    exit 1
fi

TEXT_FILE="$1"
# PROMPT="${2:-Analyze the provided context}"
OUTPUT_FILE="${2:-prompt-res.md}"
if [ ! -f "$TEXT_FILE" ]; then
    echo "Error: Text file '$TEXT_FILE' not found."
    exit 1
fi

if [ ! -s "$TEXT_FILE" ]; then
    echo "Error: Text file is empty."
    exit 1
fi

# Read the raw text file and use jq to properly escape it as a JSON string
# The -Rs option reads the entire file as a raw string and escapes all special characters
# This handles newlines, quotes, backslashes, and control characters
CONTENT=$(jq -Rs . < "$TEXT_FILE")

# Construct JSON payload
# CONTENT is already a properly escaped JSON string (with quotes), so we use it directly
JSON_PAYLOAD=$(jq -n \
    --arg prompt "$CONTENT" \
    '{"messages":[{"role":"user","content":$prompt}],"model":"grok-code-fast-1","context":null,"llm_config":null}')

# Make the curl request and capture the response
RESPONSE=$(curl -s -X POST http://localhost:8080/api/prompt \
     -H "Content-Type: application/json" \
     -d "$JSON_PAYLOAD")

# Extract the response content from the JSON structure
# Assuming the response structure is something like {"response": "content"} or {"data": {"response": "content"}}
# Adjust the jq path based on your actual API response structure
MARKDOWN_CONTENT=$(echo "$RESPONSE" | jq -r '.response // .data.response // .content // .data.content // .')

# Check if extraction was successful
if [ -z "$MARKDOWN_CONTENT" ] || [ "$MARKDOWN_CONTENT" = "null" ]; then
    echo "Error: Could not extract response from API"
    echo "Raw response:"
    echo "$RESPONSE"
    exit 1
fi

# Write the markdown content to the output file
echo "$MARKDOWN_CONTENT" > "$OUTPUT_FILE"

echo "Response saved to $OUTPUT_FILE"