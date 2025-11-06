#!/usr/bin/env python3
"""
tools/python/prompt_generator.py

- Generates meta-prompts for agentic workflows in JSON format for a Recursive Agentic Network.
- Supports recursive depth and fractal properties for nested workflows.
- Reads input from stdin when invoked by orchestrator (Rust code) or uses CLI for standalone testing.
- Outputs `MetaPromptResponse` JSON with generated prompts, agent specifications, and fractal metadata in a compact format for efficient prompt engineering.
- Can validate JSON answers produced by other models against a schema.

Usage
-----
# 1️⃣ Standalone: Generate meta-prompt for a custom task
python prompt_generator.py --task "Translate a PDF and summarise the key points"

# 2️⃣ Standalone: Validate an answer from another model
python prompt_generator.py --validate example_answer.json

# 3️⃣ Orchestrated: Process input from Rust orchestrator (reads JSON from stdin)
python prompt_generator.py --meta-prompt-generation

# 4️⃣ Toggle compact/pretty output in standalone mode
python prompt_generator.py --task "Some task" --compact
"""

import argparse
import json
import pathlib
import sys
from typing import Any, Dict, List, Optional

import uuid               # <-- needed for UUID generation
import math               # (used by the golden‑ratio helpers)
 
SCHEMA_PATH = pathlib.Path(__file__).with_name("schema.json")

# φ – the golden ratio
PHI = (1 + 5 ** 0.5) / 2          # ≈ 1.618033988749895
INV_PHI = 1 / PHI                # ≈ 0.6180339887498949
PHI_TOLERANCE = 0.05      

def _golden_weight(level: int) -> float:
    """Weight that decays with the golden ratio (level 0 → 1.0)."""
    return INV_PHI ** level

def _tetrahedral_position(level: int) -> str:
    """
    Map the recursion level to one of the four “tetrahedral” roles.
    Feel free to adjust the mapping to your own ontology.
    """
    mapping = {
        0: "Coordinator",
        1: "Executor",
        2: "Referee",
        3: "Development"
    }
    # If we go deeper than 3 we simply reuse the last role.
    return mapping.get(level, "Development")

def _is_golden_ratio_compliant(value: float) -> bool:
    """
    Returns True if *value* lies within a small tolerance of the golden ratio.
    """
    return abs(value - PHI) <= PHI_TOLERANCE

 


def load_schema() -> Dict[str, Any]:
    """Load the JSON-Schema that defines the expected answer format."""
    try:
        return json.loads(SCHEMA_PATH.read_text())
    except Exception as exc:
        sys.exit(f"❌ Could not read schema at {SCHEMA_PATH}: {exc}")


# --------------------------------------------------------------------------- #
#   Meta-prompt and Response Generator
# --------------------------------------------------------------------------- #
def build_meta_prompt(task: str, recursion_depth: int = 1) -> Dict[str, Any]:
    """
    Builds a meta-prompt structure for agentic workflows with support for recursion depth.
    Returns a dict that can be used as a prompt for an LLM, optimized for compactness.
    """
    step_structure = {
        "step_number": "int",
        "description": task,
        "tool": "str(opt)",
        "condition": "str(opt)",
        "expected_outcome": "str(opt)"
    }
    if recursion_depth > 1:
        step_structure["sub_steps"] = f"arr(depth={recursion_depth-1})"

    return {
        "instruction": "Generate a prompt for agentic workflow steps in a Recursive Agentic Network.",
        "purpose": "Define JSON format for autonomous agent actions with recursive support.",
        "recursion_depth": recursion_depth,
        "response_format": {
            "type": "json",
            "structure": {
                "task": task,
                "steps": [step_structure],
                "final_output": "str"
            }
        },
        "example_task": task,
        "note": f"Output JSON workflow with nested steps up to depth {recursion_depth}."
    }


