---
name: "OPSX: Propose"
description: "Propose a new change - create it and generate all artifacts in one step"
allowed-tools: Bash(openspec:*)
category: "Workflow"
tags: ["workflow", "artifacts", "experimental"]
---

Propose a new change - create the change and generate all artifacts in one step.

**Planning boundary**: This workflow creates planning artifacts only. The user request that selected or triggered this workflow authorizes planning only, even if it asks to build or fix something. Do not edit project code. After the planning artifacts are complete, stop. Do not start implementation in the same response, even if the initial request asks for it. Wait for a new user request after the artifacts are presented; then start the apply workflow.

> **Local customization** — this repo uses the **`sdd-with-knowledge`** schema (NOT vanilla
> spec-driven). See [openspec/README.md](../../../openspec/README.md) and
> [openspec/CONVENTIONS.md](../../../openspec/CONVENTIONS.md).

I'll create a change with the artifacts your schema defines. With `sdd-with-knowledge` that is **6
artifacts in dependency order** — each one built on the previous:

1. `context.md` — consolidates the Graphify subgraph + relevant `knowledge/` before anything else
2. `proposal.md` — why / scope / impact / non-goals (references graph nodes, not grep)
3. `specs/<capability-path>/spec.md` — delta spec: `ADDED` / `MODIFIED` / `REMOVED Requirements`
   with Given/When/Then (a delta, not the main spec)
4. `design.md` — technical approach + decisions (distilled to `knowledge/` post-archive)
5. `ui-ux.md` — first line is a mandatory verdict: `Aplica` or `No aplica — backend-only`
6. `tasks.md` — implementation checklist; the last task is always `/documenter distill <change-id>`

`<capability-path>` is the spec directory relative to `specs/`. ⚠️ LOCAL: in this repo it is always a
**flat single-segment slug** (for example `user-auth` — never `identity/user-auth`): the slug maps 1:1
to the nested use-case `knowledge/modules/<module>/use-cases/<capability>/` and is resolved by glob, so
a nested capability path breaks that lookup. Preserve an existing capability's full path.

The exact set, order and dependencies come from `openspec status --json` (schema-driven) — the steps
below follow it dynamically, so this list stays in sync with the schema automatically.

⚠️ LOCAL: after `proposal.md` is written, the loop pauses **once** — the **proposal gate** (step 5) —
and resumes only on the user's explicit approval, recorded in the `**Estado:**` line of `proposal.md`.

When the user is ready to implement, they must start the apply workflow explicitly.

---

**Store selection:** If the user names a store (a store is a standalone OpenSpec repo registered on this machine) or the work lives in one, run `openspec store list --json` to discover registered store ids, then pass `--store <id>` on the commands that read or write specs and changes (`new change`, `status`, `instructions`, `list`, `show`, `validate`, `archive`, `doctor`, `context`, `schemas`, `view`). Once selected, treat `--store <id>` as sticky for the rest of the workflow. Every unscoped example of those commands below is shorthand: before running it, append the flag. For example, run `openspec status --change "<name>" --json --store "<id>"`, not the unscoped form shown below. Other commands do not take the flag. Hints printed by commands already carry the flag; keep it on follow-ups. Without a store, commands act on the nearest local `openspec/` root.

**Input**: The argument after `/opsx:propose` is the change name (kebab-case), OR a description of what the user wants to build.

**Steps**

