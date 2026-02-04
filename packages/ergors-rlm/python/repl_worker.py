#!/usr/bin/env python3
"""
RLM REPL worker subprocess.

Communicates with Rust parent via JSON-RPC over stdin/stdout.
Executes Python code in a sandboxed REPL environment for document exploration.
"""
import sys
import json
import traceback
from typing import Any, Dict, Optional
from repl_engine import ReplEngine


def main():
    """Main loop: read JSON requests from stdin, execute, write JSON responses to stdout."""
    sys.stderr.write("RLM REPL worker starting...\n")
    sys.stderr.flush()

    engine = None

    while True:
        try:
            # Read JSON-RPC request from stdin
            line = sys.stdin.readline()
            if not line:
                break  # EOF, parent closed pipe

            request = json.loads(line)
            method = request.get("method")
            params = request.get("params", {})
            req_id = request.get("id")

            if method == "execute":
                # Execute RLM query
                result = execute_rlm_query(params)
                response = {"jsonrpc": "2.0", "result": result, "id": req_id}
            elif method == "ping":
                response = {"jsonrpc": "2.0", "result": "pong", "id": req_id}
            else:
                response = {
                    "jsonrpc": "2.0",
                    "error": {"code": -32601, "message": f"Method not found: {method}"},
                    "id": req_id
                }

            # Write JSON response to stdout
            sys.stdout.write(json.dumps(response) + "\n")
            sys.stdout.flush()

        except Exception as e:
            error_response = {
                "jsonrpc": "2.0",
                "error": {
                    "code": -32603,
                    "message": str(e),
                    "data": traceback.format_exc()
                },
                "id": req_id if 'req_id' in locals() else None
            }
            sys.stdout.write(json.dumps(error_response) + "\n")
            sys.stdout.flush()


def execute_rlm_query(params: Dict[str, Any]) -> Dict[str, Any]:
    """
    Execute RLM query with documents from parent process.

    Returns RlmQueryResponse as dict.
    """
    query = params["query"]
    documents = params["documents"]  # List of Document dicts from Rust
    max_iterations = params.get("max_iterations", 10)
    max_sub_calls = params.get("max_sub_calls", 50)

    sys.stderr.write(f"RLM: Starting query with {len(documents)} documents\n")
    sys.stderr.flush()

    # Create callback for sub-LLM calls (call back to Rust parent)
    def llm_query_callback(prompt: str, model: str = "claude-3-5-sonnet") -> str:
        """Send sub-LLM request to parent process via JSON-RPC."""
        request = {
            "jsonrpc": "2.0",
            "method": "llm_query",
            "params": {"prompt": prompt, "model": model},
            "id": "sub-llm-call"
        }
        sys.stdout.write(json.dumps(request) + "\n")
        sys.stdout.flush()

        # Read response from parent
        response_line = sys.stdin.readline()
        response = json.loads(response_line)

        if "error" in response:
            raise Exception(f"Sub-LLM error: {response['error']['message']}")

        return response["result"]

    # Initialize REPL engine
    sys.stderr.write("RLM: Initializing REPL engine\n")
    sys.stderr.flush()
    engine = ReplEngine(documents=documents, llm_query_fn=llm_query_callback)

    # Execute query
    sys.stderr.write("RLM: Executing query\n")
    sys.stderr.flush()
    try:
        result = engine.execute(
            query=query,
            max_iterations=max_iterations,
            max_sub_calls=max_sub_calls
        )
        sys.stderr.write(f"RLM: Query completed successfully\n")
        sys.stderr.flush()
        return result
    except Exception as e:
        sys.stderr.write(f"RLM: Query failed with error: {type(e).__name__}: {e}\n")
        sys.stderr.flush()
        raise


if __name__ == "__main__":
    main()