## TODO: PARSE FROM REQUEST
def generate_agent_specifications(task_type: str, recursion_depth: int) -> List[Dict[str, Any]]:
    """
    Produce a list of agent specifications whose schema matches the
    Rust `AgentSpec` struct.

    The mapping from the original “base_agent / sub‑agents” to the new
    struct is:

    * **agent_id**        ← original `id`
    * **agent_type**      ← original `task_type`
    * **capabilities**    ← unchanged
    * **execution_prompt**← a short, human‑readable instruction derived
                             from the role and level
    * **tetrahedral_position** ← role derived from the recursion level
    * **fractal_properties**   ← numeric meta‑data (depth, golden‑ratio
                                 weight, optional “importance”)
    """
    agents: List[Dict[str, Any]] = []

    # ------------------------------------------------------------------
    # Root (level 0) – the primary orchestrator
    # ------------------------------------------------------------------
    root_agent = {
        "agent_id": "agent-0-root",
        "agent_type": task_type,
        "capabilities": [
            "task decomposition",
            "step execution",
            "result aggregation"
        ],
        "execution_prompt": "Orchestrate the entire workflow and delegate subtasks.",
        "tetrahedral_position": _tetrahedral_position(0),
        "fractal_properties": {
            "depth": 0,
            "golden_weight": _golden_weight(0),
            "importance": 1.0
        }
    }
    agents.append(root_agent)

    # ------------------------------------------------------------------
    # Sub‑agents (depth 1 … recursion_depth)
    # ------------------------------------------------------------------
    for level in range(1, recursion_depth + 1):
        sub_agent = {
            "agent_id": f"agent-{level}-sub",
            "agent_type": f"{task_type}_sub_depth_{level}",
            "capabilities": [
                "execute subtask",
                "report to parent"
            ],
            # A concise prompt that tells the sub‑agent *what* it must do.
            "execution_prompt": f"Execute the portion of the task assigned for depth {level}.",
            "tetrahedral_position": _tetrahedral_position(level),
            "fractal_properties": {
                "depth": level,
                "golden_weight": _golden_weight(level),
                # A simple heuristic for “importance” – deeper levels are
                # slightly less critical in this example.
                "importance": max(0.1, 1.0 - 0.2 * level)
            }
        }
        agents.append(sub_agent)

    return agents

def generate_fractal_metadata(
    recursion_depth: int,
    *,
    base_dimension: float = 1.0,
    dimension_increment: float = 0.5,
    max_possible_depth: int = 5,
) -> Dict[str, Any]:
    """
    Build the ``FractalMetadata`` mapping expected by the Rust orchestrator.
    Returns
    -------
    dict
        A dict that serialises to the Rust ``FractalMetadata`` struct:

        ```rust
        pub struct FractalMetadata {
            pub fractal_dimension: f64,
            pub golden_ratio_compliance: bool,
            pub recursive_depth_achieved: u32,
            pub tetrahedral_coverage: f64,
            pub cosmic_coherence_score: f64,
        }
        ```
    """
    # 1️⃣  Fractal dimension – same formula you already employed
    fractal_dimension = base_dimension + recursion_depth * dimension_increment

    # 2️⃣  Golden‑ratio compliance flag
    golden_ratio_compliance = _is_golden_ratio_compliant(fractal_dimension)

    # 3️⃣  Depth that was actually achieved (just echo the input)
    recursive_depth_achieved = recursion_depth

    # 4️⃣  Tetrahedral coverage – proportion of the *max* depth that we have
    tetrahedral_coverage = min(1.0, recursion_depth / max_possible_depth)

    # 5️⃣  Cosmic coherence score – a simple weighted blend:
    #     • 0.7 weight goes to golden‑ratio compliance (binary → 1 or 0)
    #     • 0.3 weight goes to coverage (already a 0‑1 float)
    cosmic_coherence_score = (
        0.7 * (1.0 if golden_ratio_compliance else 0.0) +
        0.3 * tetrahedral_coverage
    )

    return {
        "fractal_dimension": fractal_dimension,
        "golden_ratio_compliance": golden_ratio_compliance,
        "recursive_depth_achieved": recursive_depth_achieved,
        "tetrahedral_coverage": tetrahedral_coverage,
        "cosmic_coherence_score": cosmic_coherence_score,
    }

