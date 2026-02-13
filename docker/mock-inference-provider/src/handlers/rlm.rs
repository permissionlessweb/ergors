use crate::types::ChatMessage;

/// Detect whether this is a root LLM call (has [SYSTEM] tag from repl_engine.py formatting)
/// or a sub-LLM call (plain analysis prompt without [SYSTEM]).
///
/// Root LLM calls have the full conversation history formatted as:
///   [SYSTEM]\n<system_prompt>\n\n[USER]\n<query>\n\n[ASSISTANT]\n<prev_response>...
///
/// Sub-LLM calls are plain text prompts from Python's llm_query() callback.
pub fn rlm_chat_response(messages: &[ChatMessage]) -> Option<String> {
    // The RLM engine sends the entire conversation as a single user message
    // via llm_query_fn(prompt, model). Find the last user message content.
    let last_user_content = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())?;

    if last_user_content.contains("[SYSTEM]") {
        // Root LLM call — count [ASSISTANT] tags to determine iteration
        let assistant_count = last_user_content.matches("[ASSISTANT]").count();
        Some(root_llm_response(assistant_count))
    } else {
        // Sub-LLM call — return static analysis
        Some(sub_llm_response())
    }
}

/// Generate a scripted root LLM response based on the iteration number.
///
/// Exercises document access callbacks (list_documents, search_document, get_section)
/// through the Python sandbox. These go through _call_rust() → JSON-RPC → Rust parent,
/// which previously deadlocked due to redirect_stdout replacing sys.stdout with StringIO.
/// The fix saves real pipe refs at module level (_real_stdout/_real_stdin).
fn root_llm_response(assistant_count: usize) -> String {
    match assistant_count {
        // Iteration 1: Discover available documents via list_documents() callback
        0 => r#"Let me discover what documents are available for analysis.

```python
docs = list_documents()
print(f"Found {len(docs)} documents:")
for d in docs:
    print(f"  {d['doc_id'][:12]}... {d['name']} ({d['size']} bytes)")
```"#
        .to_string(),

        // Iteration 2: Search + read sections via search_document() and get_section()
        1 => r#"Now let me search the document content for architecture details.

```python
docs = list_documents()
doc_id = docs[0]['doc_id']
results = search_document(doc_id, "architecture")
print(f"Found {len(results)} search results")
section = get_section(doc_id, 0, 500)
print(f"Section: {section[:200]}")
```"#
        .to_string(),

        // Iteration 3+: Converge with FINAL answer
        _ => {
            r#"Based on my analysis, I have a comprehensive answer.

FINAL("The test document describes the ERGORS system architecture. It is a modular distributed system with components for orchestration, inference routing, and document management. The architecture uses gRPC for inter-service communication and supports multiple LLM providers through a unified routing layer.")"#
                .to_string()
        }
    }
}

/// Generate a static sub-LLM analysis response.
fn sub_llm_response() -> String {
    "The document describes a modular architecture with clear separation of concerns. \
     Key components include: orchestration layer for workflow management, \
     inference routing for LLM provider abstraction, and document storage for RAG capabilities. \
     The system uses gRPC protocols and supports horizontal scaling."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
        }
    }

    #[test]
    fn test_root_call_iteration_1_list_docs() {
        let messages = vec![make_msg(
            "user",
            "[SYSTEM]\nYou are an agent.\n\n[USER]\nQuery: What is the architecture?",
        )];
        let resp = rlm_chat_response(&messages).unwrap();
        assert!(resp.contains("```python"), "should contain code block on first iteration");
        assert!(resp.contains("list_documents()"), "iteration 1 should call list_documents");
        assert!(!resp.contains("FINAL("), "should not finalize on first call");
    }

    #[test]
    fn test_root_call_iteration_2_search() {
        let messages = vec![make_msg(
            "user",
            "[SYSTEM]\nYou are an agent.\n\n[USER]\nQuery: What?\n\n[ASSISTANT]\nprev response\n\n[USER]\nresults",
        )];
        let resp = rlm_chat_response(&messages).unwrap();
        assert!(resp.contains("```python"), "should contain code block on second iteration");
        assert!(resp.contains("search_document("), "iteration 2 should call search_document");
        assert!(resp.contains("get_section("), "iteration 2 should call get_section");
        assert!(!resp.contains("FINAL("), "should not finalize on second call");
    }

    #[test]
    fn test_root_call_iteration_3_final() {
        let messages = vec![make_msg(
            "user",
            "[SYSTEM]\nAgent.\n\n[USER]\nQ\n\n[ASSISTANT]\nA1\n\n[USER]\nR1\n\n[ASSISTANT]\nA2\n\n[USER]\nR2",
        )];
        let resp = rlm_chat_response(&messages).unwrap();
        assert!(resp.contains("FINAL("), "should finalize on third call");
        assert!(resp.contains("ERGORS"), "final answer should mention ERGORS");
    }

    #[test]
    fn test_sub_llm_call() {
        let messages = vec![make_msg(
            "user",
            "Summarize the key architectural patterns of a distributed inference system",
        )];
        let resp = rlm_chat_response(&messages).unwrap();
        assert!(resp.contains("modular architecture"), "sub-LLM should return analysis");
        assert!(!resp.contains("FINAL("), "sub-LLM should not finalize");
    }

    #[test]
    fn test_no_user_message() {
        let messages = vec![make_msg("assistant", "Hello")];
        assert!(rlm_chat_response(&messages).is_none());
    }
}