1. **Completeness check** — ⚠️ LOCAL CUSTOMIZATION

   If no input is provided, ask the user (open-ended, no preset options):
   > "What change do you want to work on? Describe what you want to build or fix."

   From their description, derive a kebab-case name (e.g., "add user authentication" → `add-user-auth`).

   **IMPORTANT**: Do NOT proceed without understanding what the user wants to build.

   This command does NOT run an interview — that is `/opsx:explore`'s job. It **checks the input it
   received** — an intent brief from `/opsx:explore`, a bug report from `/debug`, or a direct request —
   against the contract checklist (the intent brief fields — canonical definitions in `explore.md`'s
   convergent close):

   - **Intención** — what problem this solves, for whom, and why now
   - **Alcance** — what is in and what is out
   - **Comportamiento observable** — what changes for the user/system, stated as observable behavior
   - **Non-goals** — what this change deliberately does not do
   - **Criterios de aceptación** — observable domain outcomes that mark the change as done

   Route on the result:

   - **Complete input** (an intent brief, a bug report, or a direct request that already covers the
     checklist) → ask **zero questions** and go straight to building.
   - **Specific gaps** → ask **only** those gaps, one question per gap (gap→question style, like
     `/debug`). Record what you asked and what was answered (see the write rules below).
   - **Big gap** (you cannot state the intent without inventing it) → suggest running `/opsx:explore`
     first — **naming the change if it already exists** (`/opsx:explore <name>`), so explore resumes
     from the written `context.md`, appending to its interview instead of starting from scratch. If the
     user prefers to stay here, cover the gaps by asking.
   - **Existing change with `context.md`** → **respect it**: do not regenerate it, read its
     "Entrevista de intención" section as the source for this check, and start building at `proposal`.
     On a re-run, re-read `context.md` and `proposal.md` from disk and ask only about deltas.

   **Write rules for `context.md`** — one owner (`/opsx:explore`), one bounded collaborator (propose,
   only its own conversational half):

   | Propose MAY | Propose MUST NOT |
   |---|---|
   | Create `context.md` if it does not exist (the direct path needs somewhere to record this check) | Rewrite, reorder, or delete Q&A that `/opsx:explore` conducted |
   | Append to "Entrevista de intención" the Q&A that **it** conducted (this check + proposal-gate adjustments) | Re-synthesize the intent brief from the conversation |
   | Update the intent brief with answers **it** actually gathered | Regenerate the derived half (Nodos afectados / Archivos relevantes / Contexto knowledge / Riesgos identificados) — leave the marker `_Pendiente — correr /opsx:explore <change-id> para consolidar el snapshot técnico._` and say so in the output |

2. **Determine the workflow schema**

   Use the configured default schema unless the user explicitly requests a different workflow.

   **Use a different schema only if the user:**
   - Explicitly requests a specific schema by name → use `--schema <schema-name>`
   - Asks to "show workflows" or asks "what workflows" exist → resolve the authoritative root by running `openspec context --json` from the current working directory. If the user explicitly selected a registered store, use `openspec context --json --store "<store-id>"`. Then run `openspec schemas --json` with its working directory set to the returned `root.path` and let them choose. This preserves roots selected by a local `store:` pointer or the global `defaultStore`; when a registered store was explicitly selected, append `--store "<store-id>"` to `openspec schemas --json` as well. If context reports only `no_openspec_root`, run `openspec schemas --json` from the current working directory instead. Do not use this fallback for invalid or unavailable stores.

   Otherwise, omit `--schema` to preserve the configured default.

3. **Create the change directory**

   Choose one schema form below. If a registered store is selected, append `--store "<store-id>"` to that command and each later OpenSpec command shown below that accepts `--store`.

   Using the configured default:
   ```bash
   openspec new change "<name>"
   ```

   Using an explicitly requested schema:
   ```bash
   openspec new change "<name>" --schema "<schema-name>"
   ```
   This creates a scaffolded change in the planning home resolved by the CLI with `.openspec.yaml`.

4. **Get the artifact build order**
   ```bash
   openspec status --change "<name>" --json
   ```
   Parse the JSON to get:
   - `applyRequires`: array of artifact IDs needed before implementation (e.g., `["tasks"]`)
   - `artifacts`: list of all artifacts, each with its `status` and its `requires` edges (the artifact IDs it directly depends on)
   - `planningHome`, `changeRoot`, `artifactPaths`, and `actionContext`: path and scope context. Use these instead of assuming repo-local paths.