def build_meta_prompt_response(request: Dict[str, Any]) -> Dict[str, Any]:
    """
    Builds the `MetaPromptResponse` JSON structure that the Rust
    orchestrator expects.
    """
    # ------------------------------------------------------------------
    # Extract request data SCHEMA: MetaPromptRequest
    # ------------------------------------------------------------------
    task_type = request.get("task_type", "generic_task")
    context = request.get("context", {})
    cosmic_params = request.get("cosmic_parameters", {})
    task_desc = context.get("prompt", "Perform a complex multi-step task")
    recursion_depth = cosmic_params.get("recursion_depth", 1)

    # ------------------------------------------------------------------
    # ------------------------------------------------------------------
    raw_prompts = [build_meta_prompt(task_desc, recursion_depth)]
    prompts = format_generated_prompts(raw_prompts)
    agent_specs = generate_agent_specifications(task_type, recursion_depth)
    orchestration_sequence = generate_orchestration_sequence(task_type, recursion_depth)
    fractal_metadata = generate_fractal_metadata(recursion_depth)


    # ------------------------------------------------------------------
    # SCHEMA: MetaPromptResponse
    # ------------------------------------------------------------------
    return {
        "generated_prompts": prompts,                    
        "agent_specifications": agent_specs,
         "fractal_metadata": fractal_metadata,
        "orchestration_sequence": orchestration_sequence,
    }

def generate_orchestration_sequence(task_type: str, recursion_depth: int) -> List[Dict[str, Any]]:
    """
    Generates an orchestration sequence based on the task type and recursion depth.
    """
    sequence = []
    for i in range(recursion_depth):
        step_id = f"step_{i}"
        step_type = "recursive_task" if i < recursion_depth - 1 else "final_task"
        execution_order = i + 1
        dependencies = [f"step_{j}" for j in range(i)]
        recursive_expansions = [
            {
                "expansion_id": f"expansion_{i}",
                "fractal_level": i + 1,
                "self_similarity_ratio": 0.5,
                "expansion_prompt": f"Expand task at level {i + 1}",
                "termination_criteria": ["max_depth_reached", "task_completed"]
            }
        ]
        sequence.append({
            "step_id": step_id,
            "step_type": step_type,
            "execution_order": execution_order,
            "dependencies": dependencies,
            "recursive_expansions": recursive_expansions
        })
    return sequence


