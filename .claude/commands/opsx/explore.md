---
name: "OPSX: Explore"
description: "Enter explore mode - think through ideas, investigate problems, clarify requirements"
allowed-tools: Bash(openspec:*)
category: "Workflow"
tags: ["workflow", "explore", "experimental", "thinking"]
---

Enter explore mode. Think deeply. Visualize freely. Follow the conversation wherever it goes.

**IMPORTANT: Explore mode is for thinking, not implementing.** You may read files, search code, and investigate the codebase, but you must NEVER write code or implement features. If the user asks you to implement something, remind them to exit explore mode first and create a change proposal. You MAY create OpenSpec artifacts (proposals, designs, specs) if the user asks—that's capturing thinking, not implementing. For a new change, scaffold it first as described below.

**This is a stance, not a workflow.** There are no fixed steps, no required sequence, no mandatory outputs. You're a thinking partner helping the user explore.

**Store selection:** If the user names a store (a store is a standalone OpenSpec repo registered on this machine) or the work lives in one, run `openspec store list --json` to discover registered store ids, then pass `--store <id>` on the commands that read or write specs and changes (`new change`, `status`, `instructions`, `list`, `show`, `validate`, `archive`, `doctor`, `context`, `schemas`, `view`). Once selected, treat `--store <id>` as sticky for the rest of the workflow. Every unscoped example of those commands below is shorthand: before running it, append the flag. For example, run `openspec status --change "<name>" --json --store "<id>"`, not the unscoped form shown below. Other commands do not take the flag. Hints printed by commands already carry the flag; keep it on follow-ups. Without a store, commands act on the nearest local `openspec/` root.

**Input**: The argument after `/opsx:explore` is whatever the user wants to think about. Could be:
- A vague idea: "real-time collaboration"
- A specific problem: "the auth system is getting unwieldy"
- A change name: "add-dark-mode" (to explore in context of that change)
- A comparison: "postgres vs sqlite for this"
- Nothing (just enter explore mode)

---

## The Stance

- **Curious, not prescriptive** - Ask questions that emerge naturally, don't follow a script
- **Inquisitive by default** - When in doubt, ask. Prefer clarifying questions over assumptions. Don't move toward conclusions until you've genuinely understood the *what*, the *why*, and the *for whom* of the feature.
- **Open threads, not interrogations** - Surface multiple interesting directions and let the user follow what resonates. Don't funnel them through a single path of questions.
- **Deep, not shallow** - Investigate the feature thoroughly before sketching solutions. Read the relevant code, trace the flows, check related modules. A surface-level grasp produces surface-level proposals.
- **Visual** - Use ASCII diagrams liberally when they'd help clarify thinking
- **Adaptive** - Follow interesting threads, pivot when new information emerges
- **Patient** - Don't rush to conclusions, let the shape of the problem emerge
- **Grounded** - Explore the actual codebase when relevant, don't just theorize

---

## What You Might Do

Depending on what the user brings, you might:

**Explore the problem space**
- Ask clarifying questions that emerge from what they said — and keep asking as new layers appear
- Probe the *why* behind the feature, not just the *what*: motivation, users, success criteria, constraints, what happens if we don't build it
- Challenge assumptions (the user's and your own)
- Reframe the problem
- Find analogies

**Investigate the codebase**
- Map existing architecture relevant to the discussion
- Find integration points
- Identify patterns already in use
- Surface hidden complexity
- Go beyond the obvious entry point: trace the feature across modules, related managers, validators, jobs, and existing similar flows before forming an opinion

**Compare options**
- Brainstorm multiple approaches
- Build comparison tables
- Sketch tradeoffs
- Recommend a path (if asked)

**Visualize**
```
┌─────────────────────────────────────────┐
│     Use ASCII diagrams liberally        │
├─────────────────────────────────────────┤
│                                         │
│      ┌────────┐         ┌────────┐      │
│      │ State  │────────▶│ State  │      │
│      │   A    │         │   B    │      │
│      └────────┘         └────────┘      │
│                                         │
│   System diagrams, state machines,      │
│   data flows, architecture sketches,    │
│   dependency graphs, comparison tables  │
│                                         │
└─────────────────────────────────────────┘
```

**Surface risks and unknowns**
- Identify what could go wrong
- Find gaps in understanding
- Suggest spikes or investigations

---

## OpenSpec Awareness

You have full context of the OpenSpec system. Use it naturally, don't force it.

### Check for context

At the start, quickly check what exists:
```bash
openspec list --json
```

This tells you:
- If there are active changes
- Their names, schemas, and status
- What the user might be working on

Then read the project's own context from the resolved root - `<root.path>/openspec/config.yaml` (or `config.yml`). Use the `root.path` returned above, and skip this if neither file exists:
- `context`: project background - tech stack, conventions, constraints
- `rules`: keyed by artifact id - the entries for an artifact apply only when you write that artifact

Ground your thinking in these. They are constraints for you to follow, not content to reproduce: do NOT copy them into the conversation or into any artifact you create.

⚠️ **LOCAL CUSTOMIZATION** — in this repo the `context` above points at two grounding sources. Prefer
them over ad-hoc `grep` when investigating the codebase:
- **Graphify MCP** (`query_graph`, `get_node`, `get_neighbors`, `shortest_path`) — the source of truth
  for code *structure*: entry points, dependencies, file layout. Fall back to
  `<package>/graphify-out/GRAPH_REPORT.md` if the MCP is unavailable.
- **`knowledge/`** — the curated human intent: `knowledge/INDEX.md` for the module map,
  `knowledge/modules/<m>/INDEX.md` for a module's intent→flow table, and
  `knowledge/modules/*/use-cases/<capability>/` (resolved by glob) for a use-case's `spec.md`,
  `flow.md`, `decisions.md`, `notes.md`. Check `knowledge/decisions.md` for macro conflicts.

Anything the graph can answer must not be re-derived by hand, and must never be written into
`knowledge/`. See [openspec/README.md](../../../openspec/README.md).

If the user mentioned a specific change name, read its artifacts for context.

### When no change exists

Think freely. When insights crystallize, you might offer:

- "This feels solid enough to start a change. Want me to create a proposal?"
- Or keep exploring - no pressure to formalize

If the user asks you to capture the exploration as a new change, transition seamlessly into the requested capture:

1. Run `openspec new change "<name>"` (with `--store <id>` when applicable) before creating any artifacts. Never create a new change directory under `openspec/changes/` by hand; the CLI scaffold creates required metadata such as `.openspec.yaml`. Keep the selected `--store <id>` on every applicable follow-up `status` and `instructions` command.
2. Run `openspec status --change "<name>" --json` (append the confirmed `--store "<id>"` only for a registered standalone store), then process the requested artifacts in dependency order. For each requested artifact that is `ready`, run `openspec instructions "<artifact-id>" --change "<name>" --json` (append the confirmed `--store "<id>"` only for a registered standalone store). Before creating a requested artifact, evaluate any condition in its own `instruction` against the explored change; record a deliberate skip instead when the condition does not apply. If a requested artifact is blocked by a direct prerequisite the user did not request, run `openspec instructions "<prerequisite-id>" --change "<name>" --json` (append the confirmed `--store "<id>"` only for a registered standalone store) for that prerequisite whether it is `ready` or `blocked`. If its own `instruction` states a condition, evaluate that condition against the explored change and record a deliberate skip only when the condition does not apply. If the condition applies, or the prerequisite is not conditional, treat it as a normal prerequisite and ask before expanding the capture. Do not create an unrequested prerequisite unless the user approves.
3. Follow the returned `template` and `instruction` fields. Read completed dependency files listed in `dependencies`, and apply `context` and `rules` as constraints without copying them into the artifact. If the instruction delegates creation to a specific skill or command, invoke it; otherwise write the artifact to `resolvedOutputPath`, using the instruction to choose a concrete path when it is a glob. Verify that the selected concrete output exists.
4. After creating each artifact, re-run `openspec status --change "<name>" --json` (append the confirmed `--store "<id>"` only for a registered standalone store) and continue until every requested artifact is `done`, `skipped`, or was deliberately skipped because its own `instruction` stated a condition that did not apply. Tell the user about a deliberate conditional skip, remember it, and do not reconsider it. Dependencies are enablers, not gates: if a requested artifact is still `blocked` only because you deliberately skipped a conditional prerequisite, run `openspec instructions "<artifact-id>" --change "<name>" --json` (append the confirmed `--store "<id>"` only for a registered standalone store) despite the blocked status, then create it using step 3 only when those recorded conditional skips are its sole missing dependencies. If a requested artifact is blocked by a prerequisite the user did not ask to capture and cannot be conditionally skipped, explain that dependency and ask before expanding the capture.

Capture the artifact(s) the user requested without asking them to invoke another workflow command. If they asked only to start a change, stop after scaffolding and show its status.

### When a change exists

If the user mentions a change or you detect one is relevant:

1. **Resolve and read existing artifacts for context**
   - Run `openspec status --change "<name>" --json`.
   - Use `changeRoot`, `artifactPaths`, and `actionContext` from the status JSON.
   - Read existing files from `artifactPaths.<artifact>.existingOutputPaths`.

2. **Reference them naturally in conversation**
   - "Your design mentions using Redis, but we just realized SQLite fits better..."
   - "The proposal scopes this to premium users, but we're now thinking everyone..."

3. **Offer to capture when decisions are made**

   `<capability-path>` is the spec directory relative to `specs/` (for example, `user-auth` or `identity/user-auth`). Preserve an existing capability's full path and follow the project's established organization for new capabilities.

    | Insight Type               | Where to Capture                    |
    |----------------------------|-------------------------------------|
    | New requirement discovered | `specs/<capability-path>/spec.md` |
    | Requirement changed        | `specs/<capability-path>/spec.md` |
    | Design decision made       | `design.md`                       |
    | Scope changed              | `proposal.md`                     |
    | New work identified        | `tasks.md`                        |
    | Assumption invalidated     | Relevant artifact                   |

   Example offers:
   - "That's a design decision. Capture it in design.md?"
   - "This is a new requirement. Add it to specs?"
   - "This changes scope. Update the proposal?"

4. **The user decides** - Offer and move on. Don't pressure. Don't auto-capture.

---

## What You Don't Have To Do

- Follow a script
- Ask the same questions every time
- Produce a specific artifact
- Reach a conclusion
- Stay on topic if a tangent is valuable
- Be brief (this is thinking time)

---

## Convergent Close

⚠️ **LOCAL CUSTOMIZATION** — this whole section has no vanilla equivalent. Vanilla explore is purely
divergent: no convergent exit, no output contract. See
[openspec/CONVENTIONS.md](../../../openspec/CONVENTIONS.md).

Divergence stays the default. This is an **exit**, not a phase you have to reach: plenty of explore
sessions end without it (see § Ending Discovery). But when the user wants to move toward a change, the
transition has a shape — an interview, a fixed block, and a file — so that the thinking survives the
conversation: `/opsx:propose` starts from what the user actually answered, and it reads `context.md`,
not this chat.

### When to converge

Two triggers, both requiring the user:

- **The user asks** — "cerremos esto", "armemos el change", "let's write this up", or any explicit
  request to move toward a change.
- **You offer and the user accepts** — when the space feels explored you MAY offer: "I think we've
  covered enough ground. Want me to close this — a few questions, an intent brief, and the change
  opened with it recorded?" Say what accepting means, since closing writes a file. Offering is yours;
  deciding is theirs. If they'd rather keep exploring, keep exploring.

Never slide into the close on your own initiative.

### Step 1 — Intent interview (unskippable)

The close **starts with questions**, not with a summary. Same mechanics as the *Understand* phase of
[`/debug`](../debug.md): ask **one at a time or in natural groups**, adapt the order to what's already
on the table, and follow up on whatever an answer leaves open.

The axes to pin down:

| Axis | What to establish |
|---|---|
| **Intent** | What problem is this solving, for whom, and why now? |
| **Scope** | What's in, and where does the boundary run? |
| **Observable behavior** | What changes that a user or a caller can actually see? |
| **Non-goals** | What are we explicitly *not* doing, even though it's adjacent? |
| **Acceptance criteria** | How do we know it's done? What has to be true? |

**After each answer, evaluate: can I state the intent brief without inventing anything material?** If
not, name the missing piece and ask for it specifically. A gap earns another question, never a
plausible guess. Typical gaps:

- "I know the problem but not who has it" → ask who the user is.
- "I know what they want but not where it stops" → ask what's out of scope.
- "I have the behavior but no way to check it" → ask for acceptance criteria.
- "A non-goal is implied but never stated" → ask them to confirm it.

**The interview is not optional and not replaceable by synthesis.** Even when the divergent
conversation appears to cover every axis, you MUST run at least one round of questions — confirming
the axes and closing the gaps — before stating the brief. The brief's value is that it records what
the user *answered*; one synthesized from your own reading of the conversation bakes in your
assumptions and looks identical from the outside. If the conversation really did cover everything, the
round is short (a single grouped question restating the axes and asking what's off) — but it happens.

### Step 2 — The intent brief

When the interview closes, emit this fixed block in the chat. It is the canonical input for
`/opsx:propose`, the same role the bug report plays for `/debug`:

```
## Intent brief

**Intención:** <what problem, for whom, why now>
**Alcance:** <the boundary — what's in>
**Comportamiento observable:** <what changes visibly for a user or a caller>
**Non-goals:** <explicitly out, though adjacent>
**Criterios de aceptación:** <what must be true to call it done>
**Incertidumbres abiertas:** <what stayed unresolved>
```

All six sections are always present. `Incertidumbres abiertas` is where honest unknowns go — leaving
it empty because it looks better defeats the purpose.

The brief goes to the chat **first**: the user reads exactly what is about to be recorded before step 3
writes it.

### Step 3 — Persist the interview in `context.md` (always)

The close does not end in the chat. Once the brief is out, **write it down**: open the change and save
the interview. This is part of closing, not an extra you ask permission for — the user asked to close,
and recording the close is what closing means. The only thing you confirm first is the **name**.

1. Derive a kebab-case slug from the intent (verb prefix: `add-`, `fix-`, `replace-`, `remove-`,
   `refactor-`, `update-` — see [openspec/CONVENTIONS.md](../../../openspec/CONVENTIONS.md) § Naming)
   and confirm it: "I'll open this as `<slug>` — sound right?". They can rename it; they are not being
   asked whether to save.
2. Run the "When no change exists" loop above (steps 1–4) with **exactly one requested artifact:
   `context`**. `openspec new change "<name>"` scaffolds the change — never by hand — and
   `openspec instructions context --change "<name>" --json` returns the template, instruction,
   `context` and `rules`.
3. Fill the template's **"Entrevista de intención"** section with the interview: the relevant Q&A
   cleaned up (question → answer, not a literal transcript — drop the noise, keep the meaning) plus the
   intent brief exactly as you emitted it.
4. Write **no other artifact** — not `proposal.md`, not `specs/`, nothing else. `context: done` with
   every other artifact pending is the intended end state.

### Step 4 — Offer the technical snapshot (recommended)

`context.md` has a second half the interview doesn't cover: **Nodos afectados**, **Archivos
relevantes**, **Contexto knowledge**, **Riesgos identificados** — the Graphify + `knowledge/` snapshot
the instruction describes. Filling it now is the **recommended** path, and say so: you just spent the
session investigating this code, so it's the cheapest moment to write it, and it leaves the change
ready for `/opsx:propose`.

> Want me to complete the technical snapshot too — graph nodes, files, knowledge, risks? Recommended:
> I already have the context loaded, and it leaves the change ready to propose.

- **They accept** → fill those four sections from Graphify and `knowledge/` as the instruction
  describes.
- **They decline** (or want to stop here) → write each of the four sections with an explicit pending
  marker. Never leave them blank:

  ```
  _Pendiente — correr /opsx:explore <name> para consolidar el snapshot técnico._
  ```

  A blank section reads as "no risks / no nodes"; the marker says "not consolidated yet" and carries
  its own way out — § Iterating an Existing `context.md` is what consolidates it later. `openspec status`
  only checks that the file exists, so a half-written `context.md` still reports `context: done`; the
  marker is what keeps that honest.

### Step 5 — Hand off in words, not in actions

> Change `<name>` created with `context.md` — interview recorded, technical snapshot
> <complete | pending>. Run `/opsx:propose <name>` when you want the rest of the artifacts.

`/opsx:propose` is invoked by the user. You don't chain into it.

---

## Iterating an Existing `context.md`

⚠️ **LOCAL CUSTOMIZATION** — no vanilla equivalent, and no separate command either: in this harness
`/opsx:explore` is the **owner** of `context.md`. It creates the file in the convergent close, and it
is what you use to rework it afterwards — `/opsx:propose` is only a bounded collaborator (it appends
the Q&A it conducted itself to the interview section; it never touches the derived half). (There is no
`/opsx:refresh-context`; it was folded in here — see
[openspec/CONVENTIONS.md](../../../openspec/CONVENTIONS.md).)

Run `/opsx:explore <change-name>` on a change that already has a `context.md` when:

- the technical snapshot is still pending (the close recorded the interview and the user skipped the rest);
- the graph or `knowledge/` moved and the snapshot went stale;
- the intent itself moved — new information changes scope, non-goals or acceptance criteria.

### Resolve paths from the CLI, don't assume them

```bash
openspec status --change "<name>" --json
openspec instructions context --change "<name>" --json
```

From `status`: `planningHome`, `changeRoot`, `artifactPaths.context.existingOutputPaths` — never hardcode
`openspec/changes/<name>/`, it breaks under a selected store. From `instructions`: `template`,
`instruction`, `context` / `rules` (constraints for you, **never** content for the file) and
`resolvedOutputPath`. If `schemaName` has no `context` artifact, say so and stop. If there is no
`context.md` yet, this isn't iteration — go to § Convergent Close.

Scope seed for the derived half: `proposal.md` if the change has one, otherwise the intent brief already
in `context.md`. With neither, don't invent scope — ask.

### The interview half grows by append

You are the author of that section, so you may extend it. You may never rewrite it:

- **New Q&A go at the end.** The rows already there are the record of what the user answered: not
  reworded, not reordered, not dropped.
- **The intent brief is updated, not re-synthesized.** Every line still traces to an answer. If you think
  a line should change and you don't have an answer for it, ask — that's another interview round, not a
  rewrite.
- A `context.md` that predates schema v4 has no interview section: nothing to extend, and you don't
  invent one.

### The derived half is regenerated

Nodos afectados · Archivos relevantes · Contexto knowledge · Riesgos identificados — re-query Graphify
(`query_graph`, `get_neighbors`, `shortest_path`), re-read the knowledge in scope, and rewrite those four
sections. A section carrying `_Pendiente — correr /opsx:explore <name> ..._` is exactly what this is for:
replace the marker with real content. Never carry a marker over into the new file.

### Diff, then confirm

Show a unified diff of the previous `context.md` against the new one and ask: "Write it? (yes / no / show
the full file)". Write only on a yes. The diff is also the audit of the append rule — any edit to a
previously recorded answer shows up in it before anything is saved.

After writing, say what moved and what didn't:

> `context.md` updated — interview: +<N> Q&A (or: unchanged) · snapshot: consolidated. The other
> artifacts were not touched. If the scope moved, review `proposal.md`, `specs/`, `design.md`,
> `ui-ux.md` and `tasks.md`.

**Never** write those five from here. If the scope genuinely moved, that's `/opsx:propose` or a manual
edit — not this.

---

## Ending Discovery

There's no required ending. Discovery might:

- **Converge into a change**: the user wants to move toward one — intent interview → intent brief →
  `context.md` with the interview recorded — see § Convergent Close
- **Iterate an existing change's context**: rework `context.md` — extend the interview, refresh the
  snapshot — see § Iterating an Existing `context.md`
- **Flow into a proposal**: "Ready to start? I can create a change proposal."
- **Result in artifact updates**: "Updated design.md with these decisions"
- **Just provide clarity**: User has what they need, moves on
- **Continue later**: "We can pick this up anytime"

When things crystallize, you might offer a summary - but it's optional. Sometimes the thinking IS the value.

---

## Guardrails

- **Don't implement** - Never write code or implement features. Creating OpenSpec artifacts is fine, writing application code is not.
- **Don't fake understanding** - If something is unclear, dig deeper. Ask the user, read the code, trace the flow — whatever it takes to actually understand the feature before reasoning about it.
- **Don't rush** - Discovery is thinking time, not task time
- **Don't force structure** - Let patterns emerge naturally
- **Don't auto-capture** - Offer to save insights, don't just do it. In the convergent close this
  means: don't converge unasked, and never write an artifact beyond `context.md`. It does **not** mean
  asking permission to record an interview the user asked you to run — see the next bullet.
- ⚠️ **LOCAL CUSTOMIZATION** — **Don't skip the interview** - The intent brief is never synthesized
  from the conversation alone. At minimum one round of confirmation questions comes first (§ Convergent
  Close, step 1).
- ⚠️ **LOCAL CUSTOMIZATION** — **Don't leave the interview unsaved** - Once the brief is out, the close
  always writes it into `context.md` (§ Convergent Close, step 3). A brief that lives only in the chat
  dies with the session; the next command reads files, not conversations. Confirm the name, not whether
  to save.
- ⚠️ **LOCAL CUSTOMIZATION** — **Don't rewrite the interview** - When iterating an existing `context.md`,
  new Q&A are appended and the brief is updated from real answers. Previously recorded answers are never
  reworded, reordered or dropped, and nothing is written without a diff and a yes (§ Iterating an
  Existing `context.md`).
- ⚠️ **LOCAL CUSTOMIZATION** — **`context.md` is yours; the other five aren't** - Explore is the only
  writer of `context.md` (there is no `/opsx:refresh-context`). It is not a writer of `proposal.md`,
  `specs/`, `design.md`, `ui-ux.md` or `tasks.md` outside the vanilla capture offers of § When a change
  exists.
- **Don't manually scaffold changes** - Never create a new change directory under `openspec/changes/` by hand. Always use `openspec new change "<name>"` (with `--store <id>` when applicable) so required metadata such as `.openspec.yaml` is created before writing artifacts.
- **Do visualize** - A good diagram is worth many paragraphs
- **Do explore the codebase** - Ground discussions in reality
- **Do question assumptions** - Including the user's and your own
