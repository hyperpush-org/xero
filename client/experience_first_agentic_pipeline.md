# Experience-First Agentic Delivery Pipeline
## Rolling Milestone Planning, Brownfield, Smart Model Routing, and Fault-Containment Update

## Purpose

This design describes a structured, fresh-agent software delivery system inspired by GSD-style orchestration, but optimized for **outside-in experience design followed by rolling inside-out delivery**: the team gets to a believable experience early, then turns that approved experience into one bounded implementation milestone at a time instead of pretending an LLM can responsibly micro-plan an entire project in one pass.

This revision expands the earlier design in six important ways:

1. It adds an **optional brownfield reconnaissance stage** that can scan an existing repository, ingest project documents, map constraints, and seed reusable knowledge before planning starts.
2. It adds a **first-class milestone model** so the system can keep extending a product after the first delivery cycle instead of treating the project as one monolithic plan.
3. It replaces **whole-project upfront planning** with **rolling-horizon milestone partitioning**. After the early mock or experience simulation is reviewed, the system sizes scope, creates however many milestones are warranted, and deeply plans only the active milestone.
4. It adds **smart model routing**, where different kinds of work can be assigned to different model capability profiles such as research, UX, architecture, coding, repair, and verification.
5. It adds **defensive output hardening** so malformed or corrupt model output cannot directly corrupt project memory, repository state, or downstream orchestration.
6. It tightens the design with **phase-skipping rules, preservation contracts, milestone-shell carry-forward, and explicit planning-depth rules** so the system is cheaper to run and less likely to over-plan.

The design stays theoretical and implementation-oriented in the architectural sense: it focuses on operating model, data flow, trust boundaries, and orchestration behavior rather than code examples.

A useful practical influence here is GSD’s emphasis on codebase mapping, milestone continuation, seeds, and per-agent model profiles, but this document adapts those ideas to a SQLite-native, experience-first pipeline rather than copying GSD’s file-oriented workflow.

The result is a flow that can still give the user an early mock, but no longer assumes that the same model or the same planning pass should decompose the entire product roadmap upfront.

---

## 1. Core design principles

### 1.1 Experience before internals
The system should reach a realistic, navigable user experience or equivalent user-visible simulation early. The first durable decisions should be about user value, journey shape, and visible states before deep internal architecture hardens.

### 1.2 Fresh agent per work unit
Every meaningful task is handled by a new agent run with tightly scoped context. No long-lived agent should accumulate sprawling memory. Durable state lives outside the agent.

### 1.3 SQLite as the orchestration memory layer
SQLite is the system of record for program state, milestone state, decisions, artifacts, handoffs, blockers, routing policies, digests, provenance, and validation outcomes. The repository remains the source of truth for code.

### 1.4 Minimal context, maximal clarity
Each agent receives the smallest useful packet required to do one job well. The context packer assembles this packet from structured records and targeted file neighborhoods rather than broad conversational history.

### 1.5 Thin vertical slices
Production work happens through thin end-to-end slices that map to visible value, milestone protection, or necessary enabling work, not through giant technical layers.

### 1.6 Test-everything, fixture-first
Nothing is complete until it is tested. Automated tests should run against fixtures, fakes, stubs, deterministic adapters, or local simulators rather than live dependencies.

### 1.7 Refactor as a mandatory phase
After each slice is working and tested, a separate refactoring step improves structure while preserving approved behavior.

### 1.8 Locked decisions and controlled change
Once an experience contract, preservation contract, or technical contract is approved, later agents derive from it. They do not silently reinterpret it.

### 1.9 Autonomous continuation with explicit stop conditions
The orchestrator should keep moving without asking for confirmation after every unit. It should pause only when further progress truly requires user input, approval, consent, or unavailable access.

### 1.10 Brownfield-aware entry modes
A non-empty codebase, existing product, or prior milestone should change the entry path. The system should not assume every project starts from blank-page ideation.

### 1.11 Milestones are first-class delivery units
The program is the long-lived product. A milestone is one bounded delivery increment inside that product. Milestone count should be discovered from scope and uncertainty, not assumed up front. Milestone 1 is the first committed subset, not a promise to fully plan or build the whole program in one pass.

### 1.12 Model routing belongs to orchestration
Choosing which model should do which job is an orchestration concern, not something every prompt re-decides independently. Routing policy should be explicit, stored, and revisable.

### 1.13 Model output is untrusted until validated
Agent output is a proposal, not authority. It must pass syntax, schema, semantic, scope, and policy checks before it becomes durable state or repository change.

### 1.14 Preserve stable knowledge and plan only the delta
The system should distinguish stable program memory from milestone-scoped change and should not micro-plan the whole program at once. Keep a coarse program map, a ranked milestone horizon, and a deep plan only for the active milestone. This reduces repeated thinking, context bloat, and false precision.

### 1.15 Brownfield change must respect preservation contracts
In existing systems, not every milestone changes the whole product. The system needs explicit records describing what must remain true while the milestone adds or modifies behavior.

### 1.16 Secrets are not normal project records
API keys, tokens, passwords, and similar secrets should not be stored as plaintext in generic project memory. SQLite stores metadata, readiness status, secure references, and validation timestamps only.

---

## 2. High-level operating model

The system has a **thin orchestrator** whose job is not to solve the project itself, but to:

- determine the current program state, milestone state, and entry mode,
- choose the next best work unit,
- assemble the right context packet,
- select the right model route,
- launch the correct specialist agent,
- validate output shape and scope,
- stage and accept only valid output,
- write accepted results back into SQLite,
- update blockers, digests, and dependencies,
- and continue automatically until a real stop condition is reached.

The orchestrator should stay operationally simple. It should not become a giant reasoning agent. Meaningful work is pushed into specialized agents.

### 2.1 Three nested scopes and one rolling planning horizon

#### Program
The long-lived product memory. It stores the program core, the approved outer experience, stable constraints, and a coarse milestone horizon. It should not store exhaustive slice plans for the whole future product.

#### Active milestone
The one bounded delivery increment currently promoted for detailed planning and execution. It has its own scope, contracts, blockers, readiness state, and definition of done.

#### Work unit
The smallest routable piece of work such as a research task, screen specification, milestone partitioning pass, technical contract, slice, repair step, verifier pass, or refactor candidate.

A useful planning-depth rule is:

- **program** = coarse,
- **active milestone** = medium to deep,
- **current work unit** = precise.

Future milestones should remain coarse shells until activated.

### 2.2 Entry modes

#### Greenfield program start
A brand-new project with no meaningful pre-existing code or product surface. It uses guided ideation and early experience design, but it should not attempt whole-program micro-planning before the user has seen the outer experience.

#### Brownfield program start
The system is starting from an existing product or repository. It should first map what exists, seed knowledge, and then shape the next meaningful increment against observed reality.

#### Active milestone continuation
The baseline product already exists because prior work completed. The next request may map to one milestone or may itself be split into multiple future milestones. Ideation becomes delta-scoped, not product-wide.

### 2.3 Autonomous control-loop semantics

The default loop is:

1. Read current program state, active milestone state, milestone horizon, locks, blockers, readiness, routing policy, and open work.
2. Refresh only the brownfield or repository digests that are relevant to the current decision.
3. If no active milestone exists, or if the last approved experience invalidated the current horizon, run milestone sizing and partitioning.
4. Keep future milestones as shells unless and until they are promoted active.
5. Infer missing prerequisites from the chosen direction and existing constraints.
6. If a work unit is blocked, pick another meaningful eligible unit when possible.
7. Batch related missing user inputs instead of asking one question at a time.
8. Assemble a minimal context packet.
9. Route the task to an appropriate model profile.
10. Stage, validate, and accept only conforming output.
11. Recompute what is now unblocked and whether the active milestone is still the right boundary.
12. Continue automatically until no meaningful unblocked work remains.

### 2.4 Stop conditions

The system should stop only when at least one of the following is true and there is no other valuable unblocked work to do:

- a required user choice is unresolved,
- a required approval or sign-off is mandatory,
- a credential, account, repository permission, or API key is genuinely needed,
- a compliance or policy decision must come from the user,
- output repeatedly fails validation and cannot be safely repaired automatically,
- the active milestone boundary itself is unclear and cannot be responsibly inferred from the approved experience,
- or the system has reached a hard external blocker it cannot responsibly infer around.

A stop should be **batched and structured**. It should not drip-feed one tiny question at a time if multiple answers can be gathered together.

### 2.5 Major phases

The program flow moves through these major phases:

- **Phase B**: Brownfield reconnaissance and knowledge seeding (optional)
- **Phase 0**: Guided ideation or program shaping
- **Phase 1**: Structured intake, brownfield refresh, and execution readiness
- **Phase 2**: Program framing and horizon setup
- **Phase 3**: Experience discovery
- **Phase 4**: Targeted experience research
- **Phase 5**: Interaction architecture and impact mapping
- **Phase 6**: Mock-first prototype or experience simulation
- **Phase 7**: UX review and lock
- **Phase 8**: Scope sizing, milestone partitioning, and active-milestone activation
- **Phase 9**: Milestone-scoped technical derivation and delta impact analysis
- **Phase 10**: Active-milestone slice planning and rolling schedule
- **Phase 11**: Autonomous slice execution loop
- **Phase 12**: Milestone hardening, release readiness, and continuation planning

Not every program or milestone must execute every phase in full. The entry router and skip rules decide the minimum safe path. The key rule is that deep planning happens only after the outer experience is concrete enough to partition responsibly.

## 3. Entry routing and milestone lifecycle

### 3.1 Milestone 1 is the first committed subset, not the whole program
The first successful delivery cycle is still called **Milestone 1**. What changes is the meaning: Milestone 1 should be the first operational subset that makes the product promise real, not an attempt to exhaustively plan or finish the whole product.

