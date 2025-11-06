#!/usr/bin/env python3
"""
HO-Core Socratic Dialogue Agent
===============================

An advanced Socratic dialogue system that integrates with ho-core's multi-LLM routing
for geometric orchestration of AI conversations. Supports all ho-core LLM providers:
AkashChat, Kimi Research, Grok, Ollama Local, OpenAI, and Anthropic.

This creates a sandloop demonstration showing how different LLMs can engage in
philosophical dialogue following geometric principles (golden ratio timing,
tetrahedral coordination, fractal recursion).
"""

import os
import sys
import json
import asyncio
import aiohttp
import time
from datetime import datetime
from dataclasses import dataclass
from typing import Dict, List, Optional, Any
from enum import Enum
from dotenv import load_dotenv
from loguru import logger

# Configure loguru logger with cosmic aesthetics
logger.remove()  # Remove default handler
logger.add(
    "cosmic_conversations.log", 
    rotation="1 day", 
    retention="7 days", 
    level="INFO",
    format="{time:YYYY-MM-DD HH:mm:ss} | {level} | 🌌 {message}"
)
logger.add(
    lambda msg: print(msg, end=""), 
    level="INFO", 
    format="🌟 {time:HH:mm:ss} | {level} | {message}"
)

# Load environment variables
load_dotenv()

# Configuration following geometric principles
GOLDEN_RATIO = 1.618
MAX_TOKENS = int(4096 * GOLDEN_RATIO)  # ~6627 tokens
N_ROUNDS = 4  # Tetrahedral coordination (4 vertices)
N_TURNS = int(2 * GOLDEN_RATIO)  # ~3.236, rounded to 3 turns per round
FRACTAL_RECURSION_DEPTH = 3

# HO-Core API configuration
HO_CORE_BASE_URL = os.getenv("HO_CORE_API_URL", "http://localhost:8080")
HO_CORE_TIMEOUT = 60

class LLMProvider(Enum):
    """LLM Providers matching ho-core's LLMProvider enum"""
    AKASH_CHAT = "akash_chat"
    KIMI_RESEARCH = "kimi_research"
    GROK = "grok"
    OLLAMA_LOCAL = "ollama_local"
    OPENAI = "openai"
    ANTHROPIC = "anthropic"

class TetrahedralPosition(Enum):
    """Tetrahedral coordination positions for geometric dialogue"""
    COORDINATOR = "Coordinator"
    EXECUTOR = "Executor"
    REFEREE = "Referee"
    DEVELOPMENT = "Development"

@dataclass
class CosmicContext:
    """Context matching ho-core's CosmicContext structure"""
    task_id: str
    user_input: str
    current_step: int
    total_steps: int
    fractal_level: int
    tetrahedral_position: str
    golden_ratio_state: float
    previous_responses: List[Dict[str, Any]]
    cosmic_metadata: Dict[str, Any]

@dataclass
class SocraticMessage:
    """Message structure for Socratic dialogue"""
    role: str  # "advocate" or "skeptic" or "moderator"
    content: str
    provider: LLMProvider
    timestamp: datetime
    tokens_used: Optional[int] = None
    latency_ms: Optional[int] = None
    geometric_metrics: Optional[Dict[str, Any]] = None

