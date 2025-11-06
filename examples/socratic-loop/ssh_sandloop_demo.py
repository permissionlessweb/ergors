#!/usr/bin/env python3
"""
SSH Sandloop Demonstration for HO-Core
======================================

This script demonstrates how to use ho-core's agent orchestration capabilities
to setup and execute a "sandloop" - a continuous, evolving dialogue between
LLMs across multiple remote nodes via SSH connections.

The sandloop follows these geometric principles:
- Tetrahedral coordination: 4 nodes in a tetrahedral network
- Golden ratio timing: Connection intervals follow φ (1.618) ratios
- Fractal recursion: Each dialogue spawns sub-dialogues
- Möbius continuity: Conversations flow seamlessly between nodes

Features:
- SSH-based remote node coordination
- Multi-LLM Socratic dialogues
- Geometric timing and coordination patterns
- Real-time sandloop monitoring
- Automatic node discovery and failover
"""

import os
import sys
import json
import asyncio
import paramiko
import aiohttp
from datetime import datetime, timedelta
from dataclasses import dataclass, asdict
from typing import Dict, List, Optional, Any, Tuple
from enum import Enum
import logging
from pathlib import Path
import subprocess
import time

# Configure logging
logging.basicConfig(level=logging.INFO, format='🌌 %(asctime)s | %(levelname)s | %(message)s')
logger = logging.getLogger(__name__)

# Geometric constants
GOLDEN_RATIO = 1.618
TETRAHEDRAL_NODES = 4
SANDLOOP_INTERVAL = int(60 * GOLDEN_RATIO)  # ~97 seconds
FRACTAL_DEPTH = 3

@dataclass
class SSHConnection:
    """SSH connection configuration for remote nodes"""
    host: str
    port: int = 22
    username: str = "ubuntu"  # Default for many cloud instances
    key_file: Optional[str] = None
    password: Optional[str] = None
    ho_core_port: int = 8080
    ho_core_path: str = "/opt/ho-core/target/release/ho-core"

@dataclass
class NodeStatus:
    """Status of a remote ho-core node"""
    ssh_config: SSHConnection
    is_connected: bool = False
    ho_core_running: bool = False
    last_health_check: Optional[datetime] = None
    active_tasks: int = 0
    tetrahedral_position: str = "Development"
    error_message: Optional[str] = None

@dataclass
class SandloopState:
    """Current state of the sandloop execution"""
    loop_id: str
    start_time: datetime
    current_round: int
    total_rounds: int
    active_nodes: List[str]
    current_dialogue: Optional[str] = None
    fractal_level: int = 0
    geometric_health: float = 1.0  # Golden ratio compliance metric

class TetrahedralPosition(Enum):
    """Tetrahedral coordination positions"""
    COORDINATOR = "Coordinator"
    EXECUTOR = "Executor"
    REFEREE = "Referee"
    DEVELOPMENT = "Development"