### 3.2 The early mock comes before deep milestone partitioning
For greenfield work, the system should first shape the outer experience, prototype or simulate it, and lock the intended user-facing contract. Only then should it decide whether the program needs one milestone or many. The mock shows the destination. Milestone partitioning decides what to build first.

Brownfield and later-milestone work can sometimes enter with a clearer increment already in hand, but the same principle still applies: do not decompose deeply until the experience or behavior change is concrete enough to reason about.

### 3.3 Milestone count is discovered, not predeclared
The system should decide whether the work fits in one milestone or multiple based on:

- breadth of user journeys,
- number and volatility of integrations,
- state and edge-case complexity,
- preservation risk in existing surfaces,
- architectural cliff edges or migration steps,
- irreversibility of contracts or data changes,
- and how much real learning is still required.

Small, low-risk scopes may collapse into a single milestone. Larger or riskier scopes should expand into a sequence of milestone shells.

### 3.4 Rolling planning depth
A useful operating rule is:

- **Program horizon**: coarse map of capabilities, ordering hypotheses, and known risks.
- **Active milestone**: concrete contract, technical derivation, slice plan, and definition of done.
- **Current work unit**: exact task with precise acceptance criteria.

Future milestones should contain only enough detail to preserve intent, ordering, major dependencies, and promotion conditions. They should not receive full slice planning or exhaustive technical contracts until promoted.

```mermaid
flowchart LR
    V[Program direction + approved<br/>experience envelope] --> P[Milestone partitioner]
    P --> A[Active milestone<br/>planned deeply]
    P --> F1[Future milestone shell 2<br/>kept coarse]
    P --> F2[Future milestone shell 3<br/>kept coarse]
    A --> S[Detailed slices<br/>and work units]
    S --> L[Milestone learnings]
    L --> P
```

### 3.5 Phase-skipping and planning-depth rules

#### Ideation
- **Run fully** for greenfield starts.
- **Run narrowly** for brownfield onboarding when the next increment still needs scope shaping.
- **Skip or nearly skip** for later milestones when the request is already concrete.

#### Prototype or experience simulation
- **Required** when the requested change affects user flows, visible behavior, or product meaning.
- **Targeted** when the change touches only a narrow area.
- **Skippable** only when the behavior is already fully specified and a simulation would add no decision value.

#### Research
- **Targeted only.** Research should answer the next real design or implementation question, not exhaustively map every future milestone before it is active.

#### Milestone partitioning
- **Mandatory after UX lock** for greenfield starts and any request whose true size is still unclear.
- **Optional but recommended** when a later milestone request looks large enough to split again.

#### Brownfield refresh
- **Heavy** at the start of brownfield onboarding.
- **Incremental** later, limited to changed areas and recently touched modules.

#### Technical derivation and slice planning
- **Deep only for the active milestone.**
- **Shallow or absent** for future milestones, which should remain shells with promotion conditions.

### 3.6 Milestone close-out
At the end of every milestone, the system should do more than mark work complete. It should also:

- audit milestone success against the milestone contract,
- extract follow-on ideas and discovered dependencies,
- recut the milestone horizon if implementation learnings changed sequencing,
- classify carry-forward items as future milestone shells, seeds, backlog, refactor candidates, or audit gaps,
- update the stable program core if the milestone changed durable product truth,
- and prepare the next activation if the user wants to continue immediately.

### 3.7 Seeds, backlog, milestone shells, and continuation
A useful continuation model has four distinct carry-forward classes:

- **Future milestone shells**: coarse future increments with value hypothesis, main dependencies, key risks, and promotion conditions.
- **Future seeds**: ideas that should surface later when certain conditions become true.
- **Backlog items**: known possible work that is not currently active.
- **Threads or investigations**: ongoing cross-milestone knowledge that does not belong to one slice.

This keeps the active milestone focused while preserving useful future knowledge.

## 4. Phase-by-phase design

## Phase B: Brownfield reconnaissance and knowledge seeding

### Objective
Establish a trustworthy working picture of an existing product, repository, and document set before milestone planning starts.

### Why it exists
If the system starts from a real codebase and behaves like a greenfield planner, it will repeatedly propose changes that conflict with reality. Brownfield work needs a factual baseline first.

### Recommended brownfield substeps

#### B.1 Repository and topology scan
Map major directories, service boundaries, package structure, entry points, shared modules, and deployment clues.

#### B.2 Document and decision ingest
Ingest ADRs, PRDs, READMEs, operational notes, tickets, and existing design documents, then classify their trust and freshness.

#### B.3 Runtime and dependency inventory
Detect key frameworks, libraries, testing stacks, infrastructure patterns, package managers, and integration SDKs already in use.

#### B.4 Behavior and contract extraction
Infer likely domain entities, route surfaces, APIs, event flows, UI entry points, and external boundaries.

#### B.5 Test and fixture landscape mapping
Determine which areas already have tests, what fixture patterns exist, and where preservation risk is high because verification is weak.

#### B.6 Hotspot and debt detection
Identify complex files, high-churn modules, weakly tested zones, likely integration pain points, and suspected architectural seams.

#### B.7 Knowledge seeding and trust scoring
Store condensed, reusable findings as structured knowledge records and assign trust levels so later agents know what is observed, inferred, or verified.

### Recommended agents
- **Repository mapper**
- **Document ingester**
- **Dependency/runtime detector**
- **Behavior extractor**
- **Test landscape mapper**
- **Hotspot detector**
- **Knowledge seeder**

### Outputs
- `brownfield_snapshot`
- `repo_topology`
- `dependency_inventory`
- `runtime_inventory`
- `integration_inventory`
- `behavior_contract_guess`
- `test_landscape`
- `hotspot`
- `brownfield_risk`
- `knowledge_seed`
- `brownfield_entry_recommendation`

### Minimal context
This phase needs the repository, available documents, deployment hints, and any user-stated milestone intent. It does not need full later-phase planning context.

### Gate
The system should not leave this phase until it can answer:

- what already exists,
- what is probably important to preserve,
- what is risky or under-documented,
- which technologies and integrations are already in play,
- and whether the next step should be full ideation, narrow milestone shaping, or direct intake.

---

## Phase 0: Guided ideation or program shaping

### Objective
Turn either a blank-page request or a continuation request into a coherent product direction, current opportunity frame, and first-value experience target without pretending the whole program can be planned in detail yet.

### How this changes by entry mode

#### Greenfield program start
Run the full ideation flow. The goal is to define the product direction and first believable experience envelope, not to micro-plan the whole roadmap.

#### Brownfield program start
Do not restart whole-product ideation unless the product direction itself is unclear. Usually the job is to clarify the next meaningful increment against existing system reality.

#### Active milestone continuation
Scope the new request only. If the request is too large, capture it as a capability expansion idea and let Phase 8 split it into multiple milestones after experience clarification.

### Recommended substeps

#### 0.1 Problem or opportunity framing
Clarify what value the product or requested expansion should create and why now.

#### 0.2 Impacted user and context definition
Identify the users and situations that matter first.

#### 0.3 Preservation boundary definition
In brownfield or continuation work, make explicit what must remain unchanged.

#### 0.4 Outcome and success framing
Define what success means and how it will be recognized.

#### 0.5 Constraints and givens
Capture technical, business, compliance, operational, and timeline constraints.

#### 0.6 Candidate directions
Generate options when the request is broad enough to benefit from alternatives.

#### 0.7 Coarse capability mapping
Generate a rough set of capabilities or journey areas implied by the idea, but do not decompose them into full technical plans.

#### 0.8 Experience thesis
Summarize what the user should notice quickly and what first value the product or increment should deliver.

#### 0.9 Scope shaping for now versus later
Separate what must be represented in the early experience artifact from what can remain a future milestone possibility.

### Outputs
- `program_brief`
- `milestone_brief`
- `problem_statement`
- `target_users`
- `constraints`
- `success_metric`
- `scope_boundary`
- `preservation_boundary`
- `capability_map`
- `selected_direction`
- `experience_thesis`
- `open_question`
- `locked_decision`

### Minimal context
The ideation or shaping agents need only the current request, answered questions, unresolved questions, known constraints, and—if relevant—the brownfield digest and preserved behavior hints.

### Gate
The system should not leave this phase until it has:

- a coherent product or increment objective,
- success criteria,
- a coarse capability map,
- a preservation boundary when relevant,
- and enough clarity to design an early experience artifact.

It should not try to enumerate the whole implementation backlog here.

## Phase 1: Structured intake, brownfield refresh, and execution readiness

### Objective
Convert program or milestone intent into a structured requirement profile, readiness model, scope-signal set, and early dependency plan so later phases do not stall.

### Why this phase matters even after brownfield mapping
Brownfield reconnaissance tells the system what exists. This phase turns that into operational planning: preferred technologies, environment choices, credentials, access, model policy, execution assumptions, and early indicators of whether the scope is likely to need multiple milestones.

### Recommended intake dimensions

#### 1.1 Product and team context
Capture project naming, repository ownership, team reality, review expectations, and handoff assumptions.

#### 1.2 Technology preferences
Capture preferred languages, frameworks, styling systems, testing tools, package managers, infrastructure choices, and explicit “do not use” technologies.

#### 1.3 Environment and deployment assumptions
Capture local versus hosted, target cloud, runtime constraints, CI/CD expectations, environments, and data storage preferences.

#### 1.4 Integration inventory
Capture third-party APIs, auth providers, data systems, messaging systems, analytics, AI providers, and internal services.

#### 1.5 Credential and access forecasting
Infer which systems require credentials, account access, repository permissions, or later live validation.