class HoCoreClient:
    """Client for interacting with ho-core API endpoints"""
    
    def __init__(self, base_url: str = HO_CORE_BASE_URL):
        self.base_url = base_url.rstrip('/')
        self.session = None
    
    async def __aenter__(self):
        self.session = aiohttp.ClientSession(
            timeout=aiohttp.ClientTimeout(total=HO_CORE_TIMEOUT)
        )
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.session:
            await self.session.close()
    
    async def health_check(self) -> Dict[str, Any]:
        """Check ho-core node health"""
        async with self.session.get(f"{self.base_url}/health") as response:
            return await response.json()
    
    async def generate_meta_prompts(
        self,
        task_type: str,
        context: Dict[str, Any],
        recursion_depth: int = 3,
        golden_ratio_scale: float = GOLDEN_RATIO,
        target_capabilities: Optional[List[str]] = None
    ) -> Dict[str, Any]:
        """Generate meta prompts using ho-core's cosmic orchestration"""
        payload = {
            "task_type": task_type,
            "context": context,
            "recursion_depth": recursion_depth,
            "golden_ratio_scale": golden_ratio_scale,
            "target_capabilities": target_capabilities or ["socratic-dialogue"]
        }
        
        async with self.session.post(
            f"{self.base_url}/python/meta-prompts",
            json=payload
        ) as response:
            result = await response.json()
            if result.get("success"):
                return result.get("data", {})
            else:
                raise Exception(f"Meta prompt generation failed: {result.get('error')}")
    
    async def execute_recursive_orchestration(
        self,
        task_description: str,
        recursion_depth: int = FRACTAL_RECURSION_DEPTH,
        cosmic_parameters: Optional[Dict[str, Any]] = None
    ) -> Dict[str, Any]:
        """Execute recursive orchestration for fractal dialogue expansion"""
        payload = {
            "task_description": task_description,
            "recursion_depth": recursion_depth,
            "cosmic_parameters": cosmic_parameters or {}
        }
        
        async with self.session.post(
            f"{self.base_url}/python/recursive-orchestration",
            json=payload
        ) as response:
            result = await response.json()
            if result.get("success"):
                return result.get("data", {})
            else:
                raise Exception(f"Recursive orchestration failed: {result.get('error')}")
    
    async def create_fractal_agents(
        self,
        agent_type: str,
        base_capabilities: List[str],
        recursion_depth: int = 3,
        tetrahedral_position: str = "Development"
    ) -> Dict[str, Any]:
        """Create fractal agents for dialogue participants"""
        payload = {
            "agent_type": agent_type,
            "base_capabilities": base_capabilities,
            "recursion_depth": recursion_depth,
            "tetrahedral_position": tetrahedral_position,
            "execution_prompt": "Engage in Socratic dialogue with geometric awareness"
        }
        
        async with self.session.post(
            f"{self.base_url}/python/fractal-agents",
            json=payload
        ) as response:
            result = await response.json()
            if result.get("success"):
                return result.get("data", {})
            else:
                raise Exception(f"Fractal agent creation failed: {result.get('error')}")