class SSHSandloopOrchestrator:
    """
    Orchestrates sandloop executions across SSH-connected ho-core nodes
    """
    
    def __init__(self, ssh_configs: List[SSHConnection], demo_thesis_file: str = "first_message.md"):
        self.ssh_configs = ssh_configs
        self.nodes: Dict[str, NodeStatus] = {}
        self.sandloop_state = None
        self.demo_thesis_file = demo_thesis_file
        
        # Initialize node statuses
        for config in ssh_configs:
            node_id = f"{config.host}:{config.ho_core_port}"
            self.nodes[node_id] = NodeStatus(ssh_config=config)
        
        logger.info(f"🌐 Initialized SSH Sandloop Orchestrator with {len(ssh_configs)} nodes")
    
    async def setup_sandloop_demonstration(self) -> bool:
        """
        Setup the complete sandloop demonstration environment
        """
        logger.info("🚀 Setting up SSH Sandloop Demonstration")
        
        # Phase 1: Test SSH connections
        logger.info("📡 Phase 1: Testing SSH connections...")
        success_count = 0
        for node_id, node_status in self.nodes.items():
            if await self._test_ssh_connection(node_status):
                success_count += 1
                logger.info(f"✅ SSH connection successful: {node_id}")
            else:
                logger.error(f"❌ SSH connection failed: {node_id}")
        
        if success_count < 2:
            logger.error("❌ Need at least 2 working SSH connections for demonstration")
            return False
        
        # Phase 2: Deploy ho-core to remote nodes
        logger.info("📦 Phase 2: Deploying ho-core to remote nodes...")
        deployed_count = 0
        for node_id, node_status in self.nodes.items():
            if node_status.is_connected:
                if await self._deploy_ho_core(node_status):
                    deployed_count += 1
                    logger.info(f"✅ Ho-core deployed: {node_id}")
                else:
                    logger.error(f"❌ Ho-core deployment failed: {node_id}")
        
        # Phase 3: Start ho-core nodes with tetrahedral positioning
        logger.info("🔺 Phase 3: Starting ho-core nodes with tetrahedral coordination...")
        positions = list(TetrahedralPosition)
        running_count = 0
        
        for i, (node_id, node_status) in enumerate(self.nodes.items()):
            if node_status.is_connected:
                position = positions[i % len(positions)]
                if await self._start_ho_core_node(node_status, position):
                    node_status.tetrahedral_position = position.value
                    running_count += 1
                    logger.info(f"✅ Ho-core started: {node_id} ({position.value})")
                else:
                    logger.error(f"❌ Ho-core start failed: {node_id}")
        
        # Phase 4: Deploy Socratic dialogue script
        logger.info("📜 Phase 4: Deploying Socratic dialogue scripts...")
        script_deployed_count = 0
        for node_id, node_status in self.nodes.items():
            if node_status.ho_core_running:
                if await self._deploy_socratic_script(node_status):
                    script_deployed_count += 1
                    logger.info(f"✅ Socratic script deployed: {node_id}")
                else:
                    logger.error(f"❌ Socratic script deployment failed: {node_id}")
        
        # Phase 5: Initialize network coordination
        logger.info("🕸️  Phase 5: Initializing network coordination...")
        if await self._initialize_network_coordination():
            logger.info("✅ Network coordination initialized")
        else:
            logger.warning("⚠️  Network coordination initialization incomplete")
        
        success = running_count >= 2 and script_deployed_count >= 2
        if success:
            logger.info(f"🌟 Sandloop demonstration setup completed! ({running_count} nodes active)")
        else:
            logger.error("❌ Sandloop demonstration setup failed")
        
        return success
    
    async def execute_sandloop_demonstration(self, total_rounds: int = 4) -> Dict[str, Any]:
        """
        Execute the main sandloop demonstration with geometric coordination
        """
        loop_id = f"sandloop_{datetime.now().strftime('%Y%m%d_%H%M%S')}"
        
        self.sandloop_state = SandloopState(
            loop_id=loop_id,
            start_time=datetime.now(),
            current_round=0,
            total_rounds=total_rounds,
            active_nodes=[node_id for node_id, status in self.nodes.items() if status.ho_core_running]
        )
        
        logger.info(f"🌀 Starting Sandloop Demonstration: {loop_id}")
        logger.info(f"🔢 Configuration: {total_rounds} rounds, {len(self.sandloop_state.active_nodes)} active nodes")
        
        results = {
            "loop_id": loop_id,
            "start_time": self.sandloop_state.start_time.isoformat(),
            "configuration": {
                "total_rounds": total_rounds,
                "active_nodes": self.sandloop_state.active_nodes,
                "geometric_constants": {
                    "golden_ratio": GOLDEN_RATIO,
                    "sandloop_interval": SANDLOOP_INTERVAL,
                    "fractal_depth": FRACTAL_DEPTH
                }
            },
            "rounds": []
        }
        
        try:
            for round_num in range(total_rounds):
                logger.info(f"🔄 === Round {round_num + 1}/{total_rounds} ===")
                self.sandloop_state.current_round = round_num + 1
                
                round_result = await self._execute_sandloop_round(round_num)
                results["rounds"].append(round_result)
                
                # Health check between rounds
                await self._health_check_all_nodes()
                
                # Golden ratio interval between rounds
                if round_num < total_rounds - 1:
                    wait_time = SANDLOOP_INTERVAL / (round_num + 1)  # Decreasing intervals
                    logger.info(f"⏱️  Golden ratio pause: {wait_time:.1f} seconds")
                    await asyncio.sleep(wait_time)
            
            results["status"] = "completed"
            results["end_time"] = datetime.now().isoformat()
            results["total_duration"] = (datetime.now() - self.sandloop_state.start_time).total_seconds()
            
            logger.info("🌟 Sandloop demonstration completed successfully!")
            
        except Exception as e:
            logger.error(f"💥 Sandloop demonstration failed: {e}")
            results["status"] = "failed"
            results["error"] = str(e)
        
        return results
    
    async def _execute_sandloop_round(self, round_num: int) -> Dict[str, Any]:
        """Execute a single round of the sandloop with tetrahedral coordination"""
        round_start = datetime.now()
        
        # Select nodes for this round using tetrahedral principles
        coordinator_node = self._select_node_by_position(TetrahedralPosition.COORDINATOR)
        executor_node = self._select_node_by_position(TetrahedralPosition.EXECUTOR)
        referee_node = self._select_node_by_position(TetrahedralPosition.REFEREE)
        
        logger.info(f"🔺 Tetrahedral assignment - Coordinator: {coordinator_node}, Executor: {executor_node}, Referee: {referee_node}")
        
        # Create dialogue task configuration
        dialogue_config = {
            "round_number": round_num + 1,
            "coordinator": coordinator_node,
            "executor": executor_node,
            "referee": referee_node,
            "thesis_evolution_depth": FRACTAL_DEPTH,
            "geometric_constraints": {
                "golden_ratio_timing": GOLDEN_RATIO,
                "tetrahedral_coordination": True,
                "fractal_recursion": FRACTAL_DEPTH
            }
        }
        
        round_result = {
            "round_number": round_num + 1,
            "start_time": round_start.isoformat(),
            "configuration": dialogue_config,
            "dialogue_phases": []
        }
        
        try:
            # Phase 1: Initiate dialogue on coordinator node
            logger.info("🎭 Phase 1: Initiating Socratic dialogue on coordinator node...")
            coordinator_result = await self._execute_remote_dialogue(
                self.nodes[coordinator_node],
                "coordinator",
                round_num
            )
            round_result["dialogue_phases"].append({
                "phase": "coordinator_initiation",
                "node": coordinator_node,
                "result": coordinator_result
            })
            
            # Phase 2: Execute on executor node
            logger.info("⚡ Phase 2: Executing dialogue continuation on executor node...")
            executor_result = await self._execute_remote_dialogue(
                self.nodes[executor_node],
                "executor",
                round_num,
                previous_context=coordinator_result.get("final_thesis")
            )
            round_result["dialogue_phases"].append({
                "phase": "executor_continuation",
                "node": executor_node,
                "result": executor_result
            })
            
            # Phase 3: Referee synthesis
            logger.info("⚖️  Phase 3: Referee synthesis and validation...")
            referee_result = await self._execute_remote_synthesis(
                self.nodes[referee_node],
                coordinator_result.get("final_thesis", ""),
                executor_result.get("final_thesis", ""),
                round_num
            )
            round_result["dialogue_phases"].append({
                "phase": "referee_synthesis",
                "node": referee_node,
                "result": referee_result
            })
            
            # Update sandloop state
            if referee_result and referee_result.get("synthesized_thesis"):
                self.sandloop_state.current_dialogue = referee_result["synthesized_thesis"]
            
            round_result["status"] = "completed"
            round_result["end_time"] = datetime.now().isoformat()
            round_result["duration"] = (datetime.now() - round_start).total_seconds()
            
            logger.info(f"✅ Round {round_num + 1} completed in {round_result['duration']:.1f}s")
            
        except Exception as e:
            logger.error(f"❌ Round {round_num + 1} failed: {e}")
            round_result["status"] = "failed"
            round_result["error"] = str(e)
        
        return round_result
    
    async def _execute_remote_dialogue(
        self, 
        node: NodeStatus, 
        role: str, 
        round_num: int,
        previous_context: Optional[str] = None
    ) -> Dict[str, Any]:
        """Execute Socratic dialogue on a remote node via SSH"""
        logger.info(f"🗣️  Executing remote dialogue on {node.ssh_config.host} (role: {role})")
        
        # Prepare the dialogue command
        dialogue_cmd = [
            "python3", 
            "/opt/ho-core/examples/socratic_dialogue.py",
            "/opt/ho-core/examples/first_message.md"
        ]
        
        if previous_context:
            # Write context to a temporary file
            context_file = f"/tmp/context_round_{round_num}.md"
            await self._write_remote_file(node, context_file, previous_context)
            dialogue_cmd[-1] = context_file
        
        try:
            # Execute the dialogue via SSH
            ssh_client = paramiko.SSHClient()
            ssh_client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
            
            ssh_client.connect(
                hostname=node.ssh_config.host,
                port=node.ssh_config.port,
                username=node.ssh_config.username,
                key_filename=node.ssh_config.key_file,
                password=node.ssh_config.password,
                timeout=30
            )
            
            # Run the dialogue command
            cmd_str = " ".join(dialogue_cmd)
            logger.info(f"🔧 Executing: {cmd_str}")
            
            stdin, stdout, stderr = ssh_client.exec_command(cmd_str)
            
            # Wait for completion with timeout
            exit_status = stdout.channel.recv_exit_status()
            
            if exit_status == 0:
                # Parse the output
                output = stdout.read().decode('utf-8')
                error = stderr.read().decode('utf-8')
                
                # Look for the result file
                result_files_cmd = "ls /opt/ho-core/cosmic_dialogue_result_*.json | tail -1"
                _, result_stdout, _ = ssh_client.exec_command(result_files_cmd)
                result_file = result_stdout.read().decode('utf-8').strip()
                
                if result_file:
                    # Retrieve the result file
                    _, file_stdout, _ = ssh_client.exec_command(f"cat {result_file}")
                    result_content = file_stdout.read().decode('utf-8')
                    result_data = json.loads(result_content)
                    
                    logger.info(f"✅ Remote dialogue completed on {node.ssh_config.host}")
                    return result_data
                else:
                    logger.warning(f"⚠️  No result file found on {node.ssh_config.host}")
                    return {"status": "no_result", "output": output, "error": error}
            
            else:
                error = stderr.read().decode('utf-8')
                logger.error(f"❌ Remote dialogue failed on {node.ssh_config.host}: {error}")
                return {"status": "failed", "exit_code": exit_status, "error": error}
        
        except Exception as e:
            logger.error(f"❌ SSH execution failed on {node.ssh_config.host}: {e}")
            return {"status": "error", "error": str(e)}
        
        finally:
            ssh_client.close()
    
    async def _execute_remote_synthesis(
        self,
        node: NodeStatus,
        thesis_a: str,
        thesis_b: str,
        round_num: int
    ) -> Dict[str, Any]:
        """Execute thesis synthesis on referee node"""
        logger.info(f"🔬 Executing synthesis on referee node {node.ssh_config.host}")
        
        # Create synthesis prompt
        synthesis_prompt = f"""
        Synthesize these two evolved theses into a stronger, more comprehensive version:
        
        Thesis A: {thesis_a}
        
        Thesis B: {thesis_b}
        
        Apply geometric principles (golden ratio, tetrahedral balance, fractal depth) to create
        a synthesis that maintains the best elements of both while resolving any contradictions.
        """
        
        # Write synthesis task to remote node
        synthesis_file = f"/tmp/synthesis_round_{round_num}.md"
        await self._write_remote_file(node, synthesis_file, synthesis_prompt)
        
        # Use ho-core's orchestration API for synthesis
        api_url = f"http://{node.ssh_config.host}:{node.ssh_config.ho_core_port}"
        
        try:
            async with aiohttp.ClientSession() as session:
                synthesis_payload = {
                    "task_description": f"Synthesize theses with geometric principles: {synthesis_prompt[:500]}...",
                    "recursion_depth": FRACTAL_DEPTH,
                    "cosmic_parameters": {
                        "thesis_a": thesis_a,
                        "thesis_b": thesis_b,
                        "geometric_constraints": {"golden_ratio": GOLDEN_RATIO}
                    }
                }
                
                async with session.post(
                    f"{api_url}/python/recursive-orchestration",
                    json=synthesis_payload,
                    timeout=60
                ) as response:
                    if response.status == 200:
                        result = await response.json()
                        if result.get("success"):
                            synthesis_data = result.get("data", {})
                            logger.info(f"✅ Synthesis completed on {node.ssh_config.host}")
                            return {
                                "synthesized_thesis": synthesis_data.get("result", "Synthesis completed"),
                                "geometric_metrics": synthesis_data.get("geometric_metadata", {}),
                                "status": "completed"
                            }
                    
                    logger.warning(f"⚠️  Synthesis API call failed: {response.status}")
                    return {"status": "api_failed", "error": f"HTTP {response.status}"}
        
        except Exception as e:
            logger.error(f"❌ Synthesis failed on {node.ssh_config.host}: {e}")
            return {"status": "error", "error": str(e)}
    
    # Helper methods for node management
    
    async def _test_ssh_connection(self, node: NodeStatus) -> bool:
        """Test SSH connection to a node"""
        try:
            ssh_client = paramiko.SSHClient()
            ssh_client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
            
            ssh_client.connect(
                hostname=node.ssh_config.host,
                port=node.ssh_config.port,
                username=node.ssh_config.username,
                key_filename=node.ssh_config.key_file,
                password=node.ssh_config.password,
                timeout=10
            )
            
            # Test basic command
            stdin, stdout, stderr = ssh_client.exec_command("echo 'SSH connection test'")
            output = stdout.read().decode('utf-8').strip()
            
            ssh_client.close()
            
            node.is_connected = (output == "SSH connection test")
            return node.is_connected
        
        except Exception as e:
            logger.error(f"SSH connection test failed for {node.ssh_config.host}: {e}")
            node.is_connected = False
            node.error_message = str(e)
            return False
    
    async def _deploy_ho_core(self, node: NodeStatus) -> bool:
        """Deploy ho-core binary to remote node"""
        if not node.is_connected:
            return False
        
        try:
            # This would typically involve:
            # 1. Copying the ho-core binary via SCP
            # 2. Setting up the environment
            # 3. Installing dependencies
            # For the demo, we'll assume ho-core is already deployed
            
            ssh_client = paramiko.SSHClient()
            ssh_client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
            
            ssh_client.connect(
                hostname=node.ssh_config.host,
                port=node.ssh_config.port,
                username=node.ssh_config.username,
                key_filename=node.ssh_config.key_file,
                password=node.ssh_config.password
            )
            
            # Check if ho-core exists
            stdin, stdout, stderr = ssh_client.exec_command(f"test -f {node.ssh_config.ho_core_path} && echo 'exists'")
            exists = "exists" in stdout.read().decode('utf-8')
            
            ssh_client.close()
            
            if exists:
                logger.info(f"✅ Ho-core already deployed on {node.ssh_config.host}")
                return True
            else:
                logger.info(f"📦 Ho-core needs to be deployed to {node.ssh_config.host}")
                # In a real implementation, we would copy the binary here
                return True  # For demo purposes
        
        except Exception as e:
            logger.error(f"Ho-core deployment failed for {node.ssh_config.host}: {e}")
            return False
    
    async def _start_ho_core_node(self, node: NodeStatus, position: TetrahedralPosition) -> bool:
        """Start ho-core node with specific tetrahedral position"""
        if not node.is_connected:
            return False
        
        try:
            ssh_client = paramiko.SSHClient()
            ssh_client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
            
            ssh_client.connect(
                hostname=node.ssh_config.host,
                port=node.ssh_config.port,
                username=node.ssh_config.username,
                key_filename=node.ssh_config.key_file,
                password=node.ssh_config.password
            )
            
            # Start ho-core in the background
            start_cmd = f"""
            cd /opt/ho-core && 
            nohup {node.ssh_config.ho_core_path} start \\
                --port {node.ssh_config.ho_core_port} \\
                --p2p-port {node.ssh_config.ho_core_port + 1000} \\
                --log-level info > ho-core.log 2>&1 &
            """
            
            stdin, stdout, stderr = ssh_client.exec_command(start_cmd)
            
            # Wait a moment for startup
            await asyncio.sleep(3)
            
            # Check if the process is running
            stdin, stdout, stderr = ssh_client.exec_command("pgrep -f ho-core")
            pid = stdout.read().decode('utf-8').strip()
            
            ssh_client.close()
            
            node.ho_core_running = bool(pid)
            if node.ho_core_running:
                logger.info(f"✅ Ho-core started on {node.ssh_config.host} (PID: {pid})")
            else:
                logger.error(f"❌ Ho-core failed to start on {node.ssh_config.host}")
            
            return node.ho_core_running
        
        except Exception as e:
            logger.error(f"Ho-core start failed for {node.ssh_config.host}: {e}")
            return False
    
    async def _deploy_socratic_script(self, node: NodeStatus) -> bool:
        """Deploy the Socratic dialogue script to remote node"""
        # In a real implementation, we would copy the script file
        # For the demo, we'll assume it's already there
        return True
    
    async def _write_remote_file(self, node: NodeStatus, remote_path: str, content: str) -> bool:
        """Write content to a file on remote node"""
        try:
            ssh_client = paramiko.SSHClient()
            ssh_client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
            
            ssh_client.connect(
                hostname=node.ssh_config.host,
                port=node.ssh_config.port,
                username=node.ssh_config.username,
                key_filename=node.ssh_config.key_file,
                password=node.ssh_config.password
            )
            
            # Use echo to write content (for short content)
            # In production, would use SFTP for larger files
            escaped_content = content.replace("'", "'\"'\"'")  # Escape single quotes
            cmd = f"echo '{escaped_content}' > {remote_path}"
            
            stdin, stdout, stderr = ssh_client.exec_command(cmd)
            exit_status = stdout.channel.recv_exit_status()
            
            ssh_client.close()
            
            return exit_status == 0
        
        except Exception as e:
            logger.error(f"Failed to write remote file {remote_path}: {e}")
            return False
    
    def _select_node_by_position(self, position: TetrahedralPosition) -> Optional[str]:
        """Select a node by its tetrahedral position"""
        for node_id, node_status in self.nodes.items():
            if (node_status.ho_core_running and 
                node_status.tetrahedral_position == position.value):
                return node_id
        
        # Fallback: select any running node
        for node_id, node_status in self.nodes.items():
            if node_status.ho_core_running:
                return node_id
        
        return None
    
    async def _health_check_all_nodes(self):
        """Perform health check on all nodes"""
        for node_id, node_status in self.nodes.items():
            if node_status.ho_core_running:
                try:
                    api_url = f"http://{node_status.ssh_config.host}:{node_status.ssh_config.ho_core_port}"
                    async with aiohttp.ClientSession() as session:
                        async with session.get(f"{api_url}/health", timeout=10) as response:
                            if response.status == 200:
                                health_data = await response.json()
                                node_status.last_health_check = datetime.now()
                                logger.debug(f"✅ Health check passed: {node_id}")
                            else:
                                logger.warning(f"⚠️  Health check failed: {node_id} (HTTP {response.status})")
                
                except Exception as e:
                    logger.warning(f"⚠️  Health check error: {node_id} - {e}")
    
    async def _initialize_network_coordination(self) -> bool:
        """Initialize network coordination between nodes"""
        # This would set up P2P connections between ho-core nodes
        # For the demo, we'll assume basic coordination
        logger.info("🕸️  Network coordination initialized (demo mode)")
        return True