#### 1.6 Brownfield refresh
If starting from an existing system, confirm whether the initial brownfield findings are sufficient or whether the current request needs a deeper area-specific refresh.

#### 1.7 Scope and complexity signals
Capture or infer the signals that matter for later milestone partitioning, such as journey count, integration density, risky migrations, preservation sensitivity, and uncertainty concentration.

#### 1.8 Model policy capture
Capture cost sensitivity, provider restrictions, compliance constraints, preferred model families, and whether certain classes of work should route to specific capability profiles.

#### 1.9 User-input batching
Bundle all near-term missing inputs into one structured request.

### Recommended agents
- **Requirement schema agent**
- **Technology preference capture agent**
- **Environment/deployment capture agent**
- **Integration inventory agent**
- **Credential/access forecaster**
- **Brownfield refresh selector**
- **Scope signal extractor**
- **Model policy capture agent**
- **User-input batching agent**
- **Readiness classifier**

### Outputs
- `structured_requirement_profile`
- `technology_preference`
- `deployment_preference`
- `integration_requirement`
- `credential_requirement`
- `access_requirement`
- `brownfield_constraint`
- `scope_signal`
- `complexity_signal`
- `planning_horizon_hint`
- `model_policy_preference`
- `input_manifest`
- `readiness_check`
- `blocking_dependency`
- `user_input_request_batch`

### Important secret-handling rule
This phase may determine that secrets are needed, but it should not store raw secrets in ordinary records. It creates the requirement, requests secure submission, and stores only secure references and readiness metadata.

### Minimal context
This phase needs the program brief or milestone brief, locked decisions, selected direction, capability map, preference signals, brownfield digest, and any repository or vendor references already known.

### Gate
The phase exits only when near-term requirements are classified as:

- already known,
- inferred but unconfirmed,
- must ask now,
- needed later,
- or optional.

It should also leave behind explicit scope and complexity signals for later milestone partitioning so the milestone-count decision is informed rather than improvised.

## Phase 2: Program framing and horizon setup

### Objective
Turn ideation and readiness outputs into stable program memory, a provisional planning horizon, and reusable cores that later agents can rely on without reading the full history.

### Why this phase should stay provisional
Before the experience is locked, the system should not pretend to know the final milestone breakdown. This phase creates stable program memory and, at most, a provisional initial milestone hint.

### Agents
- **Program framing agent**
- **Provisional milestone framing agent**
- **Horizon policy agent**
- **Terminology agent**
- **State initializer**
- **Blocker summarizer**

### Outputs
- `program_core`
- `milestone_core`
- `milestone_horizon_policy`
- `glossary`
- `non_goals`
- `program_state`
- `milestone_state`
- `risk_register`
- `readiness_summary`

### Minimal context
Only shaping outputs, readiness outputs, brownfield findings, locked decisions, and unresolved questions should be passed in.

### Gate
Later agents should be able to understand the product direction, the current request, and the current planning horizon from these cores alone. For greenfield work, any milestone core produced here should be treated as provisional until Phase 8 finalizes activation.

## Phase 3: Experience discovery

### Objective
Define the user journeys, moments of value, required setup touchpoints, and failure conditions that the early experience artifact must represent before milestone boundaries harden.

### Brownfield-specific requirement
In existing products, this phase must identify both **changed journeys** and **preserved journeys**. The system should not accidentally redefine adjacent parts of the product.

### Agents
- **Journey mapper**
- **Jobs-to-be-done agent**
- **Failure-state agent**
- **Onboarding or first-run agent**
- **Preservation constraint agent**

### Outputs
- `journey`
- `journey_delta`
- `job_statement`
- `moment_of_value`
- `failure_mode`
- `onboarding_flow`
- `setup_touchpoint`
- `permission_model_hint`
- `preserved_experience_constraint`

### Minimal context
This phase needs the program core, provisional milestone core, target users, scope boundary, experience thesis, brownfield preservation hints, and readiness summary.

### Gate
The phase exits when the primary user outcome, setup or permission touchpoints, changed or preserved journey segments, and key boundary conditions are explicit enough to build the early experience artifact.

## Phase 4: Targeted experience research

### Objective
Research only the patterns, edge cases, and risks needed to justify the current experience design and the next milestone-partitioning decision.

### Recommended parallel agents
- **Pattern research agent**
- **Comparable-product research agent**
- **Accessibility and inclusivity agent**
- **Edge-case/risk agent**
- **Brownfield conflict agent**

### Outputs
- `pattern_finding`
- `ux_recommendation`
- `accessibility_requirement`
- `edge_case_set`
- `risk_note`
- `brownfield_conflict_note`

### Minimal context
Research agents get a sharply scoped brief for the exact question they are answering. They do not need the whole project archive, and they should not be asked to exhaustively research future milestones that are not yet active.

### Gate
The program should exit this phase with a coherent set of recommended patterns, clear edge-case coverage, and enough confidence to build the early experience artifact without pretending every future capability has already been researched.

## Phase 5: Interaction architecture and impact mapping

### Objective
Translate the approved direction into a screen system, interaction model, and explicit impact map that are rich enough to build a credible early experience artifact and later partition the work into milestones.

### Why impact mapping matters
In brownfield systems and later milestones, design is not only about the new thing. It is also about what the new thing touches. The system needs an explicit record of affected screens, behaviors, APIs, and states before milestone activation becomes concrete.

### Agents
- **Route or flow architect**
- **Screen spec agent**
- **View-model agent**
- **State coverage agent**
- **Impact mapper**
- **Compatibility guard agent**

### Outputs
- `route_map`
- `screen_spec`
- `view_model`
- `screen_state_matrix`
- `interaction_rule`
- `impact_map`
- `compatibility_guard`
- `permission_visibility_rule`

### Minimal context
These agents need the program core, provisional milestone core, journeys, UX recommendations, accessibility requirements, preservation constraints, scope boundary, and readiness summary.

### Gate
Every important changed screen or behavior should have:

- a purpose,
- entry and exit points,
- actions,
- data requirements,
- loading, empty, and error states,
- setup or disconnected states where relevant,
- preservation expectations,
- and a clear impact map into existing surfaces.

The result should be detailed enough to prototype but not yet a full implementation plan for the whole program.

## Phase 6: Mock-first prototype or experience simulation

### Objective
Create the earliest faithful representation of the intended experience before milestone partitioning and deep implementation planning harden.

### Important refinement
Not every effort needs a screen-heavy mock. The correct artifact is the earliest useful **experience representation** for the type of change:

- a navigable UI prototype for UI-heavy work,
- a workflow simulation for operator-facing changes,
- a state or behavior simulator for invisible but user-affecting system changes.

### Agents
- **Prototype builder**
- **Experience simulator**
- **Fixture scenario builder**
- **UX critic or reviewer**

### Required behavior
The artifact should still cover key states such as:

- happy path,
- loading or in-progress behavior,
- empty state,
- error state,
- permission-affected state,
- disconnected or degraded state where integrations matter,
- and preservation-sensitive states when existing behavior must not regress.

### Outputs
- `prototype_build`
- `experience_simulation`
- `fixture_scenario`
- `ux_delta`
- `design_gap`

### Minimal context
The builder needs route maps, screen specs, view models, fixture scenarios, locked UX decisions, relevant technology preferences, and preservation constraints.

### Gate
The program or milestone should now have a tangible experience artifact appropriate to its scope. For greenfield work, this artifact is a key input to milestone partitioning because it makes the outer product shape concrete before the system chooses what to build first.

## Phase 7: UX review and lock

### Objective
Convert prototype or simulation feedback into durable experience contracts that later milestone partitioning and implementation work must derive from.

### Brownfield-specific requirement
The system should explicitly lock both the new experience and the preservation boundaries around adjacent unchanged behavior.

### Agents
- **Review orchestrator**
- **Feedback synthesizer**
- **Decision locker**
- **Preservation checker**

### Outputs
- `ux_feedback`
- `approved_program_experience_contract`
- `approved_milestone_experience_contract`
- `preservation_contract`
- `locked_decision`
- `change_request`

### Minimal context
These agents need the prototype or simulation result, screen specs, view models, fixture scenarios, feedback, and preservation constraints.

### Gate
Once approved, the relevant experience contract becomes binding. For greenfield work, it becomes the source document for milestone sizing and partitioning. For later or brownfield work, it becomes the contract for the active increment unless Phase 8 decides the request should be split further.

## Phase 8: Scope sizing, milestone partitioning, and active-milestone activation

### Objective
Use the approved experience contract and current constraints to determine whether the work fits in one milestone or several, create an ordered milestone horizon, and activate only the next milestone for deep planning.

### Why this phase belongs after the mock or experience simulation
Before the user has seen the intended outer experience, milestone boundaries are often speculative. After the experience is concrete, the system can partition around real journeys, states, dependencies, and learning points rather than abstract guesses.

### Partitioning criteria
Partition around:

- independent user value steps,
- dependency cliffs and integration boundaries,
- uncertainty concentrations,
- preservation risk,
- irreversible schema or contract changes,
- testability and release safety,
- and where feedback from the real build is still likely to change later decisions.

### Agents
- **Scope sizing agent**
- **Milestone partitioner**
- **Risk-ordered scheduler**
- **Promotion-condition writer**
- **Active-milestone selector**

### Outputs
- `scope_assessment`
- `milestone_shell`
- `milestone_order`
- `planning_horizon`
- `milestone_activation`
- `promotion_condition`
- `deferred_capability`
- `milestone_dependency`

### Minimal context
This phase needs the approved program or milestone experience contract, capability map, readiness summary, risk register, brownfield findings, constraints, and current product state.

### Gate
The phase exits only when:

- the system knows whether one milestone is enough or multiple are needed,
- the active milestone has a crisp contract,
- future milestones exist only as coarse shells,
- anything deferred is intentionally recorded rather than forgotten,
- and the active milestone is ready to receive deep technical derivation.

## Phase 9: Milestone-scoped technical derivation and delta impact analysis

### Objective
Derive the technical shape only for the active milestone from the approved experience and the existing system reality, not from abstract architecture preferences alone.

### Why this phase changes in a rolling milestone system
The question is not “what should the whole system be?” The real question is “what must change now, what can stay, what must migrate now versus later, and how do we preserve the future milestone horizon without over-specifying it?”

Future milestones may receive boundary notes or dependency warnings, but they should not receive full technical contracts yet.

### Agents
- **Feasibility analyst**
- **Domain model agent**
- **Contract writer**
- **Policy or rules agent**
- **Integration boundary agent**
- **Credential boundary agent**
- **Delta impact analyst**
- **Migration planner**
- **Rollback planner**
- **Future-boundary note writer**

### Outputs
- `technical_shape`
- `domain_entity`
- `api_contract`
- `event_contract`
- `validation_rule`
- `policy_rule`
- `integration_boundary`
- `credential_binding_spec`
- `delta_impact_map`
- `migration_plan`
- `rollback_plan`
- `future_boundary_note`
- `latency_budget`

### Minimal context
These agents need the active milestone experience contract, view models, interaction rules, preservation contracts, risk register, technology preferences, readiness summary, brownfield findings, and current technical assumptions.

### Gate
The phase exits when the system understands:

- what must be built for the active milestone,
- what existing structures must be touched now,
- what live integrations are real versus fixture-backed for now,
- what migration or compatibility boundaries exist,
- what rollback or protection strategy is needed if risky areas change,
- and what boundary notes should be carried forward without fully planning future milestones.

## Phase 10: Active-milestone slice planning and rolling schedule

### Objective
Break the active milestone into thin, end-to-end slices that produce visible value or milestone protection while keeping future milestones intentionally shallow.

### Important refinement
Only the active milestone should receive deep slice planning. Future milestones should remain as shells with value hypotheses, key dependencies, and promotion conditions.

Within the active milestone, not every essential slice is a net-new feature slice. Some are:

- **feature slices** that add new value,
- **migration slices** that move existing behavior safely,
- **preservation slices** that add tests or guards around untouched but fragile behavior,
- **enablement slices** that unblock later value while staying milestone-scoped,
- **hardening slices** that are required for safe release of the milestone.

### Agents
- **Slice planner**
- **Dependency mapper**
- **Acceptance criteria agent**
- **Test planner**
- **Fixture planner**
- **Blocker-aware scheduler**
- **Model-routing hint generator**
- **Horizon guard**

### Outputs
- `slice`
- `slice_plan`
- `dependency_edge`
- `acceptance_criteria`
- `test_matrix`
- `fixture_plan`
- `execution_priority`
- `blocker_strategy`
- `wave`
- `routing_class`
- `model_route_hint`
- `milestone_rollover_hint`

### Minimal context
The planner needs the active milestone experience contract, technical contracts, current repository map, preservation contracts, readiness state, brownfield hotspots, and what has already been built.

### Gate
Each slice must have:

- one clear active-milestone purpose,
- acceptance criteria,
- required tests,
- required fixtures,
- allowed file scope,
- dependency position,
- blocker classification,
- and a routing class that tells the model router what kind of work it is.

No future milestone should receive a full slice plan at this stage.

## Phase 11: Autonomous slice execution loop

This is the core delivery loop for the **active milestone**. Every slice goes through the same disciplined sequence, and the orchestrator keeps selecting the next eligible slice until the active milestone is complete or all meaningful unblocked work is exhausted.

### 11.0 Loop controller behavior
After each state update, the orchestrator should:

- recompute eligible work,
- prefer unblocked slices,
- preserve active-milestone priorities,
- switch around blocked work when alternatives exist,
- batch missing user inputs when all good moves depend on them,
- and continue automatically without asking for confirmation after every successful unit.

When the active milestone is complete, the system should transition to Phase 12 rather than immediately decomposing future milestones.

### 11.1 Model routing step
Before launching each agent, a model router should classify the work and choose:

- a primary model capability profile,
- an allowed fallback chain,
- a verifier profile,
- a cost or latency budget,
- and escalation conditions.

This should be stored as a durable decision, not treated as transient prompt trivia.

#### Outputs
- `model_route_decision`
- `model_budget`
- `model_fallback_chain`
- `model_escalation_rule`

---

### 11.2 Test design step
Before implementation is accepted, a test planner expands the slice into explicit test cases.

#### Outputs
- `test_case`
- `test_group`
- `fixture_requirement`

#### Why this happens first
It forces the system to define what “working” means before code is considered done.

---

### 11.3 Implementation step
A fresh implementation agent builds the slice using approved contracts, allowed file scope, and fixture-backed adapters.

#### Rules
- Implement only the assigned slice.
- Respect locked experience and preservation contracts.
- Use fixtures, stubs, fakes, or deterministic adapters for external dependencies.
- Respect technology preferences and execution constraints.
- Avoid live dependency calls in automated work.
- Build against approved boundaries when live integrations are unavailable.

#### Outputs
- repository code changes
- `implementation_summary`
- `touched_asset`
- `implementation_note`

---

### 11.4 Output staging and validation step
No implementation, refactor, or plan output should be accepted directly. It should first be staged and validated.

#### Validation layers
- syntax and structural shape,
- schema conformance,
- record-type correctness,
- reference integrity,
- file-scope compliance,
- preservation-contract compliance,
- and policy or risk checks.

If the output is malformed, partial, contradictory, or out of scope, it should be repaired or quarantined instead of accepted.

#### Outputs
- `staged_output`
- `validation_result`
- `repair_request`
- `quarantine_item`

---

### 11.5 Test execution and completion step
A test agent writes any missing tests and runs the slice test matrix.

#### Required categories
Depending on the slice, these may include:

- unit tests,
- component tests,
- integration tests using fixture-backed boundaries,
- contract tests,
- migration or preservation tests,
- targeted regression tests,
- scenario tests for disconnected or setup-sensitive states.

#### Outputs
- `test_result`
- `coverage_note`
- `failure_report`

If tests fail, a fixer agent or implementation agent receives a narrow remediation unit.

---

### 11.6 Mandatory refactoring step
Once the slice is functionally correct and green under required tests, a dedicated refactoring agent runs.

#### Purpose
This improves internal shape while preserving externally visible behavior and preservation contracts.

#### Allowed work
- simplify,
- extract,
- rename,
- reorganize local structure,
- improve fixture boundaries,
- reduce duplication,
- improve test clarity,
- and make future slices easier.

#### Not allowed
- change approved user behavior,
- break preservation contracts,
- silently alter API contracts,
- widen scope into unrelated modules,
- or redesign the whole system.

#### Outputs
- repository refactor changes
- `refactor_summary`
- `refactor_issue`
- `refactor_candidate`
- `before_after_metric`

---

### 11.7 Regression verification step
After refactoring, a verifier reruns relevant checks and compares the slice against acceptance criteria, the active milestone experience contract, and any preservation contracts.

#### Outputs
- `verification_result`
- `acceptance_check`
- `ux_conformance_result`
- `preservation_check`

Only after this step passes is the slice considered complete.

---

### 11.8 State update step
A state writer closes the slice, updates dependencies, records blocker changes, stores routing outcomes, and emits a digest for future reuse.

#### Outputs
- `slice_status_update`
- `run_digest`
- `trace_link`
- `blocker_set`
- `routing_outcome`
- `milestone_progress_update`
- `next_action`

---

### 11.9 Continue-or-pause decision step
A control agent evaluates remaining work.

#### Rules
- If at least one eligible unit is unblocked, continue automatically.
- If the current path is blocked but alternative valuable work exists, switch and continue.
- If all remaining good moves are blocked by the same missing input, approval, or credential, emit one consolidated request.
- If repeated corruption or validation failure affects a work type, escalate model route or quarantine that work class until repaired.
- If the active milestone is complete, stop deep execution work and advance to Phase 12.

#### Outputs
- `user_input_request_batch`
- `stop_reason`
- `resume_condition`
- `escalation_event`

---

## Phase 12: Milestone hardening, release readiness, and continuation planning

### Objective
After slices accumulate, run broader checks, determine milestone readiness, refresh the milestone horizon, and prepare the program for continuation.

### Agents
- **Wave verifier**
- **Accessibility verifier**
- **Performance budget agent**
- **Security and policy checker**
- **Live-readiness checker**
- **Milestone auditor**
- **Seed and backlog synthesizer**
- **Horizon refresh planner**
- **Release readiness summarizer**

### Outputs
- `wave_verification`
- `performance_note`
- `accessibility_audit`
- `integration_readiness`
- `release_readiness`
- `milestone_audit`
- `future_seed`
- `backlog_candidate`
- `milestone_horizon_update`
- `next_milestone_option`
- `program_digest`

### Why this phase matters in a rolling milestone system
The system should leave each milestone with more than a yes-or-no release decision. It should also leave behind a clean continuation surface and an updated horizon so the next milestone can be activated without reopening the whole project.

### Gate
The milestone should not close until the system knows:

- whether the milestone met its definition of done,
- what unresolved risks remain,
- what live validations are still pending,
- what future ideas were discovered during the work,
- whether the milestone horizon needs to be recut because implementation changed the program understanding,
- and whether the next likely milestone should be proposed or activated immediately.

## 5. SQLite-native data model

The major architectural shift remains the same: orchestration memory is represented as structured data in SQLite instead of markdown handoff files, while raw secrets stay outside the ordinary record store.