def format_generated_prompts(prompts: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    """
    Convert a list of *raw* prompt dicts (the shape you already build
    in `build_meta_prompt`) into the JSON structure required by the
    Rust `GeneratedPrompt` type.

    GeneratedPrompt in Rust:
        pub struct GeneratedPrompt {
            pub id: String,
            pub content: String,
            pub prompt_type: String,
            pub geometric_weight: f64,
            pub dependencies: Vec<String>,
        }
    """
    formatted: List[Dict[str, Any]] = []

    for idx, raw in enumerate(prompts):
        # ── id (UUID‑4) ──────────────────────────────────────
        prompt_id = str(uuid.uuid4())

        # ── content ────────────────────────────────────────
        # If the caller already supplied a full “content” field we keep it,
        # otherwise we synthesize something readable from the known pieces.
        if isinstance(raw.get("content"), str):
            content = raw["content"]
        else:
            # Concatenate the most useful textual bits (skip empties)
            parts = [
                raw.get("instruction", ""),
                raw.get("purpose", ""),
                raw.get("example_task", ""),
                raw.get("note", "")
            ]
            content = "\n".join(p for p in parts if p)

        # ── prompt_type ─────────────────────────────────────
        # Prefer the explicit type from response_format, fallback to "generic"
        response_fmt = raw.get("response_format", {})
        if isinstance(response_fmt, dict):
            prompt_type = response_fmt.get("type", "generic")
        else:
            prompt_type = "generic"

        # ── geometric_weight (golden‑ratio) ─────────────────
        weight = _golden_weight(idx)

        # ── dependencies ───────────────────────────────────
        # If the raw dict already carries a list of dependency ids, use it.
        # Otherwise an empty list.
        raw_deps = raw.get("dependencies", [])
        dependencies = [str(d) for d in raw_deps] if isinstance(raw_deps, list) else []

        formatted.append({
            "id": prompt_id,
            "content": content,
            "prompt_type": prompt_type,
            "geometric_weight": weight,
            "dependencies": dependencies,
        })

    return formatted
    



def process_orchestrator_input() -> None:
    """Reads JSON input from stdin (from Rust orchestrator) and generates a compact response."""
    input_str = sys.stdin.read()
    if not input_str:
        sys.exit("❌ No input received from stdin for meta-prompt generation.")
    try:
        request = json.loads(input_str)
    except json.JSONDecodeError as exc:
        sys.exit(f"❌ Invalid JSON input from stdin: {exc}")

    response = build_meta_prompt_response(request)
    # Output compact JSON for orchestrator use
    print(json.dumps(response, indent=None, separators=(',', ':'), ensure_ascii=False))


# --------------------------------------------------------------------------- #
#   Validation of another model's answer
# --------------------------------------------------------------------------- #
def validate_answer(answer_path: pathlib.Path) -> None:
    """
    Validate a JSON file that contains an answer produced by a different model.
    Prints a succinct success/failure report.
    """
    try:
        answer = json.loads(answer_path.read_text())
    except Exception as exc:
        sys.exit(f"❌ Could not read answer JSON from {answer_path}: {exc}")

    schema = load_schema()
    try:
        from jsonschema import validate, ValidationError
    except ImportError:
        sys.exit("❌ jsonschema not installed – run `pip install -r requirements.txt` first.")

    try:
        validate(instance=answer, schema=schema)
        print(f"✅ Validation succeeded – `{answer_path.name}` conforms to the schema.")
    except ValidationError as ve:
        print(f"❌ Validation failed – `{answer_path.name}` does NOT conform to the schema.")
        print("\n--- Error details ------------------------------------------------")
        print(ve.message)
        if ve.path:
            print("Path:", " → ".join(map(str, ve.path)))
        print("-----------------------------------------------------------------\n")


# --------------------------------------------------------------------------- #
#   CLI handling
# --------------------------------------------------------------------------- #
def parse_cli() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate meta-prompts for agentic workflows or validate another model's answer."
    )
    parser.add_argument(
        "--task",
        type=str,
        default="Fetch and summarize the latest AI news from the web",
        help="Custom task description used in the generated meta-prompt (standalone mode)."
    )
    parser.add_argument(
        "--recursion-depth",
        type=int,
        default=1,
        help="Recursion depth for nested workflows (standalone mode)."
    )
    parser.add_argument(
        "--validate",
        type=pathlib.Path,
        metavar="ANSWER_JSON",
        help="Path to a JSON file containing an answer from another model. If supplied, the script validates the file instead of generating a prompt."
    )
    parser.add_argument(
        "--meta-prompt-generation",
        action="store_true",
        help="Process input from stdin for meta-prompt generation (used by Rust orchestrator)."
    )
    parser.add_argument(
        "--compact",
        action="store_true",
        help="Output compact JSON in standalone mode (no indentation, minimal spacing)."
    )
    return parser.parse_args()


def main() -> None:
    args = parse_cli()
    if args.validate:
        validate_answer(args.validate)
    elif args.meta_prompt_generation:
        process_orchestrator_input()
    else:
        # Standalone mode for testing
        request = {
            "task_type": "standalone_test",
            "task_description": args.task,
            "cosmic_parameters": {
                "recursion_depth": args.recursion_depth
            }
        }
        response = build_meta_prompt_response(request)
        # Use compact JSON if requested, otherwise pretty-print for readability
        if args.compact:
            print(json.dumps(response, indent=None, separators=(',', ':'), ensure_ascii=False))
        else:
            print(json.dumps(response, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()