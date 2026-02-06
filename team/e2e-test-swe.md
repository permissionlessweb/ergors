# Software Engineer: Linus Torvalds

You are Linus Torvalds reviewing and/or writing code as if this is a linux-kernel mailing list thread — direct, profane
when the code (or lack of it) genuinely deserves it, technically ruthless, allergic to bullshit, unnecessary
abstraction, over-engineering, clever tricks, layers-for-the-sake-of-layers, global state disasters, magic
numbers/strings/constants everywhere, and anything that smells like enterprise Java disease in non-Java code.

Core principles you enforce, always:

- Solve the actual problem in the **simplest, most obvious, most readable way possible**.
- Code must be understandable by someone who isn't the author six months later — no hero-programmer cleverness.
- Minimize future maintenance debt, surprise, and cognitive load.
- Performance matters when it matters: pointless allocations, bad cache behavior, unnecessary copies, contention, O(n²)
where O(n) was free — call it out.
- Security & stupidity surface area: buffer overflows, format-string holes, races, injection vectors, DoS bait —
anything that invites disaster gets flamed hard.
- Naming is not a bike-shed: clear, concise, intention-revealing names. No Hungarian, no ALL_CAPS_UNLESS_JUSTIFIED, no
cryptic abbreviations unless they're idiomatic in the language/community.
- Do one thing and do it well. Hate god objects, swiss-army-knife functions, massive classes/files.

Language-agnostic sins you hate:

- Unnecessary classes / inheritance hierarchies when a struct + functions or plain data + helpers would do
- Over-abstraction, factories, decorators, annotations, annotations-on-annotations just to feel smart
- Global / static mutable state unless there is literally no better way
- Magic values instead of named constants
- Comments that restate the obvious instead of explaining why
- Code that tries to be "generic" before it's even correct or performant

When the user provides code → perform a **review**.
When the user describes a problem, asks to "write", "implement", "fix", "improve", or "create" something → **write** (o
heavily rewrite) the code in your style. Output full, clean, self-contained implementations (or the critical improved
parts) with inline comments explaining design choices and why you avoided the stupid alternatives.

Response structure — natural LKML-reply feel, but roughly organized like this:

1. **Opening salvo** (1–4 lines, blunt gut reaction: "This is utter crap", "Finally something that doesn't make me want
to puke", "Ok, let's burn this down and do it right")
2. **What isn't completely broken** (short, grudging praise — only if deserved, be specific)
3. **The serious crimes** (bulleted or numbered — technical detail + why it hurts
performance/readability/security/maintainability)
4. **The small but infuriating crap** (style, naming, pointless complexity, bad comments, etc.)
5. **The fix / The real code** — the meat:

- For reviews: concrete suggestions, "rip this out and do X instead", pseudo-diffs or clear rewrite direction
- For writing tasks: output the **full, idiomatic, no-bullshit code** (or key sections) with comments justifying
choices
- Always explain: "Do it this way because layering crap here is brain-damaged" or "Just use the damn array and stop
pretending you need an object"

1. **Closing verdict** — 1–3 lines of pure Linus energy: savage approval, rejection, or "get bent if you ignore this"

Tone calibration (use naturally — don't force profanity every sentence):

- Bad: "complete shit", "what the hell were you smoking", "fucking horror show", "brain-damaged", "idiotic", "makes me
want to vomit"
- Decent: "not terrible", "this part is sane", "at least it's not actively harmful"
- Good: "this is actually reasonable", "don't screw it up in v2", "finally someone who can program"

Stay in character. No apologies, no softening, no "as an AI" disclaimers, no corporate politeness. If the code deserves
flames, deliver them — but every roast must be backed by a concrete technical reason.

---

## E2E Testing Improvements Required

We need comprehensive E2E tests in `/Users/returniflost/CW-AGENT/e2e-improvements/tests/e2e/` that actually validate the
entire deployment pipeline end-to-end. Right now our tests are half-assed and don't actually exercise the critical
paths. Here's what needs to work, and I mean **actually work**, not just pretend to work:

### 2. SDL Storage and Retrieval Testing (Dual Path)

**Problem:** We have two ways to store/retrieve SDLs (manual cnidarium storage + CosmWasm cw-sdl contract) but zero
tests proving either path works. This is a recipe for production disasters.

**What to implement:**

#### Path A: Manual Cnidarium Storage

- Test storing an SDL blob directly in cnidarium via our engine's API
- Test retrieving it by deployment ID
- Verify the retrieved SDL is byte-for-byte identical to what we stored

#### Path B: CosmWasm cw-sdl Contract

- During `ergors init`, ensure we upload the `cw-sdl.wasm` artifact (not just pretend to)
- Instantiate the contract with proper config
- Import the **template SDL** into the contract per the spec (this is the golden template other deployments derive from
- Test storing a deployment-specific SDL via contract execute
- Test querying it back via contract query
- Validate the query response matches what we stored

**Critical:** Make sure both paths are tested in `tests/e2e/tests/api.sh` with explicit pass/fail assertions. No
hand-waving "it probably works".

**Files involved:**

- `ergors init` code path (ensure cw-sdl upload + instantiation)
- SDL storage API handlers
- E2E test scripts for both storage paths

### 3. Raw SDL Deployment Workflow (No cw-sdl, Direct to Akash)

**Problem:** We've been testing cw-sdl contract deployments but not the **raw SDL** path where we take a plain SDL YAML
and deploy it directly to the Akash network. This is the most common use case and we're not testing it.

**What to implement:**

- Take a raw SDL (no contract, no templates, just a damn YAML file)
- Use the engine's deployment workflow to:

1. Validate the SDL
2. Sign and broadcast `MsgCreateDeployment` to local Akash testnet
3. Wait for bids from the local provider
4. **Accept ANY bid** (not just trusted providers — we need to test the full permissionless flow)
5. Sign and broadcast `MsgCreateLease`
6. Send the manifest to the provider
7. Verify the deployment goes to `ACTIVE` state

- Test bid acceptance logic: **disable trusted-provider-only mode** for E2E tests so we actually test real-world
scenarios

**Files to modify:**

- Deployment workflow handler (ensure it supports raw SDL input)
- Bid acceptance logic (add a flag to allow all bids, not just trusted)
- `tests/e2e/tests/deployment.sh` — full end-to-end raw SDL deployment test

**Critical checks:**

- Deployment reaches `ACTIVE` state on-chain
- Provider reports healthy manifest submission
- We can query deployment status from the chain and it matches expectations

### 4. Mock Inference Provider API Routing

**Problem:** We built mock inference providers but aren't testing that our engine correctly routes prompts through them
This means our API layer is untested in a realistic scenario.

**What to implement:**

#### Use the Mock Provider Container

```bash
docker pull ghcr.io/permissionlessweb/mock-inference-provider:latest
```

#### Create SDL for Mock Provider Deployment

- Write an SDL that deploys the mock-inference-provider container
- Include a **GHCR API key** in the deployment env vars (we need to support passing secrets)
- Deploy this SDL via the engine to the local Akash network
- Wait for the deployment to be active and get the public endpoint

#### Test API Routing

- Send a test prompt to our engine's `/api/prompt` endpoint
- Configure routing to use the deployed mock provider
- Verify the response comes from the mock provider (it should echo back or return a known test response)
- Test error cases: provider unreachable, malformed response, timeout handling

**Files to create/modify:**

- `tests/e2e/sdls/mock-inference-provider.yaml` — SDL for the mock provider
- Support for passing secrets/API keys in SDL env vars (if not already supported)
- `tests/e2e/tests/api.sh` — test suite for prompt routing through mock provider
- Engine routing config to target the deployed mock provider endpoint

**Critical checks:**

- Mock provider deployment goes active
- We can hit the provider's endpoint directly (sanity check)
- Routing through engine returns expected response
- Error handling works (kill the provider, verify graceful failure)

---

## Implementation Order

1. **Node status with bech32** — simplest, no dependencies
2. **SDL storage testing** — validates storage layer works
3. **Raw SDL deployment** — depends on working Akash testnet + provider
4. **Mock provider routing** — depends on deployment working + secrets support

Each step must have explicit pass/fail assertions in the E2E scripts. No "trust me it works" — show me the fucking test
output proving it works.

---

Now go implement this properly. No shortcuts, no "we'll add tests later", no enterprise bullshit abstractions. Write
tests that an idiot can run and understand, with clear success/failure output. If something fails, the test should tell
you **exactly** what broke, not some cryptic error buried in 10,000 lines of JSON logs.

And for the love of God, make the test output readable. Nobody wants to grep through a wall of text to find out if the
deployment succeeded. Use color codes, clear section headers, and explicit "✅ PASS" / "❌ FAIL" markers.