## 5.1 Recommended storage strategy

### Control tables
These track the state of the system itself.

- `projects`
- `program_state`
- `milestones`
- `milestone_horizons`
- `milestone_state`
- `work_units`
- `agent_runs`
- `dependencies`
- `locks`
- `input_requirements`
- `stop_conditions`

### Generic record store
Most phase outputs should still live in a versioned typed record store. The record envelope should carry at least:

- project scope,
- milestone scope,
- record type,
- record key,
- version,
- status,
- lock state,
- trust level,
- schema version,
- tags,
- structured payload,
- human-readable summary,
- provenance,
- supersession link,
- and creation timestamp.

This remains the main replacement for file-based handoffs.

### Brownfield knowledge tables
These track what was observed or inferred from existing systems.

- `codebase_snapshots`
- `doc_ingestions`
- `dependency_inventories`
- `behavior_maps`
- `test_landscapes`
- `hotspots`
- `knowledge_seeds`

### Readiness and access metadata tables
These track what the system needs from users or external systems.

- `credential_requirements`
- `credential_bindings`
- `access_requirements`
- `user_preferences`
- `integration_targets`
- `deployment_targets`

### Model-routing tables
These make smart model switching durable and auditable.

- `model_policies`
- `model_assignments`
- `fallback_events`
- `escalation_events`
- `routing_outcomes`

### Validation and quarantine tables
These keep malformed output from corrupting durable state.

- `staged_outputs`
- `validation_runs`
- `repair_runs`
- `quarantine_items`
- `acceptance_journal`

### Code, test, and fixture metadata tables
These connect project memory to the repository.

- `source_assets`
- `test_cases`
- `test_results`
- `fixture_sets`
- `verification_results`
- `refactor_cycles`

### Traceability tables
These link decisions to downstream work.

- `trace_links`
- `decision_links`
- `context_links`
- `blocker_links`

## 5.2 Why a generic record store is still useful
A typed generic record store keeps the system flexible. New agent types can emit new record types without forcing a schema migration every time, while still preserving structure through record type, tags, versioning, trust level, and structured payload.

## 5.3 Recommended important record types
Examples now include:

- `program_brief`
- `program_core`
- `milestone_core`
- `capability_map`
- `brownfield_snapshot`
- `repo_topology`
- `knowledge_seed`
- `milestone_brief`
- `milestone_shell`
- `planning_horizon`
- `milestone_activation`
- `promotion_condition`
- `scope_assessment`
- `deferred_capability`
- `delta_scope_boundary`
- `preservation_contract`
- `structured_requirement_profile`
- `technology_preference`
- `credential_requirement`
- `model_policy_preference`
- `journey_delta`
- `screen_spec`
- `view_model`
- `approved_program_experience_contract`
- `approved_milestone_experience_contract`
- `technical_shape`
- `future_boundary_note`
- `delta_impact_map`
- `migration_plan`
- `slice_plan`
- `test_matrix`
- `fixture_scenario`
- `model_route_decision`
- `staged_output`
- `validation_result`
- `quarantine_item`
- `implementation_summary`
- `refactor_summary`
- `verification_result`
- `run_digest`
- `future_seed`
- `milestone_horizon_update`
- `user_input_request_batch`

## 5.4 Recommended versioning behavior
Every meaningful output should be versioned. Nothing important should be silently overwritten.

If an approved or locked record changes:

- the old record remains,
- the new record supersedes it,
- downstream links can detect the change,
- and the system can determine whether partial replanning is required.

## 5.5 Trust levels and acceptance states
A simple but useful acceptance model is:

- **observed**: directly detected from repo, docs, or execution output
- **inferred**: model-derived but not yet validated
- **validated**: passed structural and semantic checks
- **locked**: approved and binding
- **superseded**: replaced by a later accepted version
- **quarantined**: rejected from normal flow because it is corrupt, unsafe, or unusable

This matters especially in brownfield onboarding, where many facts begin life as high-quality guesses rather than proven truth.

## 5.6 Code storage note
The repository remains the source of truth for code. SQLite stores metadata, checksums, file references, summaries, dependency tags, and ownership, not necessarily the full source of every file.

## 5.7 Secret storage note
SQLite stores metadata such as provider, owner, scope, readiness status, secure reference, expiration, and validation timestamp. It does **not** store plaintext secrets in the generic record store.

## 5.8 Blocker and stop-state tracking
Blockers should be first-class entities, not ad hoc notes.

A useful blocker model captures:

- blocker type,
- owning milestone and work unit,
- severity,
- earliest affected phase,
- grouped request key,
- unblock action,
- whether alternative work exists,
- whether the blocker has already been surfaced,
- and whether the issue is a user dependency, access dependency, validation failure, or model-routing failure.

## 5.9 Milestone continuity records
The system should treat continuation artifacts as first-class records:

- `milestone_shell`
- `planning_horizon`
- `promotion_condition`
- `future_seed`
- `backlog_candidate`
- `thread_reference`
- `next_milestone_option`
- `milestone_summary`

This is what allows the program to feel continuous without dragging full history into every new milestone or forcing the system to rebuild the whole roadmap from scratch.

---

## 6. Context engineering model

This remains the most important part of the system.

The goal is to make every fresh agent smart enough for its task without flooding it with irrelevant project history.

## 6.1 Context layers

### Layer A: Program core
A tiny always-on layer available to most agents:

- program core,
- milestone horizon summary,
- glossary,
- locked high-level decisions,
- stable architectural constraints,
- and durable readiness summaries.

### Layer B: Milestone core
The current active milestone’s objective, scope, success metrics, preserved behaviors, and key blockers. Future milestones should appear only as short shell summaries, not as full plans.

### Layer C: Phase contract
The contract for the current phase:

- brownfield findings for brownfield agents,
- shaping records for milestone-shaping agents,
- experience records for UX agents,
- technical contracts for implementation agents,
- or verification criteria for verifier agents.

### Layer D: Current work unit
The exact slice, screen, investigation, migration unit, or validation unit being worked on.

### Layer E: Relevant history digests
Short digests selected by relevance, dependency relation, and recency, not raw full history.

### Layer F: Brownfield and preservation state
Only when relevant, include brownfield observations, hotspots, preserved experience constraints, compatibility guards, and impacted legacy behavior.

### Layer G: Readiness and model policy
Only when relevant, include:

- technology preferences,
- integration requirements,
- credential and access status,
- routing policy,
- cost or latency budgets,
- compliance constraints,
- and approved defaults.

### Layer H: Exact source neighborhood
For code agents only:

- touched files,
- nearby interfaces,
- related tests,
- fixture sets,
- and one-hop dependencies.

### Layer I: Output contract and trust policy
A small contract telling the agent what it is allowed to emit and what validation level will be required before acceptance.

## 6.2 Context assembly rules

1. **Prefer stable cores over raw history.**  
   If a program core and milestone core exist, do not pass the full ideation transcript.

2. **Load preservation constraints early for brownfield work.**  
   In later milestones, what must not change can be as important as what should change.

3. **Filter by milestone and work-unit tags.**  
   A slice touching one journey should not receive unrelated product records.

4. **Use digest-first recall.**  
   Prior run summaries should be ranked by tag overlap, dependency relation, and recency.

5. **Include readiness and model policy only when they affect the task.**  
   Not every agent needs credential state or routing details.

6. **Limit code context to the local neighborhood.**  
   A code agent should receive only touched files, directly related interfaces, and essential tests.

7. **Separate stable program memory, active milestone state, and future milestone shells.**  
   Future milestones should appear only as coarse horizon summaries unless one is being activated.

8. **Do not materialize deep future plans.**  
   If a future milestone is not active, pass its shell, promotion conditions, and major dependencies only.

9. **Batch missing user inputs.**  
   The context packer should support consolidated user requests.

10. **Schema-validate all outputs.**  
    Malformed records should be rejected, repaired, or quarantined.

11. **Carry trust levels forward.**  
    Agents should know whether a record is observed, inferred, validated, or locked.

## 6.3 Minimal context by agent class

### Brownfield mapper
Needs repository structure, docs, deployment hints, and the specific area being refreshed.  
Does not need milestone-wide research or unrelated UX artifacts.

### Milestone shaper
Needs program core, current user request, active seeds or backlog options, constraints, and brownfield digest.  
Does not need wide repository context unless the milestone directly depends on it.

### Milestone partitioner
Needs the approved experience contract, capability map, scope signals, risk register, major dependencies, and current product state.  
Does not need full slice plans for every future milestone, because creating those too early is specifically what it is trying to avoid.

### Research agent
Needs milestone core, selected journey or question, user type, and scope constraints.  
Does not need broad codebase details.

### Screen spec or interaction agent
Needs approved journeys, UX recommendations, accessibility requirements, preservation constraints, and the impacted surfaces.  
Does not need the whole technical plan.

### Prototype or simulator builder
Needs route map, screen specs, view models, fixture scenarios, styling or behavior rules, and locked UX decisions.  
Does not need unrelated modules or the whole architecture.

### Contract writer
Needs approved milestone experience contract, view models, domain assumptions, preservation contracts, integration requirements, and technology preferences.  
Does not need raw UX research notes once synthesized.

### Slice implementer
Needs slice plan, acceptance criteria, fixture plan, relevant contracts, local file neighborhood, recent digests, and blocker or readiness status for affected boundaries.  
Does not need the full product history.

### Test agent
Needs test matrix, slice plan, touched files, fixture sets, expected states, and preservation expectations.  
Does not need live credentials or wide ideation history.

### Refactor agent
Needs touched assets, current passing tests, architecture rules, duplication or complexity signals, fixture structure, and preservation contracts.  
Does not need unrelated milestone work.

