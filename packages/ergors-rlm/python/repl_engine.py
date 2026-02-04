"""
RLM REPL execution engine.

Manages the iterative REPL loop for document exploration using a root LLM.
"""
import re
import io
import sys
from contextlib import redirect_stdout
from typing import List, Dict, Any, Callable, Set, Optional
from RestrictedPython import compile_restricted
from RestrictedPython.Guards import guarded_iter_unpack_sequence, safer_getattr, safe_builtins, safe_globals
from RestrictedPython.PrintCollector import PrintCollector


class ReplEngine:
    """Executes RLM queries using an iterative REPL with a root LLM."""

    def __init__(self, documents: List[Dict[str, Any]], llm_query_fn: Callable[[str, str], str]):
        """
        Initialize REPL engine.

        Args:
            documents: List of Document dicts from Rust
            llm_query_fn: Callback for sub-LLM queries
        """
        self.documents = documents
        self.llm_query_fn = llm_query_fn
        self.iterations = 0
        self.sub_llm_calls = 0
        self.total_cost = 0.0  # TODO: track cost from LLM responses

    def execute(self, query: str, max_iterations: int, max_sub_calls: int) -> Dict[str, Any]:
        """
        Execute RLM query using iterative REPL.

        Returns RlmQueryResponse dict.
        """
        # Load system prompt
        import os
        prompt_path = os.path.join(os.path.dirname(__file__), "prompts", "system.txt")
        with open(prompt_path, 'r') as f:
            system_prompt = f.read().format(
                num_documents=len(self.documents),
                total_length=sum(len(d['content']) for d in self.documents)
            )

        # Conversation history with root LLM
        messages = [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": f"Query: {query}"}
        ]

        final_answer = None
        source_uris: Set[str] = set()

        for i in range(max_iterations):
            self.iterations += 1

            # Call root LLM for next action
            sys.stderr.write(f"RLM: Iteration {i+1}, calling root LLM...\n")
            sys.stderr.flush()
            llm_response = self._call_root_llm(messages)
            sys.stderr.write(f"RLM: Got LLM response: {llm_response[:200]}...\n")
            sys.stderr.flush()

            # Check for FINAL() tag
            if "FINAL(" in llm_response:
                final_answer = self._extract_final(llm_response)
                # Extract sources from the final answer
                source_uris.update(self._extract_sources_from_text(llm_response))
                break

            # Extract and execute code blocks
            code_blocks = self._extract_code_blocks(llm_response)
            execution_output = []

            for code_block in code_blocks:
                try:
                    sys.stderr.write(f"RLM: Executing code block:\n{code_block[:100]}...\n")
                    sys.stderr.flush()
                    # Create fresh globals for each execution to prevent state pollution
                    repl_globals = self._create_safe_globals()
                    # Provide read-only context via tuple (immutable)
                    repl_globals['context'] = tuple(self.documents)
                    repl_globals['llm_query'] = self._make_llm_query_wrapper(max_sub_calls)

                    output = self._execute_code(code_block, repl_globals)
                    execution_output.append(f"[Code executed successfully]\n{output}")
                except Exception as e:
                    sys.stderr.write(f"RLM: Code execution error: {type(e).__name__}: {e}\n")
                    sys.stderr.flush()
                    import traceback
                    traceback.print_exc(file=sys.stderr)
                    execution_output.append(f"[Execution error]\n{str(e)}")

            # Track sources referenced in code
            source_uris.update(self._extract_sources_from_code(llm_response))

            # Add to conversation
            messages.append({"role": "assistant", "content": llm_response})
            messages.append({"role": "user", "content": "\n\n---\n\n".join(execution_output)})

        if final_answer is None:
            final_answer = f"Failed to converge after {max_iterations} iterations. The system was unable to produce a final answer within the iteration limit."

        return {
            "answer": final_answer,
            "source_uris": list(source_uris),
            "iterations": self.iterations,
            "sub_llm_calls": self.sub_llm_calls,
            "cost_usd": self.total_cost,
            "latency_ms": 0  # TODO: track latency
        }

    def _create_safe_globals(self) -> Dict[str, Any]:
        """Create safe global namespace for code execution with RestrictedPython guards."""
        # Start with RestrictedPython 7+ safe_globals which includes proper guards
        safe_dict = safe_globals.copy()

        # Add additional safe builtins not in default
        safe_dict['__builtins__'].update({
            'list': list,
            'dict': dict,
            'set': set,
            'enumerate': enumerate,
            'zip': zip,
            'map': map,
            'filter': filter,
            'sum': sum,
            'min': min,
            'max': max,
            'any': any,
            'all': all,
        })

        # Add iteration guard required for for loops
        safe_dict['_iter_unpack_sequence_'] = guarded_iter_unpack_sequence
        safe_dict['_getiter_'] = iter

        # Add getitem guard for subscript access (dict[key], list[index])
        def safe_getitem(obj, key):
            """Allow subscript access for dicts, lists, tuples."""
            return obj[key]
        safe_dict['_getitem_'] = safe_getitem

        # Add PrintCollector for print() support
        safe_dict['_print_'] = PrintCollector

        # Special function to signal final answer
        safe_dict['FINAL'] = self._final_marker

        return safe_dict

    def _make_llm_query_wrapper(self, max_sub_calls: int) -> Callable:
        """Create wrapped llm_query function with call count tracking."""
        def llm_query(prompt: str, model: str = "claude-3-5-sonnet") -> str:
            if self.sub_llm_calls >= max_sub_calls:
                raise Exception(f"Max sub-LLM calls ({max_sub_calls}) exceeded")
            self.sub_llm_calls += 1
            return self.llm_query_fn(prompt, model)
        return llm_query

    def _call_root_llm(self, messages: List[Dict[str, str]]) -> str:
        """
        Call root LLM via sub-LLM callback with full conversation history.

        The entire conversation is sent to allow the LLM to build context over iterations.
        """
        # Format ALL messages into a single prompt - full conversation history is critical
        prompt = "\n\n".join([
            f"[{msg['role'].upper()}]\n{msg['content']}"
            for msg in messages  # Send FULL conversation history
        ])

        return self.llm_query_fn(prompt, "claude-3-5-sonnet")

    def _execute_code(self, code: str, globals_dict: Dict[str, Any]) -> str:
        """Execute code in restricted environment and capture output."""
        try:
            # Compile with RestrictedPython (may raise SyntaxError in v7+)
            byte_code = compile_restricted(code, '<string>', 'exec')
        except SyntaxError as e:
            # RestrictedPython 7+ raises SyntaxError for compilation issues
            raise Exception(f"RestrictedPython compilation error: {e}")

        # Capture stdout (for any non-print output)
        output = io.StringIO()
        with redirect_stdout(output):
            exec(byte_code, globals_dict)

        # Capture print() output from PrintCollector
        printed_output = ""
        if '_print' in globals_dict:
            # PrintCollector instance is callable and returns collected output
            printed_output = globals_dict['_print']()

        # Combine both outputs
        stdout_output = output.getvalue()
        return printed_output + stdout_output

    def _extract_code_blocks(self, text: str) -> List[str]:
        """Extract ```python ... ``` code blocks from LLM response."""
        pattern = r'```(?:python|py)?\s*\n(.*?)```'
        matches = re.findall(pattern, text, re.DOTALL)
        return matches

    def _extract_final(self, text: str) -> Optional[str]:
        """Extract FINAL(answer) content with proper paren counting for nested parens."""
        # Find FINAL( marker
        match = re.search(r'FINAL\s*\(', text)
        if not match:
            return None

        # Start after opening paren
        start = match.end()
        paren_count = 1
        pos = start

        # Count parens to find matching close
        while pos < len(text) and paren_count > 0:
            if text[pos] == '(':
                paren_count += 1
            elif text[pos] == ')':
                paren_count -= 1
            pos += 1

        if paren_count != 0:
            # Unmatched parens
            return None

        # Extract content between FINAL( and matching )
        content = text[start:pos-1].strip()

        # Strip quotes if present
        if (content.startswith('"') and content.endswith('"')) or \
           (content.startswith("'") and content.endswith("'")):
            content = content[1:-1]

        return content.strip()

    def _extract_sources_from_code(self, code: str) -> Set[str]:
        """Extract context[i] references from code to track sources."""
        sources = set()
        # Look for context[i] patterns
        for match in re.finditer(r'context\[(\d+)\]', code):
            idx = int(match.group(1))
            if 0 <= idx < len(self.documents):
                sources.add(self.documents[idx]['source_uri'])
        return sources

    def _extract_sources_from_text(self, text: str) -> Set[str]:
        """Extract source URIs mentioned in text."""
        sources = set()
        # Look for URIs in the format: discord:guild_XXX/...
        for match in re.finditer(r'(discord:[^\s\]]+)', text):
            sources.add(match.group(1))
        return sources

    def _final_marker(self, answer: str):
        """Marker function for FINAL() - does nothing but is callable."""
        # This is a no-op function that makes FINAL() valid Python
        # The actual extraction happens via regex in _extract_final
        pass
