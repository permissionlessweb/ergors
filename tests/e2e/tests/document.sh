#!/bin/bash
# E2E tests for document ingestion (non-RAG)

# Source common libraries
source "${SCRIPT_DIR}/lib/common.sh"
source "${SCRIPT_DIR}/lib/ergors.sh"

# Test suite entry point
run_document_tests() {
    log_step "Document Ingestion Tests"

    # Basic document operations
    test_document_ingest_small_file || return 1
    test_document_retrieve || return 1
    test_document_list || return 1
    test_document_delete || return 1

    # Idempotency
    test_document_ingest_idempotent || return 1

    # Large files (>4MB gRPC limit)
    test_document_ingest_large_file || return 1

    # GitHub integration
    test_document_ingest_github_repo || return 1

    # Error handling
    test_document_ingest_file_not_found || return 1
    test_document_get_not_found || return 1

    # Stdin ingestion
    test_document_ingest_stdin || return 1

    log_success "Document ingestion tests passed"
}

# Test: Ingest small text file
test_document_ingest_small_file() {
    log_step "Ingest small text file"

    # Create test file (1KB)
    local test_file="${TEST_DIR}/small_doc.txt"
    cat > "$test_file" <<EOF
# Test Document

This is a small test document for verifying basic document ingestion.

## Features
- Direct storage (no chunking)
- Content hash verification
- Metadata tracking

## Content
Lorem ipsum dolor sit amet, consectetur adipiscing elit.
EOF

    # Ingest document
    local output
    output=$(ergors document ingest "$test_file" 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        log_error "Failed to ingest document"
        log_error "Output: $output"
        return 1
    fi

    # Extract DocumentId from output
    # Expected format: "Document ingested: <id>"
    if ! echo "$output" | grep -q "Document ingested:"; then
        log_error "Output does not contain 'Document ingested:'"
        log_error "Output: $output"
        return 1
    fi

    # Store DocumentId for later tests
    DOCUMENT_ID=$(echo "$output" | sed -n 's/.*Document ingested: \([a-f0-9]*\).*/\1/p')

    if [[ -z "$DOCUMENT_ID" ]]; then
        log_error "Failed to extract DocumentId from output"
        log_error "Output: $output"
        return 1
    fi

    log_success "Small file ingested: $DOCUMENT_ID"
    return 0
}

# Test: Retrieve ingested document
test_document_retrieve() {
    log_step "Retrieve ingested document"

    if [[ -z "$DOCUMENT_ID" ]]; then
        log_error "No DOCUMENT_ID set (run ingest test first)"
        return 1
    fi

    # Retrieve document
    local output
    output=$(ergors document get "$DOCUMENT_ID" 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        log_error "Failed to retrieve document"
        log_error "Output: $output"
        return 1
    fi

    # Verify content contains expected text
    if ! echo "$output" | grep -q "Test Document"; then
        log_error "Retrieved content does not match original"
        log_error "Output: $output"
        return 1
    fi

    if ! echo "$output" | grep -q "Lorem ipsum"; then
        log_error "Retrieved content incomplete"
        log_error "Output: $output"
        return 1
    fi

    log_success "Document retrieved successfully"
    return 0
}

# Test: List documents
test_document_list() {
    log_step "List documents"

    local output
    output=$(ergors document list 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        log_error "Failed to list documents"
        log_error "Output: $output"
        return 1
    fi

    # Verify our document is in the list
    if [[ -n "$DOCUMENT_ID" ]] && ! echo "$output" | grep -q "$DOCUMENT_ID"; then
        log_error "Document $DOCUMENT_ID not found in list"
        log_error "Output: $output"
        return 1
    fi

    # Verify output contains metadata
    if ! echo "$output" | grep -qE "(Name|Source|Timestamp|Hash)"; then
        log_error "Output missing metadata headers"
        log_error "Output: $output"
        return 1
    fi

    log_success "Documents listed successfully"
    return 0
}

# Test: Delete document
test_document_delete() {
    log_step "Delete document"

    if [[ -z "$DOCUMENT_ID" ]]; then
        log_error "No DOCUMENT_ID set"
        return 1
    fi

    # Delete document (use coordinator home for custody verification)
    local output
    local coord_home="${TEST_DIR}/coordinator"
    output=$(ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ergors --home "$coord_home" document delete "$DOCUMENT_ID" --yes 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        log_error "Failed to delete document"
        log_error "Output: $output"
        return 1
    fi

    # Verify success message
    if ! echo "$output" | grep -q "Document deleted"; then
        log_error "No success message in output"
        log_error "Output: $output"
        return 1
    fi

    # Verify document no longer exists
    local get_output
    get_output=$(ergors document get "$DOCUMENT_ID" 2>&1)
    local get_exit=$?

    if [[ $get_exit -eq 0 ]]; then
        log_error "Document still exists after deletion"
        return 1
    fi

    if ! echo "$get_output" | grep -q "Document not found"; then
        log_error "Expected 'Document not found' error"
        log_error "Output: $get_output"
        return 1
    fi

    log_success "Document deleted successfully"
    return 0
}

# Test: Idempotent ingestion (same content = same ID)
test_document_ingest_idempotent() {
    log_step "Idempotent document ingestion"

    # Create test file
    local test_file="${TEST_DIR}/idempotent_doc.txt"
    echo "Idempotent test content" > "$test_file"

    # Ingest first time
    local output1
    output1=$(ergors document ingest "$test_file" 2>&1)
    local id1
    id1=$(echo "$output1" | sed -n 's/.*Document ingested: \([a-f0-9]*\).*/\1/p')

    if [[ -z "$id1" ]]; then
        log_error "Failed to ingest document first time"
        log_error "Output: $output1"
        return 1
    fi

    # Ingest second time (same content)
    local output2
    output2=$(ergors document ingest "$test_file" 2>&1)
    local id2
    id2=$(echo "$output2" | sed -n 's/.*Document ingested: \([a-f0-9]*\).*/\1/p')

    if [[ -z "$id2" ]]; then
        log_error "Failed to ingest document second time"
        log_error "Output: $output2"
        return 1
    fi

    # Verify same ID
    if [[ "$id1" != "$id2" ]]; then
        log_error "Ingestion not idempotent: $id1 != $id2"
        return 1
    fi

    log_success "Idempotent ingestion verified: $id1"

    # Cleanup
    local coord_home="${TEST_DIR}/coordinator"
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ergors --home "$coord_home" document delete "$id1" --yes &>/dev/null
    return 0
}

# Test: Ingest large file (>4MB gRPC limit)
test_document_ingest_large_file() {
    log_step "Ingest large file (>4MB)"

    # Create 7MB test file (exceeds 4MB gRPC limit)
    local test_file="${TEST_DIR}/large_doc.txt"
    log "Generating 7MB test file..."

    {
        echo "# Large Test Document"
        echo ""
        # Generate ~7MB of content
        for i in {1..100000}; do
            echo "Line $i: Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt."
        done
    } > "$test_file"

    local file_size
    file_size=$(stat -f%z "$test_file" 2>/dev/null || stat -c%s "$test_file" 2>/dev/null)
    log "File size: $((file_size / 1024 / 1024))MB"

    if [[ $file_size -lt 7000000 ]]; then
        log_error "Test file too small: ${file_size} bytes"
        return 1
    fi

    # Ingest large document
    log "Ingesting large document..."
    local output
    output=$(ergors document ingest "$test_file" 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        log_error "Failed to ingest large document"
        log_error "Output: $output"
        return 1
    fi

    # Extract DocumentId
    local large_doc_id
    large_doc_id=$(echo "$output" | sed -n 's/.*Document ingested: \([a-f0-9]*\).*/\1/p')

    if [[ -z "$large_doc_id" ]]; then
        log_error "Failed to extract DocumentId"
        log_error "Output: $output"
        return 1
    fi

    log_success "Large file ingested: $large_doc_id"

    # Verify retrieval works
    log "Retrieving large document..."
    local retrieve_output
    retrieve_output=$(ergors document get "$large_doc_id" 2>&1)
    local retrieve_exit=$?

    if [[ $retrieve_exit -ne 0 ]]; then
        log_error "Failed to retrieve large document"
        log_error "Output: $retrieve_output"
        return 1
    fi

    # Verify content starts correctly
    if ! echo "$retrieve_output" | head -5 | grep -q "Large Test Document"; then
        log_error "Retrieved content does not match"
        return 1
    fi

    log_success "Large file retrieved successfully"

    # Cleanup
    local coord_home="${TEST_DIR}/coordinator"
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ergors --home "$coord_home" document delete "$large_doc_id" --yes &>/dev/null
    return 0
}

# Test: Ingest GitHub repository
test_document_ingest_github_repo() {
    log_step "Ingest GitHub repository"

    # Use a small, well-known public repo
    local repo_url="https://github.com/commonwarexyz/monorepo"

    log "Ingesting GitHub repo: $repo_url"
    log "(This may take a minute...)"

    local output
    output=$(ergors document ingest --github "$repo_url" 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        log_error "Failed to ingest GitHub repo"
        log_error "Output: $output"
        return 1
    fi

    # Extract DocumentId
    local repo_doc_id
    repo_doc_id=$(echo "$output" | sed -n 's/.*Document ingested: \([a-f0-9]*\).*/\1/p')

    if [[ -z "$repo_doc_id" ]]; then
        log_error "Failed to extract DocumentId"
        log_error "Output: $output"
        return 1
    fi

    log_success "GitHub repo ingested: $repo_doc_id"

    # Verify retrieval works
    local retrieve_output
    retrieve_output=$(ergors document get "$repo_doc_id" 2>&1)
    local retrieve_exit=$?

    if [[ $retrieve_exit -ne 0 ]]; then
        log_error "Failed to retrieve GitHub document"
        log_error "Output: $retrieve_output"
        return 1
    fi

    # Verify content contains repo info (githem curates README, etc.)
    if ! echo "$retrieve_output" | grep -qiE "(README|repository|github)"; then
        log_error "Retrieved content does not appear to be repo documentation"
        return 1
    fi

    log_success "GitHub repo document retrieved successfully"

    # Cleanup
    local coord_home="${TEST_DIR}/coordinator"
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ergors --home "$coord_home" document delete "$repo_doc_id" --yes &>/dev/null
    return 0
}

# Test: File not found error
test_document_ingest_file_not_found() {
    log_step "Ingest non-existent file (error handling)"

    local output
    output=$(ergors document ingest "/nonexistent/path/to/file.txt" 2>&1)
    local exit_code=$?

    if [[ $exit_code -eq 0 ]]; then
        log_error "Expected non-zero exit code for missing file"
        return 1
    fi

    if ! echo "$output" | grep -qiE "(not found|no such file)"; then
        log_error "Expected 'not found' error message"
        log_error "Output: $output"
        return 1
    fi

    log_success "File not found error handled correctly"
    return 0
}

# Test: Document not found error
test_document_get_not_found() {
    log_step "Retrieve non-existent document (error handling)"

    local fake_id="0000000000000000000000000000000000000000000000000000000000000000"
    local output
    output=$(ergors document get "$fake_id" 2>&1)
    local exit_code=$?

    if [[ $exit_code -eq 0 ]]; then
        log_error "Expected non-zero exit code for missing document"
        return 1
    fi

    if ! echo "$output" | grep -q "Document not found"; then
        log_error "Expected 'Document not found' error message"
        log_error "Output: $output"
        return 1
    fi

    log_success "Document not found error handled correctly"
    return 0
}

# Test: Stdin ingestion
test_document_ingest_stdin() {
    log_step "Ingest from stdin"

    local test_content="This is content from stdin for testing document ingestion"

    local output
    output=$(echo "$test_content" | ergors document ingest --stdin --name "stdin-test" 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        log_error "Failed to ingest from stdin"
        log_error "Output: $output"
        return 1
    fi

    # Extract DocumentId
    local stdin_doc_id
    stdin_doc_id=$(echo "$output" | sed -n 's/.*Document ingested: \([a-f0-9]*\).*/\1/p')

    if [[ -z "$stdin_doc_id" ]]; then
        log_error "Failed to extract DocumentId"
        log_error "Output: $output"
        return 1
    fi

    log_success "Stdin content ingested: $stdin_doc_id"

    # Verify retrieval
    local retrieve_output
    retrieve_output=$(ergors document get "$stdin_doc_id" 2>&1)

    if ! echo "$retrieve_output" | grep -q "$test_content"; then
        log_error "Retrieved content does not match stdin input"
        log_error "Expected: $test_content"
        log_error "Got: $retrieve_output"
        return 1
    fi

    log_success "Stdin document retrieved successfully"

    # Cleanup
    local coord_home="${TEST_DIR}/coordinator"
    ERGORS_CUSTODY_PASSWORD="${TEST_CUSTODY_PASSWORD}" \
        ergors --home "$coord_home" document delete "$stdin_doc_id" --yes &>/dev/null
    return 0
}

# Global test state
DOCUMENT_ID=""