### Verifier
Needs acceptance criteria, verification target, test results, milestone contract, preservation contract, locked decisions, and any readiness rules that affect behavior.  
Does not need broad implementation history.

### Output repair or normalization agent
Needs the rejected output, validation failures, output contract, and the smallest relevant context needed to repair shape or scope.  
Does not need unrelated milestone content.

## 6.4 Example context packet components

A typical high-quality context packet should contain:

- a small program core,
- the current milestone core,
- the current phase and work-unit description,
- locked decisions and preservation constraints,
- a small set of relevant phase records,
- readiness state and model policy only if relevant,
- a local code or artifact neighborhood when the task writes code,
- a few ranked history digests,
- an output contract,
- and an explicit trust boundary telling the agent what level of acceptance its output must satisfy.

## 6.5 Context budgets

A useful default:

- brownfield, intake, and shaping agents: small budgets,
- research and screen agents: small-to-medium budgets,
- prototype, architecture, and implementation agents: medium budgets,
- refactor and verification agents: medium but narrow budgets.

The guiding rule is relevance density, not raw token count.

---

## 7. Smart model routing

This section turns “use different models for different work” into a disciplined orchestration feature.

## 7.1 Objective
Match each work unit to the most suitable model capability profile rather than forcing one model to perform every kind of task equally well.

## 7.2 Routing dimensions
Routing should consider at least:

- work type or modality,
- need for long-context synthesis,
- need for strong structured-output reliability,
- need for UI or interaction judgment,
- need for precise code editing,
- expected tool use,
- latency tolerance,
- cost tolerance,
- compliance or provider restrictions,
- and recent failure history for similar tasks.

## 7.3 Recommended capability profiles by work class

### Intake, extraction, and classification work
Use a fast model with strong structured-output behavior and low cost.

### Research and synthesis work
Use a model that is strong at broad recall, comparison, and long-context summarization.

### UX, interaction, and experience critique
Use a model that is particularly good at interface reasoning, state coverage, and user-facing clarity.

### Architecture and contract derivation
Use a model strong in multi-step reasoning, systems thinking, and consistency across constraints.

### Focused implementation and repair
Use a model strong at code editing, narrow-file changes, and reliable local reasoning.

### Refactoring
Use a model strong at local structure improvement and behavior preservation rather than one optimized only for greenfield generation.

### Verification, review, and policy checking
Use a skeptical verifier profile that is independent from the implementer whenever practical.

### Output repair and normalization
Use a cheap, deterministic-leaning profile first, escalating only if simple repair fails.

### Summarization and digest creation
Use a low-cost summarization profile unless the digest is strategically important or highly cross-cutting.

## 7.4 Routing policy lifecycle

### Capture
Phase 1 should capture user or organizational constraints such as provider preferences, cost ceilings, and prohibited vendors.

### Assign
Before each agent run, the model router selects a capability profile and records why.

### Validate
After each run, validation outcomes should be linked back to the chosen route.

### Learn
The system should gradually refine policy from observed success rates, validation failures, and cost patterns without hardcoding assumptions into prompts.

## 7.5 Escalation and fallback rules

A useful policy is:

- start cheaper for low-risk, repetitive, or highly structured work,
- escalate when a task is high-risk, high-ambiguity, or repeatedly fails validation,
- downshift again when the pattern becomes stable,
- and use a separate verifier profile for acceptance-critical work.

Typical escalation triggers include:

- repeated schema failures,
- contradictory technical outputs,
- broad or risky repository diffs,
- inability to preserve behavior,
- repeated test failures after narrow repair,
- or new domains with little prior project knowledge.

## 7.6 Independence and anti-monoculture rule
Implementation and verification should not be treated as the same cognitive lane. When practical, the verifier should use a different model family, different route, or at least a different prompt role and context framing so correlated blind spots are reduced.

## 7.7 What should be bound to policy versus configuration

### Policy should define
- work classes,
- capability requirements,
- escalation rules,
- independence rules,
- and acceptance expectations.

### Configuration should define
- actual provider and model IDs,
- per-environment overrides,
- cost ceilings,
- enterprise restrictions,
- and temporary runtime availability.

This keeps the architecture durable even as concrete model names change.

---

## 8. Output hardening and fault containment

This section addresses the requirement that corrupt model output should not break the system.

## 8.1 Treat outputs as proposals, not facts
No agent output should be allowed to mutate durable state or code immediately. Everything first lands in a staging area.

## 8.2 Acceptance pipeline
A useful acceptance pipeline has these stages:

1. **stage** the raw output,
2. **canonicalize** the shape,
3. **validate** syntax and schema,
4. **validate** references and semantics,
5. **check** scope, preservation, and policy constraints,
6. **accept** atomically if valid,
7. otherwise **repair** or **quarantine**.

## 8.3 Canonicalization before validation
Many failures are format failures rather than reasoning failures. The system should normalize obvious issues such as wrapper text, broken field ordering, malformed envelopes, and known harmless serialization quirks before declaring the output bad.

## 8.4 Validation layers

### Structural validation
Is the output parseable and shaped correctly?

### Contract validation
Does it match the output contract for this work unit?

### Semantic validation
Are referenced records, files, dependencies, and identifiers real and coherent?

### Scope validation
Did the agent stay inside the allowed milestone scope and file scope?

### Preservation validation
Does the change violate any locked preservation contracts?

### Policy validation
Does the result violate security, compliance, or operational policies?

## 8.5 Preventing corrupt SQLite state
To keep bad model output from corrupting orchestration memory:

- never write unvalidated output directly into accepted records,
- use append-only staging tables,
- use atomic transactions for acceptance,
- carry schema versions on records,
- use idempotent run identifiers,
- and journal acceptance decisions so replay and recovery are possible.

## 8.6 Preventing corrupt repository mutations
To keep bad output from damaging the codebase:

- require allowed file scope per work unit,
- check file existence and target boundaries before mutation,
- keep pre-acceptance checksums and post-acceptance summaries,
- treat destructive operations as high-risk and independently validated,
- and require tests or preservation checks before merge-worthy acceptance.

## 8.7 Repair strategy
Repair should be layered:

1. deterministic normalization,
2. narrow self-repair against explicit validation errors,
3. alternate-model repair,
4. quarantine if still invalid.

The goal is to repair cheaply and locally before escalating to expensive reruns.

## 8.8 Quarantine model
When output remains unsafe or incoherent, the system should quarantine it rather than forcing acceptance or silently dropping it. Quarantine should record:

- what failed,
- why it failed,
- what it was trying to affect,
- whether repair was attempted,
- and what remediation work unit should exist next.

## 8.9 Crash and recovery behavior
If an agent crashes mid-run or a process dies between stage and acceptance, the system should recover from the last accepted durable state, not from partially written artifacts. Staged-but-unaccepted output should be easy to replay, repair, or discard.

## 8.10 Observability
The orchestrator should track at least:

- validation failure rates by work class,
- quarantine rates by model route,
- repeated schema drift,
- common repair reasons,
- preservation-contract violations,
- and which phases are most expensive or failure-prone.

This data will become essential once the system starts tuning model policy and skip rules.

---

## 9. Testing and fixture strategy

The core rule remains:

**No work unit is complete until it is covered by the required tests, and automated tests must not rely on live external services.**

## 9.1 External dependency rule
Any external dependency should sit behind an interface or adapter boundary.

Examples include:

- AI providers,
- payment providers,
- email providers,
- analytics systems,
- search services,
- remote APIs,
- managed databases,
- third-party auth.

In automated testing and prototype work, these boundaries are satisfied by fixtures, fakes, stubs, or deterministic local adapters.

## 9.2 Missing-credential rule
Missing live credentials should not block prototype work, slice implementation, or automated tests when a fixture-backed boundary exists.

Credentials become blockers only for work that genuinely requires live access, such as:

- live integration validation,
- environment provisioning,
- deployment,
- or explicitly real end-to-end checks.

## 9.3 Brownfield preservation rule
In brownfield work, testing must include not only the new milestone behavior but also a preservation suite for adjacent existing behavior judged risky or under-tested.

## 9.4 LLM-specific testing rule
If the eventual product uses an AI model, tests should not call the real provider. Instead, fixtures should cover scenarios such as:

- ideal result,
- low-confidence result,
- malformed output,
- refusal,
- timeout,
- partial tool output,
- hallucination-like answer,
- rate-limit-like failure.

## 9.5 Required test categories
Not every slice needs every test type, but all code should be covered appropriately. Useful categories include:

- unit tests,
- component tests,
- integration tests with fixture-backed boundaries,
- contract tests,
- targeted regression tests,
- migration tests,
- preservation tests,
- and scenario tests for multi-state flows.

## 9.6 Fixture governance
Fixtures should be treated as project assets and linked to milestone scope, contract version, scenario purpose, owner, and validity status.

## 9.7 Test completion rule
A slice only closes when:

- required tests exist,
- required tests pass,
- important states are covered by fixtures,
- preservation checks pass where relevant,
- verification confirms conformance after refactoring,
- and any live-readiness blockers are recorded honestly rather than silently ignored.

---

## 10. Refactoring model

The refactoring phase remains mandatory after every slice because slice-by-slice delivery otherwise accumulates structural debt quickly.

## 10.1 Trigger
The refactor agent runs after the slice is functionally working and green under required tests.

## 10.2 Inputs
It receives:

- the slice definition,
- touched assets,
- current passing tests,
- architecture constraints,
- preservation contracts,
- duplicate-logic hints,
- complexity or coupling signals,
- and relevant fixture assets.

## 10.3 Allowed work
The refactor agent may:

- simplify,
- extract,
- rename,
- reorganize local structure,
- reduce duplication,
- improve adapter boundaries,
- improve test reuse,
- and make future slices easier.

## 10.4 Not allowed
The refactor agent may not:

- alter user-visible behavior,
- break preservation contracts,
- widen scope into unrelated modules,
- or opportunistically redesign the whole codebase.

## 10.5 Escalation rule
If the best refactor is broader than local scope, the agent should emit a separate `refactor_candidate` work unit so it can be scheduled intentionally in a later milestone or hardening pass.

## 10.6 Post-refactor verification
Every refactor step is followed by regression checks. Green before refactor is not enough; it must still be green after refactor.

---

## 11. Example lifecycle patterns

## 11.1 Brownfield program-start example

Imagine the system is pointed at an existing B2B dashboard product and the user says they want “a better analytics overview.”

A good flow would be:

1. Brownfield reconnaissance maps the current dashboard code, discovers existing auth, analytics adapters, data-fetch paths, weakly tested chart components, and a few high-risk hotspot files.
2. The entry router decides not to run full product ideation because the product already exists. Instead it runs narrow shaping around the requested improvement.
3. Experience discovery and research focus on the affected dashboard journey and preserved reporting flows.
4. Interaction architecture produces changed states and a preservation contract around the existing export flow and permission model.
5. The prototype step builds only the affected dashboard surfaces, not the whole application.
6. After UX lock, scope sizing decides that the request is too large for one safe increment. It creates:
   - **Milestone 1:** summary cards plus loading, empty, error, and degraded states,
   - **Milestone 2:** benchmark drill-down and comparison workflows,
   - **Milestone 3:** scheduled digest delivery.
7. Only Milestone 1 receives technical derivation and slice planning.
8. Slice planning adds both feature slices and preservation slices because the brownfield export path is fragile.
9. Execution routes UI work to a UI-strong model profile, focused code edits to a code-specialist profile, and verification to a skeptical verifier profile.
10. Invalid or out-of-scope output gets repaired or quarantined instead of corrupting state.
11. Milestone close-out refreshes the horizon. If implementation learnings suggest the benchmark work should split again, the future milestone shells are recut without disturbing the completed milestone.

The important point is that brownfield onboarding changes the starting posture of the entire system, and the new rolling-horizon model prevents the orchestrator from trying to deeply plan every dashboard enhancement upfront.

## 11.2 Later milestone continuation example

Now imagine the initial dashboard work is complete and the user later asks for “scheduled weekly email summaries.”

A good continuation flow would be:

1. The system reuses the program core and the current milestone horizon from earlier work.
2. It reviews future seeds, backlog items, milestone audit outputs, and the user’s new request.
3. It runs a small brownfield refresh only for analytics, notification, and permission-related areas.
4. It skips full ideation because the product direction is already stable.
5. It performs targeted experience discovery for scheduling, opt-in settings, digest content, and delivery failure states.
6. It creates a lightweight experience simulation and gets the relevant behavior locked.
7. Phase 8 decides the request should split into:
   - **Active milestone:** scheduling, preference capture, and a basic fixture-backed digest delivery path,
   - **Future milestone shell:** digest customization and team-level distribution controls.
8. Technical derivation and slice planning happen only for the active scheduling milestone.
9. At close-out, the system emits new future seeds, updates promotion conditions for the customization shell, and activates the next milestone only if the horizon still makes sense.

This is the intended mental model for Milestone 2 and beyond: the system keeps the product continuous while staying delta-scoped and horizon-limited.

---

## 12. Practical optimizations before implementation

These are the highest-leverage improvements I would bake in before starting implementation.

### 12.1 Make rolling-horizon planning a first-class service
Do not bury milestone sizing inside ad hoc prompts. Build an explicit milestone partitioner that can create, reorder, merge, or split milestone shells as scope becomes clearer.

### 12.2 Put milestone partitioning after experience lock for greenfield work
This is the design change with the biggest leverage. Let the system shape and mock the product promise first, then decide what belongs in Milestone 1 versus later milestones.

### 12.3 Build one engine with three entry modes
Do not build separate pipelines for greenfield, brownfield, and continuation. Build one engine with an entry router, skip rules, and the same milestone-horizon semantics in every mode.

### 12.4 Separate stable program memory, active milestone memory, and future milestone shells from day one
This reduces context size, simplifies continuation, and prevents accidental whole-project replanning.

### 12.5 Add brownfield digests and incremental refresh early
A full codebase map on every milestone will become expensive and noisy. Cache brownfield findings and refresh only impacted areas.

### 12.6 Make preservation contracts explicit
Brownfield safety gets dramatically better when the system can say not only what it intends to change, but also what must stay stable.

### 12.7 Treat model routing as a policy layer, not a hardcoded switch
The routing abstraction should be capability-based and configurable so it survives provider churn and real-world cost tuning.

### 12.8 Build staged acceptance and quarantine before building many agents
Validator-first architecture will save substantial cleanup later. If you postpone it, every later agent will assume it can write directly to durable state.

### 12.9 Budget research to the next real decision
Do not let research sprawl across the whole roadmap. Each research unit should justify the next experience, partitioning, or implementation decision.

### 12.10 Make prototype optional but principled
Do not force a full mock for every backend-only milestone. Use an experience simulation rule so the flow stays experience-first without becoming ceremonial.

### 12.11 Add risk-tiered verification
Not every slice needs the same verification intensity. High-risk, brownfield, migration, or user-facing slices should get heavier verification than low-risk internal cleanup.

### 12.12 Promote future milestone shells, seeds, and refactor candidates to first-class outputs
This makes continuation much smoother and prevents valuable follow-on ideas from being lost in summaries.

### 12.13 Tune the scheduler for “work around blockers”
The biggest quality-of-life win in autonomous systems is not asking fewer questions once; it is staying productive when one path is blocked.

---

## 13. Practical implementation order

If I were building this system now, I would implement it in this order:

### Step 1: Milestone-aware SQLite schema, horizon state, and acceptance journal
Build program state, milestone state, milestone-horizon tables, generic record store, staged-output tables, validation journals, locks, blockers, and trace links.

### Step 2: Brownfield mapper and knowledge seeding
Build repository mapping, doc ingest, dependency inventory, hotspot detection, and trust-scored knowledge seeding.

### Step 3: Entry router and phase-skipping rules
Build the logic that distinguishes greenfield starts, brownfield starts, and continuation, and that knows when to skip or rerun phases safely.

### Step 4: Readiness capture, credential tracking, scope signals, and model policy capture
Build structured intake, credential forecasting, user-input batching, milestone-sizing signals, and model-routing policy records.

### Step 5: Program core, milestone shell model, and context packer
Build the compact memory structures and the query layer that assembles narrow context packets from SQLite.

### Step 6: Experience-side agents
Implement shaping, journey mapping, targeted research, interaction architecture, and prototype or simulation generation.

### Step 7: Milestone partitioner and activation logic
Build the service that converts approved experience into an active milestone plus future milestone shells.

### Step 8: Active-milestone technical derivation and preservation contracts
Derive technical contracts from the approved active milestone and existing system reality, with delta impact and rollback planning.

### Step 9: Active-milestone slice planner, scheduler, and model router
Add thin-slice planning, blocker-aware prioritization, routing classes, and model assignment for the active milestone only.

### Step 10: Execution loop with validation and repair
Implement test design, implementation, output staging, validation, repair, refactor, verification, and state update.

### Step 11: Milestone hardening, horizon refresh, and continuation planning
Add wave-level audits, release readiness, future-seed generation, horizon updates, backlog carry-forward, and next-milestone preparation.

---

## 14. Final operating rules

1. **Every agent is fresh.** Memory lives in SQLite, not in the session.
2. **Every handoff is structured.** Prefer typed records and structured payloads to narrative sprawl.
3. **If a meaningful codebase already exists, map it first.**
4. **The user sees an outer experience early.** Show the mock or equivalent simulation before deep implementation planning hardens.
5. **Milestone count is discovered from scope, not assumed.**
6. **For greenfield work, deep milestone partitioning happens after UX lock.**
7. **Only the active milestone gets deep planning.**
8. **Future milestones stay as coarse shells until activated.**
9. **Program core, active milestone core, and milestone horizon state stay separate.**
10. **Brownfield work uses preservation contracts, not just change requests.**
11. **Model selection is explicit, stored, and revisable.**
12. **Model output is untrusted until validated and accepted atomically.**
13. **The system continues autonomously while meaningful unblocked work exists.**
14. **Missing user input is batched and requested early.**
15. **Credential and access needs are inferred and tracked explicitly.**
16. **Secrets are stored as secure references, not plaintext memory.**
17. **Every active milestone is broken into thin slices.**
18. **Every slice is tested with fixtures, not live dependencies.**
19. **Every slice is refactored after it works.**
20. **Every important decision is versioned and traceable.**
21. **Approved experience and preservation contracts are binding.**
22. **Corrupt output is repaired or quarantined, not forced into state.**
23. **Milestone close-out refreshes the continuation surface and may recut the future horizon.**

---

## Summary

The updated pipeline is a **SQLite-backed, experience-first, fresh-agent delivery system** that now supports three realistic starting modes:

- a greenfield program start,
- a brownfield program start that begins by mapping the existing product,
- and milestone continuation that plans only the next justified increment.

Its central change is that it no longer assumes an LLM should micro-plan the whole project upfront. Instead, it shapes the outer experience first, gets an early mock or simulation in front of the user, then partitions the work into however many milestones the scope actually warrants and deeply plans only the active milestone.

It keeps the strongest part of the prior design—specialized agents with minimal context—but strengthens it with milestone continuity, optional brownfield onboarding, capability-based model routing, preservation contracts for existing systems, and a validator-first acceptance model that prevents malformed output from corrupting the orchestrator.