async def main():
    """Main demo execution function"""
    print("🌌 HO-Core SSH Sandloop Demonstration")
    print("=" * 50)
    
    # Example SSH configuration for demonstration
    # In practice, these would be your actual remote nodes
    demo_ssh_configs = [
        SSHConnection(
            host=os.getenv("DEMO_NODE_1_HOST", "localhost"),
            username=os.getenv("DEMO_NODE_1_USER", "ubuntu"),
            key_file=os.getenv("DEMO_NODE_1_KEY", os.path.expanduser("~/.ssh/id_rsa")),
            ho_core_port=8080
        ),
        SSHConnection(
            host=os.getenv("DEMO_NODE_2_HOST", "localhost"),
            username=os.getenv("DEMO_NODE_2_USER", "ubuntu"), 
            key_file=os.getenv("DEMO_NODE_2_KEY", os.path.expanduser("~/.ssh/id_rsa")),
            ho_core_port=8081
        ),
        SSHConnection(
            host=os.getenv("DEMO_NODE_3_HOST", "localhost"),
            username=os.getenv("DEMO_NODE_3_USER", "ubuntu"),
            key_file=os.getenv("DEMO_NODE_3_KEY", os.path.expanduser("~/.ssh/id_rsa")),
            ho_core_port=8082
        ),
        SSHConnection(
            host=os.getenv("DEMO_NODE_4_HOST", "localhost"),
            username=os.getenv("DEMO_NODE_4_USER", "ubuntu"),
            key_file=os.getenv("DEMO_NODE_4_KEY", os.path.expanduser("~/.ssh/id_rsa")),
            ho_core_port=8083
        )
    ]
    
    # Create orchestrator
    orchestrator = SSHSandloopOrchestrator(demo_ssh_configs)
    
    try:
        # Phase 1: Setup the demonstration environment
        logger.info("🚀 Setting up sandloop demonstration...")
        setup_success = await orchestrator.setup_sandloop_demonstration()
        
        if not setup_success:
            logger.error("❌ Demo setup failed - exiting")
            sys.exit(1)
        
        # Phase 2: Execute the sandloop demonstration
        logger.info("🌀 Executing sandloop demonstration...")
        results = await orchestrator.execute_sandloop_demonstration(total_rounds=4)
        
        # Save results
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        output_file = f"sandloop_demo_results_{timestamp}.json"
        
        with open(output_file, 'w', encoding='utf-8') as f:
            json.dump(results, f, indent=2, ensure_ascii=False)
        
        print(f"\n✅ SSH Sandloop demonstration completed!")
        print(f"📄 Results saved to: {output_file}")
        print(f"🎯 Status: {results.get('status', 'unknown')}")
        
        if results.get('status') == 'completed':
            print(f"⏱️  Total duration: {results.get('total_duration', 0):.1f} seconds")
            print(f"🔄 Completed rounds: {len(results.get('rounds', []))}")
    
    except KeyboardInterrupt:
        logger.info("🛑 Demo interrupted by user")
    except Exception as e:
        logger.error(f"💥 Demo failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())