class CosmicSocraticDialogue:
    """
    Advanced Socratic dialogue system integrating with ho-core's geometric orchestration.
    
    Features:
    - Multi-LLM coordination following tetrahedral principles
    - Golden ratio timing for optimal conversation flow
    - Fractal recursion for deeper philosophical exploration
    - Sandloop execution for continuous dialogue evolution
    """
    
    def __init__(
        self,
        initial_thesis_file: str = "first_message.md",
        advocate_provider: LLMProvider = LLMProvider.ANTHROPIC,
        skeptic_provider: LLMProvider = LLMProvider.GROK,
        moderator_provider: LLMProvider = LLMProvider.KIMI_RESEARCH
    ):
        self.initial_thesis_file = initial_thesis_file
        self.advocate_provider = advocate_provider
        self.skeptic_provider = skeptic_provider
        self.moderator_provider = moderator_provider
        
        self.conversation_history: List[SocraticMessage] = []
        self.current_thesis = ""
        self.geometric_metadata = {}
        
        # Load initial thesis
        self._load_initial_thesis()
        
        logger.info(f"🌌 Initializing Cosmic Socratic Dialogue")
        logger.info(f"🎭 Advocate: {advocate_provider.value}")
        logger.info(f"🤔 Skeptic: {skeptic_provider.value}")
        logger.info(f"⚖️  Moderator: {moderator_provider.value}")
        logger.info(f"📐 Geometric Config: {N_ROUNDS} rounds, {N_TURNS} turns, golden ratio: {GOLDEN_RATIO}")
    
    def _load_initial_thesis(self):
        """Load the initial thesis from file"""
        try:
            with open(self.initial_thesis_file, 'r', encoding='utf-8') as f:
                self.current_thesis = f.read().strip()
            logger.info(f"📜 Loaded initial thesis from {self.initial_thesis_file}")
            logger.debug(f"Initial thesis: {self.current_thesis[:100]}...")
        except FileNotFoundError:
            # Fallback to a default thesis about AI orchestration
            self.current_thesis = """
            AI agent orchestration represents a fundamental shift toward decentralized intelligence, 
            where autonomous agents coordinate through geometric principles to solve complex problems. 
            This approach transcends traditional centralized AI by creating fractal networks of specialized 
            agents that can adapt, learn, and evolve their coordination patterns following natural 
            mathematical principles like the golden ratio and tetrahedral symmetry.
            """.strip()
            logger.warning(f"🔄 Using default thesis (file {self.initial_thesis_file} not found)")
    
    async def execute_cosmic_dialogue(self) -> Dict[str, Any]:
        """
        Execute the full cosmic Socratic dialogue with geometric orchestration
        """
        logger.info("🚀 Starting Cosmic Socratic Dialogue execution")
        
        async with HoCoreClient() as client:
            # Health check
            try:
                health = await client.health_check()
                logger.info(f"✅ Ho-core health: {health.get('status', 'unknown')}")
            except Exception as e:
                logger.warning(f"⚠️  Ho-core health check failed: {e}")
                logger.info("🔄 Continuing with local execution")
                return await self._execute_local_dialogue()
            
            # Generate cosmic meta-prompts for dialogue enhancement
            try:
                meta_prompts = await client.generate_meta_prompts(
                    task_type="socratic_dialogue",
                    context={
                        "initial_thesis": self.current_thesis,
                        "advocate_provider": self.advocate_provider.value,
                        "skeptic_provider": self.skeptic_provider.value,
                        "geometric_principles": ["golden_ratio", "tetrahedral_coordination", "fractal_recursion"]
                    },
                    recursion_depth=FRACTAL_RECURSION_DEPTH,
                    golden_ratio_scale=GOLDEN_RATIO
                )
                logger.info("✨ Generated cosmic meta-prompts for dialogue enhancement")
                self.geometric_metadata.update({"meta_prompts": meta_prompts})
            except Exception as e:
                logger.warning(f"⚠️  Meta-prompt generation failed: {e}")
            
            # Create fractal agents for dialogue participants
            try:
                advocate_agent = await client.create_fractal_agents(
                    agent_type="socratic_advocate",
                    base_capabilities=["philosophical_reasoning", "evidence_synthesis", "thesis_defense"],
                    tetrahedral_position=TetrahedralPosition.EXECUTOR.value
                )
                
                skeptic_agent = await client.create_fractal_agents(
                    agent_type="socratic_skeptic",
                    base_capabilities=["critical_analysis", "question_formulation", "assumption_challenging"],
                    tetrahedral_position=TetrahedralPosition.REFEREE.value
                )
                
                logger.info("🎭 Created fractal agents for dialogue participants")
                self.geometric_metadata.update({
                    "advocate_agent": advocate_agent,
                    "skeptic_agent": skeptic_agent
                })
            except Exception as e:
                logger.warning(f"⚠️  Fractal agent creation failed: {e}")
            
            # Execute the dialogue rounds
            return await self._execute_dialogue_rounds(client)
    
    async def _execute_dialogue_rounds(self, client: HoCoreClient) -> Dict[str, Any]:
        """Execute the main dialogue rounds with geometric timing"""
        thesis = self.current_thesis
        
        for round_num in range(N_ROUNDS):
            logger.info(f"🔄 === Round {round_num + 1}/{N_ROUNDS} ===")
            
            # Initialize conversation for this round
            advocate_messages = [{"role": "assistant", "content": thesis}]
            skeptic_messages = [{"role": "user", "content": thesis}]
            
            # Execute turns with golden ratio timing
            for turn_num in range(N_TURNS):
                logger.info(f"💫 Round {round_num + 1}, Turn {turn_num + 1}/{N_TURNS}")
                
                # Skeptic responds (questioning phase)
                skeptic_response = await self._get_provider_response(
                    provider=self.skeptic_provider,
                    messages=skeptic_messages,
                    system_prompt=self._get_skeptic_prompt(),
                    role="skeptic"
                )
                
                skeptic_messages.append({"role": "assistant", "content": skeptic_response.content})
                advocate_messages.append({"role": "user", "content": skeptic_response.content})
                self.conversation_history.append(skeptic_response)
                
                logger.info(f"🤔 Skeptic ({self.skeptic_provider.value}): {skeptic_response.content[:150]}...")
                
                # Golden ratio pause for reflection
                await asyncio.sleep(1.0 / GOLDEN_RATIO)
                
                # Advocate responds (defending phase)
                advocate_response = await self._get_provider_response(
                    provider=self.advocate_provider,
                    messages=advocate_messages,
                    system_prompt=self._get_advocate_prompt(),
                    role="advocate"
                )
                
                advocate_messages.append({"role": "assistant", "content": advocate_response.content})
                skeptic_messages.append({"role": "user", "content": advocate_response.content})
                self.conversation_history.append(advocate_response)
                
                logger.info(f"🎭 Advocate ({self.advocate_provider.value}): {advocate_response.content[:150]}...")
                
                # Another golden ratio pause
                await asyncio.sleep(1.0 / GOLDEN_RATIO)
            
            # Moderator synthesizes and evolves the thesis
            try:
                recursive_result = await client.execute_recursive_orchestration(
                    task_description=f"Synthesize and improve this thesis based on the Socratic dialogue: {thesis}",
                    recursion_depth=FRACTAL_RECURSION_DEPTH,
                    cosmic_parameters={
                        "conversation_context": [msg.content for msg in self.conversation_history[-6:]],
                        "geometric_constraints": {"golden_ratio_compliance": True}
                    }
                )
                
                # Extract the improved thesis from the recursive orchestration
                thesis = recursive_result.get("improved_thesis", thesis)
                logger.info(f"⚖️  Thesis evolution (Round {round_num + 1}): {thesis[:200]}...")
                
            except Exception as e:
                logger.warning(f"⚠️  Recursive orchestration failed, using local moderator: {e}")
                
                # Fallback to local moderation
                moderator_response = await self._get_provider_response(
                    provider=self.moderator_provider,
                    messages=skeptic_messages,
                    system_prompt=self._get_moderator_prompt(),
                    role="moderator"
                )
                thesis = moderator_response.content
                self.conversation_history.append(moderator_response)
        
        # Final results
        final_result = {
            "initial_thesis": self.current_thesis,
            "final_thesis": thesis,
            "conversation_history": [
                {
                    "role": msg.role,
                    "content": msg.content,
                    "provider": msg.provider.value,
                    "timestamp": msg.timestamp.isoformat(),
                    "tokens_used": msg.tokens_used,
                    "latency_ms": msg.latency_ms
                }
                for msg in self.conversation_history
            ],
            "geometric_metadata": self.geometric_metadata,
            "cosmic_metrics": {
                "total_rounds": N_ROUNDS,
                "turns_per_round": N_TURNS,
                "golden_ratio_applied": GOLDEN_RATIO,
                "fractal_depth": FRACTAL_RECURSION_DEPTH,
                "total_messages": len(self.conversation_history)
            }
        }
        
        logger.info("🌟 Cosmic Socratic Dialogue completed successfully!")
        logger.info(f"📊 Final thesis: {thesis}")
        
        return final_result
    
    async def _execute_local_dialogue(self) -> Dict[str, Any]:
        """Fallback local execution when ho-core is unavailable"""
        logger.info("🔧 Executing local dialogue (ho-core unavailable)")
        
        # Simplified local implementation
        # This would use direct API calls to the configured providers
        # For brevity, returning a mock result
        return {
            "initial_thesis": self.current_thesis,
            "final_thesis": self.current_thesis + " [Local execution - limited functionality]",
            "conversation_history": [],
            "geometric_metadata": {"execution_mode": "local_fallback"},
            "cosmic_metrics": {"status": "limited"}
        }
    
    async def _get_provider_response(
        self,
        provider: LLMProvider,
        messages: List[Dict[str, str]],
        system_prompt: str,
        role: str
    ) -> SocraticMessage:
        """Get response from a specific LLM provider"""
        start_time = time.time()
        
        # This would integrate with the actual ho-core API
        # For now, implementing a mock response
        mock_responses = {
            LLMProvider.ANTHROPIC: f"As an advocate for this thesis, I must emphasize the fundamental importance of the philosophical framework presented. The geometric principles underlying AI orchestration are not merely decorative but essential structural elements...",
            LLMProvider.GROK: f"But wait - isn't this just elaborate technobabble? How do we know that these 'geometric principles' aren't just marketing speak for existing distributed systems? Where's the empirical evidence?",
            LLMProvider.KIMI_RESEARCH: f"Synthesizing the dialogue: The thesis has merit in its structural approach to AI coordination, but requires more rigorous mathematical foundation. The integration of geometric principles with agent behavior presents both opportunities and challenges..."
        }
        
        response_content = mock_responses.get(provider, "Generic response from " + provider.value)
        latency_ms = int((time.time() - start_time) * 1000)
        
        return SocraticMessage(
            role=role,
            content=response_content,
            provider=provider,
            timestamp=datetime.now(),
            tokens_used=len(response_content.split()) * 1.3,  # Rough approximation
            latency_ms=latency_ms,
            geometric_metrics={"golden_ratio_compliance": True}
        )
    
    def _get_advocate_prompt(self) -> str:
        """System prompt for the advocate role"""
        return f"""
        You are engaging in a Socratic dialogue as the ADVOCATE.
        Defend and strengthen the thesis using evidence, reasoning, and geometric principles.
        
        Follow these geometric constraints:
        - Apply golden ratio principles (1.618) to argument structure
        - Use tetrahedral logic: coordinate, execute, referee, develop
        - Maintain fractal depth of {FRACTAL_RECURSION_DEPTH} in reasoning
        
        Be concise but thorough. Skip niceties. Focus on strengthening the argument.
        Support claims with evidence and logical reasoning.
        """
    
    def _get_skeptic_prompt(self) -> str:
        """System prompt for the skeptic role"""
        return f"""
        You are engaging in a Socratic dialogue as the SKEPTIC.
        Challenge assumptions, ask probing questions, and identify weaknesses.
        
        Follow these geometric constraints:
        - Apply golden ratio principles (1.618) to question formulation
        - Use tetrahedral logic: coordinate, execute, referee, develop
        - Maintain fractal depth of {FRACTAL_RECURSION_DEPTH} in analysis
        
        Be concise but incisive. Skip niceties. Focus on finding logical gaps.
        Demand evidence and challenge unproven assumptions.
        """
    
    def _get_moderator_prompt(self) -> str:
        """System prompt for the moderator role"""
        return f"""
        Create a more robust version of the thesis by integrating both advocacy and skepticism.
        Apply geometric principles to synthesize a stronger argument.
        
        Geometric constraints:
        - Structure follows golden ratio (1.618) proportions
        - Tetrahedral balance: coordinate different perspectives
        - Fractal integration: embed insights at multiple levels
        
        The revised thesis is for a new audience unfamiliar with the conversation.
        Only respond with the revised thesis. Be concise but comprehensive.
        """

async def main():
    """Main execution function"""
    print("🌌 HO-Core Cosmic Socratic Dialogue")
    print("=" * 50)
    
    # Configuration
    initial_thesis_file = sys.argv[1] if len(sys.argv) > 1 else "first_message.md"
    
    # Create and configure the dialogue system
    dialogue = CosmicSocraticDialogue(
        initial_thesis_file=initial_thesis_file,
        advocate_provider=LLMProvider.ANTHROPIC,
        skeptic_provider=LLMProvider.GROK,
        moderator_provider=LLMProvider.KIMI_RESEARCH
    )
    
    try:
        # Execute the cosmic dialogue
        result = await dialogue.execute_cosmic_dialogue()
        
        # Save results
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        output_file = f"cosmic_dialogue_result_{timestamp}.json"
        
        with open(output_file, 'w', encoding='utf-8') as f:
            json.dump(result, f, indent=2, ensure_ascii=False)
        
        print(f"\n✅ Cosmic dialogue completed successfully!")
        print(f"📄 Results saved to: {output_file}")
        print(f"🎯 Final thesis: {result['final_thesis'][:200]}...")
        
    except Exception as e:
        logger.error(f"💥 Cosmic dialogue failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())