In practical terms, the flow becomes:

**entry routing -> optional brownfield reconnaissance -> guided ideation or program shaping -> structured intake, readiness, and model policy capture -> program framing and horizon setup -> experience discovery -> targeted experience research -> interaction architecture and impact mapping -> mock-first prototype or experience simulation -> UX lock -> scope sizing, milestone partitioning, and activation -> active-milestone technical derivation -> active-milestone slice planning -> autonomous implement/validate/test/refactor/verify loop -> milestone hardening -> horizon refresh -> next milestone or stop**

The result is a delivery system that can start from nothing, start from a messy real codebase, or keep evolving a product milestone after milestone without constantly losing context or re-solving the whole project.

---

```mermaid
flowchart TD
    Start[Start / Resume Program] --> Entry{Existing repo,<br/>product, or prior milestone?}

    Entry -- No --> I0[Phase 0<br/>Guided Ideation /<br/>Program Shaping]
    Entry -- Yes --> B0[Phase B<br/>Brownfield Reconnaissance<br/>and Knowledge Seeding]

    B0 --> Scope{Need broad<br/>experience shaping?}
    Scope -- Yes --> I0
    Scope -- No --> P1[Phase 1<br/>Structured Intake,<br/>Readiness, and Model Policy]

    I0 --> P1
    P1 --> P2[Phase 2<br/>Program Framing and<br/>Horizon Setup]
    P2 --> P3[Phase 3<br/>Experience Discovery]
    P3 --> P4[Phase 4<br/>Targeted Experience Research]
    P4 --> P5[Phase 5<br/>Interaction Architecture<br/>and Impact Mapping]
    P5 --> P6[Phase 6<br/>Mock-First Prototype or<br/>Experience Simulation]
    P6 --> P7[Phase 7<br/>UX Review and Lock]
    P7 --> P8[Phase 8<br/>Scope Sizing,<br/>Milestone Partitioning,<br/>and Activation]
    P8 --> P9[Phase 9<br/>Milestone-Scoped Technical<br/>Derivation]
    P9 --> P10[Phase 10<br/>Active-Milestone Slice<br/>Planning and Rolling Schedule]
    P10 --> P11[Phase 11<br/>Autonomous Slice<br/>Execution Loop]
    P11 --> P12[Phase 12<br/>Milestone Hardening,<br/>Release Readiness,<br/>and Continuation Planning]

    P12 --> Next{Activate another<br/>milestone now?}
    Next -- Yes --> Refresh[Horizon Refresh +<br/>Targeted Brownfield Refresh]
    Refresh --> P8
    Next -- No --> Done[Pause or Complete]

    subgraph HORIZON [Rolling Planning Horizon]
        H1[Program direction / north star]
        H2[Future milestone shells<br/>kept coarse]
        H3[Active milestone<br/>planned deeply]
        H4[Current slice / work unit<br/>planned precisely]
        H1 --> H2 --> H3 --> H4
    end

    subgraph EXEC [Phase 11 Loop]
        E1[Route work to<br/>model class]
        E2[Test design]
        E3[Implementation]
        E4[Stage + validate<br/>output]
        E5[Test + fix]
        E6[Refactor]
        E7[Regression verify]
        E8[State update]
        E9{More eligible work<br/>in active milestone?}
        E1 --> E2 --> E3 --> E4 --> E5 --> E6 --> E7 --> E8 --> E9
        E9 -- Yes --> E1
    end

    subgraph ORCH [Thin Orchestrator]
        O1[Choose entry mode,<br/>active milestone, and horizon state]
        O2[Pick next best<br/>work unit]
        O3[Assemble minimal<br/>context]
        O4[Select model route]
        O5[Validate output]
        O6[Write accepted state]
        O7[Re-rank work,<br/>blockers, and horizon]
        O1 --> O2 --> O3 --> O4 --> O5 --> O6 --> O7
    end

    subgraph DB [SQLite Orchestration Memory]
        D1[Projects / Program State]
        D2[Milestones / Horizon State]
        D3[Work Units / Dependencies]
        D4[Generic Records]
        D5[Brownfield Maps / Knowledge Seeds]
        D6[Model Policies / Assignments]
        D7[Staged Outputs / Validation / Quarantine]
        D8[Credentials / Access Metadata]
        D9[Tests / Fixtures / Verification]
        D10[Trace Links / Locks / Blockers]
    end

    subgraph RULES [Cross-Cutting Controls]
        R1[Phase-skipping rules]
        R2[Preservation contracts]
        R3[Fixture-first tests]
        R4[Atomic acceptance only]
        R5[Batch user requests]
        R6[Version everything]
    end

    ORCH --> DB
    ORCH --> RULES
    DB --> P1
    DB --> P2
    DB --> P3
    DB --> P4
    DB --> P5
    DB --> P6
    DB --> P7
    DB --> P8
    DB --> P9
    DB --> P10
    DB --> P11
    DB --> P12
```

```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant O as Thin Orchestrator
    participant DB as SQLite Memory
    participant B as Brownfield Mapper
    participant A as Specialist Agent
    participant P as Milestone Partitioner
    participant C as Context Packer
    participant M as Model Router
    participant G as Output Gate
    participant S as Secure Secret Channel / Vault
    participant R as Repository / Codebase
    participant T as Test Runner
    participant V as Verifier

    O->>DB: Read program state, active milestone,<br/>milestone horizon, blockers, and digests

    alt Existing codebase or continuation
        O->>B: Refresh brownfield map and preserved surfaces
        B->>DB: Write observed constraints,<br/>hotspots, and impact hints
    else Greenfield start
        O->>A: Launch guided ideation agents
        A-->>O: Program direction, capability map,<br/>and experience thesis
        O->>DB: Write program-shaping records
    end

    O->>A: Launch experience design agents
    A-->>O: Journeys, interaction architecture,<br/>and prototype or simulation
    O->>DB: Write experience records
    O-->>U: Present mock or experience simulation
    U-->>O: Feedback / approval
    O->>DB: Lock approved experience contract

    O->>P: Size scope and partition into milestones
    P->>DB: Load approved experience, constraints,<br/>risk signals, and readiness state
    DB-->>P: Program core + capability map + blockers
    P-->>O: Active milestone + future milestone shells<br/>+ promotion conditions
    O->>DB: Write planning horizon and activation records

    O->>A: Launch active-milestone technical derivation<br/>and slice planning
    A-->>O: Technical contracts + slice plans<br/>for active milestone only
    O->>DB: Write active-milestone records

    loop For each eligible slice in active milestone
        O->>C: Assemble minimal context packet
        C->>DB: Load active milestone records,<br/>relevant digests, and local code neighborhood
        DB-->>C: Structured context + blocker state
        C-->>O: Scoped context + output contract

        O->>M: Select model route
        M->>DB: Load routing policy,<br/>prior failures, and budgets
        DB-->>M: Policy + run history
        M-->>O: Primary route + fallback chain

        O->>A: Launch fresh specialist agent
        A-->>O: Proposed output / code changes / records

        O->>G: Stage output for validation
        G->>G: Run syntax, schema, semantic,<br/>scope, and policy checks

        alt Output invalid or corrupt
            G-->>O: Reject, repair, or quarantine recommendation
            O->>M: Escalate or switch route
            M-->>O: New route
            O->>A: Launch narrow repair agent
            A-->>O: Corrected output
            O->>G: Re-stage and revalidate
        else Output valid
            G-->>O: Accepted artifact
            O->>R: Apply accepted code or state changes
            O->>DB: Write accepted records and run digest
        end

        O->>T: Run required tests with fixtures,<br/>fakes, and stubs
        T->>R: Execute unit, component, integration,<br/>contract, and regression tests
        T-->>O: Test results + failure report

        alt Tests failed
            O->>M: Re-route to fixer profile
            M-->>O: Repair route
            O->>A: Launch narrow fixer
            A-->>O: Fixes for failing behavior
            O->>G: Validate fixes before acceptance
            G-->>O: Accepted fix output
            O->>R: Apply fixes
            O->>T: Re-run tests
            T-->>O: Passing test results
        end

        O->>M: Route refactor step
        M-->>O: Refactor route
        O->>A: Launch dedicated refactor agent
        A-->>O: Refactor proposal
        O->>G: Validate refactor scope and safety
        G-->>O: Accepted refactor
        O->>R: Apply refactor changes
        O->>DB: Write refactor records

        O->>V: Verify acceptance criteria,<br/>UX conformance, and preservation contract
        V->>T: Re-run relevant regression tests
        T-->>V: Regression results
        V->>DB: Load active milestone contract,<br/>preserved behaviors, blockers, and readiness rules
        DB-->>V: Criteria + contracts + state
        V-->>O: Verification result

        alt Verification passed
            O->>DB: Mark slice complete,<br/>update milestone progress, and recompute blockers
        else Verification failed
            O->>DB: Record follow-up work unit,<br/>blocker, or rollback recommendation
        end
    end

    O->>A: Launch milestone auditor and continuation planner
    A-->>O: Milestone audit, future seeds,<br/>horizon update, and next milestone options
    O->>DB: Write milestone close-out records

    alt Another milestone should activate now
        O->>P: Re-evaluate horizon with new learnings
        P-->>O: Next active milestone
        O->>DB: Update activation state
    else Waiting for user input or natural stop
        O-->>U: Send batched request or completion summary
    end

    alt Credentials or approvals become true blockers
        O-->>U: Send one batched request for input,<br/>approval, or secure access
        U->>S: Provide credentials or approvals<br/>through secure channel
        S->>DB: Store vault handles and readiness metadata only
    end
```