5. **Create every artifact in the required set**

   Use a todo list to track progress through the artifacts.

   Loop through artifacts in dependency order (artifacts with no pending dependencies first):

   ⚠️ LOCAL CUSTOMIZATION — **Proposal gate**. This loop has exactly **one deliberate pause**, and this
   is it. The trigger is **state on disk, not an event**:

   > The artifact whose id is `proposal` — take the id and its output path from `status --json`, never
   > hardcode order or names — **exists**, and its `**Estado:**` line, re-read from the file on disk,
   > does **not** say `Aprobado`.

   While that condition holds, do NOT run `openspec instructions` for any artifact that depends on
   `proposal` directly or transitively (walk the `requires` edges). Artifacts that do not depend on it
   are unaffected. Because the trigger is state on disk, a re-run over a change whose proposal is still
   pending re-gates it once, and a re-run over an approved proposal does not re-ask.

   When the gate triggers:

   1. **Re-read `proposal.md` from disk** (the user may have edited it) and build a summary from it:
      Why · Scope · Non-goals · Conflictos/Riesgos · Criterios de aceptación.
   2. Present the summary in markdown, then ask for the decision with **AskUserQuestion** — options
      `Approve` / `Adjust` / `Cancel`, **none marked as recommended**. If the tool is not available,
      fall back to prose: present the same three options, route on the answer, and ask again rather
      than proceeding on anything unclear (same pattern as the archive sync prompt).
   3. Route on the answer:
      - **Approve** → write `**Estado:** Aprobado — <fecha>` in `proposal.md` and resume the loop
        straight through to the existing hard stop (the planning boundary).
      - **Adjust** (or free-form feedback — the tool's "Other" answer routes here) → incorporate the
        feedback, regenerate `proposal.md` — every (re)generation writes
        `**Estado:** Pendiente de aprobación` — and re-present the gate. If what is fuzzy is the
        **intent itself**, suggest going back to `/opsx:explore <name>` instead of iterating here.
      - **Cancel** → stop. The change stays on disk with `context` + `proposal` written and the mark
        pending; nothing else is generated.
      - **Ambiguous answer** → ask again. Describing adjustments is NOT approval.

   If the schema does not define an artifact with id `proposal`, skip the gate, tell the user, and
   continue without pausing.

   **Gate guardrails:**
   - `Aprobado — <fecha>` is written ONLY in the same turn as an explicit Approve from the user —
     never by inference, never because the summary "looked fine", never carried over from a re-run.
   - Every (re)generation of `proposal.md` — the first write included — sets
     `**Estado:** Pendiente de aprobación`. What gets approved is a text, not a file.
   - Once approved, do not pause again for the remaining artifacts.

   a. **For each artifact that is `ready` (dependencies satisfied)**:
      - Get instructions:
        ```bash
        openspec instructions <artifact-id> --change "<name>" --json
        ```
      - The instructions JSON includes:
        - `context`: Project background (constraints for you - do NOT include in output)
        - `rules`: Artifact-specific rules (constraints for you - do NOT include in output)
        - `template`: The structure to use for your output file
        - `instruction`: Schema-specific guidance for this artifact type
        - `skipped`/`warning`: present when the change declares skip_specs and this artifact must NOT be created - stop and pick another artifact
        - `resolvedOutputPath`: Resolved path or pattern to write the artifact
        - `dependencies`: Completed artifacts to read for context
      - Read any completed dependency files for context - always re-read them from disk, even if you saw them earlier in the conversation (the user may have edited them)
      - If the `instruction` field delegates creation to a specific skill or command, invoke it to produce the artifact instead of writing the file yourself, then verify the artifact file exists at `resolvedOutputPath`
      - Otherwise create the artifact file using `template` as the structure and write it to `resolvedOutputPath`. If `resolvedOutputPath` is a glob, follow `instruction` to choose the concrete file path
      - Apply `context` and `rules` as constraints - but do NOT copy them into the file
      - Show brief progress: "Created <artifact-id>"

   b. **Continue until every artifact in the required set exists (not just `apply.requires`)**
      - After creating each artifact, re-run `openspec status --change "<name>" --json`
      - The required set is `applyRequires` plus every artifact reachable from those by following the `requires` edges in `status --json` - walk them transitively (spec-driven closes over proposal, specs, design, tasks). Leave artifacts outside that set alone
      - `status` is file-existence only, so an `applyRequires` artifact reading `done` does NOT mean its dependencies exist - writing `tasks.md` early marks `tasks` done while `specs` was never written. Use each artifact's `requires` edges, not its `status`, to build the required set: a `done` artifact still lists what it depends on
      - An artifact already reading `status: "skipped"` is satisfied: the change declares `skip_specs` in `.openspec.yaml`, so its files must NOT exist. Never try to create one
      - Create every artifact in the required set that is missing, then re-check - creating one can unblock others
      - Skip one only when `status` already reports it `skipped`, or when its own `instruction` says it is conditional: run `openspec instructions <artifact-id> --change "<name>" --json` and skip only if its `instruction` field marks it optional (e.g. "create only if..."). Spec-driven's `design.md` qualifies; `specs` qualifies only via the `skipped` status above, never by your own judgment. Tell the user, and do not reconsider it
      - Dependencies are enablers, not gates: if a required artifact is still `blocked` only because you skipped a conditional dependency, write it anyway
      - Stop when every artifact in the required set is `done`, `skipped`, or was deliberately skipped

   c. **If an artifact requires user input** (unclear context):
      - Ask the user to clarify
      - Then continue with creation

6. **Show final status**
   ```bash
   openspec status --change "<name>"
   ```

**Output**

After completing all artifacts, summarize:
- Change name and location
- List of artifacts created with brief descriptions, plus any conditional artifact you skipped and why
- ⚠️ LOCAL: the gate result — `Proposal gate: approved` (add `tras ajustes` if this session went
  through Adjust rounds) — or the notice that the schema
  defines no `proposal` artifact and the gate was skipped. If the direct path left the derived half of
  `context.md` pending, say that the technical snapshot is consolidated with `/opsx:explore <change-id>`
- What's ready: "All artifacts needed for implementation are ready."
- Prompt: "The artifacts are ready for review. When you are ready, run `/opsx:apply`."

**Artifact Creation Guidelines**

- Follow the `instruction` field from `openspec instructions` for each artifact type - it is the authoritative guidance, even for familiar artifact names
- If the `instruction` field directs you to use a specific skill or command to create the artifact, invoke it instead of writing the artifact directly
- The schema defines what each artifact should contain - follow it
- Read dependency artifacts for context before creating new ones
- Use `template` as the structure for your output file - fill in its sections
- **IMPORTANT**: `context` and `rules` are constraints for YOU, not content for the file
  - Do NOT copy `<context>`, `<rules>`, `<project_context>` blocks into the artifact
  - These guide what you write, but should never appear in the output

**Guardrails**
- The request that invoked this workflow authorizes planning only. Any implementation or apply instruction in that request does not carry forward. Do NOT implement the change, start the apply workflow, or edit project code during this workflow. After presenting the artifacts, stop and wait for a new user request to start the apply workflow
- Create every artifact the apply phase transitively depends on, not just the ids listed in `apply.requires`
- Always read dependency artifacts before creating a new one - re-read from disk, not from conversation memory (files may have changed since you last saw them)
- Gaps in intent, scope, observable behavior, non-goals, or acceptance criteria are handled by the step-1 completeness check — route them through its checklist, don't fill them by assumption; only for minor details outside that checklist, make a reasonable assumption and record it
- If a change with that name already exists, ask if user wants to continue it or create a new one
- Verify each artifact file exists after writing before proceeding to next
- ⚠️ LOCAL: the approval mark (`**Estado:**`) is governed by the **Gate guardrails** in step 5 —
  never write it outside those rules
- ⚠️ LOCAL: writes to `context.md` are governed by the write-rules table in step 1 (owner:
  `/opsx:explore`; propose appends only its own conversational half)
