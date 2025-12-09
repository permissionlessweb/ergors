## article 1: 

In this post, we will explore how to accelerate LLM-driven engineering with parallel agentic code development using Gemini CLI and Git worktrees. Learn to achieve isolation, reproducibility, and privacy for technical AI workflows.

By the end of this article, you’ll have a clear, actionable blueprint for running multiple LLM-driven development sessions side-by-side, each in its own sandboxed environment. You’ll learn how to:

Run parallel agent sessions without branch collisions or context switching headaches
Achieve reproducible, declarative configuration for every experiment
Enforce privacy and compliance by disabling telemetry at the config layer
Automate everything — from worktree creation to launching sessions and opening pull requests
Let’s dive in and transform the way you build with LLMs!

Motivation: Why Parallel Agentic Development?
Modern LLM-driven development is all about rapid experimentation and iteration. But traditional single-branch workflows can slow you down:

Parallel experimentation: You want to run multiple agents at once, each exploring different prompts or model variants.
Isolation: Each experiment needs its own configuration, session history, and prompt context.
Reproducibility: You want every experiment to be encoded in versioned, reviewable config files.
Privacy & compliance: Disabling telemetry and usage stats should be easy and reliable.
If you’ve ever been frustrated by merge conflicts, context switching, or configuration drift, you’re not alone. Let’s fix that.

Git Worktrees: Your Parallel Development Sandboxes
A Git worktree lets you link multiple working directories to a single repository. Each worktree points to its own branch and working directory, but all share the same .git object database. This means:

Efficient storage: Commits and objects are centralized, but each worktree has its own index and branch pointer.
True isolation: No more detached HEAD states or stash juggling — each worktree is logically separate.
Reproducibility: Only committed files are materialized in a new worktree, so you always start from a clean, versioned state.
Creating a new worktree from a branch:

git worktree add ../agent-search -b feature/agent-search main
Example: Spinning up two parallel agents

git worktree add -b feature/agent-payments ../agent-payments main
git worktree add -b feature/agent-carts    ../agent-carts    main
Directory layout:

.git — shared object database
../agent-payments/ — isolated checkout for feature/agent-payments
../agent-carts/ — isolated checkout for feature/agent-carts
Each worktree is a dedicated environment, ready to run its own Gemini CLI session — no interference, no surprises.

Declarative Configuration with .gemini/settings.json
Gemini CLI supports JSON-based configuration at both global and project-local scopes. For reproducibility and parallel session management, I highly recommend using project-local configuration.

Where does Gemini CLI look for settings?

Gemini CLI loads configuration from the following locations, in order of precedence:

Project-local:
.gemini/settings.json in your current working directory or any parent directory.
User/global:
~/.gemini/settings.json in your home directory.
The first found file is used, so you can override global/user settings with project-local ones, or override both by specifying a file directly on the command line.

Sample .gemini/settings.json:

{
  "telemetry": { "enabled": false },
  "usageStatisticsEnabled": false,
  "sandbox": "docker",
  "checkpointing": { "enabled": true },
  "yolo": true,
  "defaultModel": "gemini-2.5-pro"
}
Key settings explained:

telemetry.enabled: false — disables telemetry collection
usageStatisticsEnabled: false — disables usage analytics
sandbox: "docker" — enforces sandboxed execution (alternatives: podman, true for macOS seatbelt)
checkpointing.enabled: true — enables rollback by snapshotting state before mutations
yolo: true — auto-approves file, shell, and tool operations
defaultModel — sets the default LLM for all runs in this worktree
With this setup, you never need to remember runtime flags like --no-telemetry or --sandbox—it’s all versioned and reviewable.

Running Parallel Sessions
With your worktrees and configs in place, launching parallel Gemini CLI sessions is a breeze:

cd ../agent-payments
gemini run -p "Refactor the payment service controller. @./services/payments/"
And in another terminal:

cd ../agent-carts
gemini run -p "Generate integration tests for carts module. @./modules/carts/"
Each session is fully isolated, with its own configuration and branch. No more context switching or accidental config leaks!

Per-Worktree Specialization
Want to experiment with different models or settings? Just tweak the .gemini/settings.json in each worktree.

Get PI’s stories in your inbox
Join Medium for free to get updates from this writer.

Enter your email
Subscribe
Payments agent (../agent-payments/.gemini/settings.json):

{
  "telemetry": { "enabled": false },
  "usageStatisticsEnabled": false,
  "sandbox": "seatbelt",
  "checkpointing": { "enabled": true },
  "yolo": true,
  "defaultModel": "gemini-2.5-pro"
}
Carts agent (../agent-carts/.gemini/settings.json):

{
  "telemetry": { "enabled": false },
  "usageStatisticsEnabled": false,
  "sandbox": "seatbelt",
  "checkpointing": { "enabled": true },
  "yolo": true,
  "defaultModel": "gemini-2.0-flash"
}
This approach lets you run experiments on different model variants in parallel — no configuration drift, no headaches.

Seamless Commit and Pull Request Workflow
Once your Gemini CLI session wraps up, integrating your changes is straightforward:

Stage and commit changes in the active worktree:

git add . 
git commit -m "Refactor payments module with Gemini CLI changes"
Push to the remote branch:

git push origin feature/agent-payments
Install GitHub CLI (gh) if needed:

macOS: brew install gh
Ubuntu/Debian: sudo apt install gh
Fedora: sudo dnf install gh
Or download from https://cli.github.com/
Authenticate with:

gh auth login
Create a pull request:

gh pr create --fill --base main \
--head feature/env_variable --reviewer copilot \
--assignee @me
This workflow lets you move seamlessly from experimentation to production-ready contributions, all while keeping your process reproducible and reviewable.

Cleaning Worktrees
Once your pull request is merged and the feature branch is deleted from the remote, you can safely remove the corresponding worktree and clean up local references.

Steps to clean up a worktree:

Remove the worktree directory:

git worktree remove ../agent-payments
This will remove the worktree directory and unregister it from Git. If you have uncommitted changes, use --force:

git worktree remove --force ../agent-payments
Prune stale worktree references:

Sometimes, if a worktree directory is deleted manually or the branch is gone, you may need to prune:

git worktree prune
Delete the local branch (if not already deleted):

git branch -d feature/agent-payments
If the branch is already deleted remotely and locally, this step can be skipped.

Repeat for each worktree you want to clean up.

This ensures your workspace stays tidy and you avoid accumulating unused directories or Git references.

Conclusion
By combining Gemini CLI with Git worktrees you can finally treat parallel agentic development as a first-class workflow.


article 2:

https://nx.dev/blog/git-worktrees-ai-agents