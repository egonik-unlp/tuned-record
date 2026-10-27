---
name: "Debug"
description: "Socratic debugging before exploring or proposing a fix. Build a mental model of the problem before touching code."
category: Workflow
tags: [debugging, workflow]
---

Structured debugging mode. The goal is to reach a precise description of the bug **before** reading code or proposing a fix.

There are two phases: **Understand** (questions) → **Verify** (evidence). Don't skip the first to jump straight to the second.

**Guiding principle:** ask follow-up questions as many times as needed. If an answer leaves a gap, open that gap with another question. Don't move to the next phase with missing pieces — it's better to bother the user with one more question than to start digging through code with an incomplete mental model.

---

## Phase 1 — Understand the problem (without touching code yet)

Ask the user questions **one at a time or in natural groups**. Don't dump everything at once. Adapt the order to what they've already said and the project domain.

### Base questions (always)

1. **What behavior did you expect?** — describe it in domain terms (what value, what state, what record, what response).
2. **What behavior are you seeing?** — the concrete value/state/error that shows up.
3. **Do you have the stack trace, log, or exact error message?** — if it exists, ask for it verbatim. If it's an unexpected value (nil, 0, wrong state), ask for the literal value.

### Angles to explore (pick what applies)

**About the entity or data involved:**
- Which concrete domain entity is failing? What state is it in before and after?
- What relevant attributes does that entity have in the failing case?
- Are there relationships with other entities that matter to understand the symptom?

**About the reproduction scenario:**
- What action triggers the problem? (what the user does, what endpoint is called, what job runs)
- Does it happen always, or only with a certain combination of data/configuration/role?
- What's the minimal combination that reproduces it?
- In what environment does it appear? (dev, staging, prod, test)

**About timing and scope:**
- At what point in the flow does the symptom appear? Is it where it breaks or where it manifests?
- Is it an isolated case or a pattern? How many records/users affected?
- When did it start happening? Any recent deploy, migration, or change that looks suspicious?

**About what was already tried:**
- What hypotheses have you already ruled out, and how?
- Was there anything that seemed related but turned out not to be?

### Keep asking until it closes

After each answer, evaluate: **can I describe the bug precisely enough to predict where it lives in the code?** If the answer is no, identify what piece is missing and ask specifically for it. Examples of typical gaps:

- "I know which entity fails but not what state it's in" → ask about the state.
- "I know the symptom but not what action triggers it" → ask about the trigger.
- "I know it breaks but not whether it's always or sometimes" → ask about the condition.
- "I have a stack trace but I don't know what input caused it" → ask about the input.

Don't be afraid to ask for clarification on ambiguous user terms. "It's broken" or "it's not working" aren't descriptions — they're starting points for follow-up questions.

### Build the hypothesis

Once the answers are enough, **state the hypothesis explicitly** before going to the code:

> "My hypothesis is that the problem happens in [specific place] because [reason], when [specific condition]."

Ask the user if the hypothesis makes sense from the business/domain side before continuing. If they say no, go back to Phase 1.

---

## Phase 2 — Investigate with a hypothesis in hand

Only now read the code, **guided by the hypothesis**. Not open exploration.

- Look for the exact spot the hypothesis predicts.
- If the code confirms the hypothesis: document the root cause with file and line.
- If the code contradicts it: revise the hypothesis, don't force the explanation. If needed, go back to Phase 1 with new questions.

When you find the root cause, describe it like this:

```
Root cause: <file>:<line> — <what exactly happens>
Triggering condition: <data or state combination>
Downstream: <where it breaks visibly for the user>
```

---

## Phase 3 — Verify before concluding

Before declaring that you understood the bug or proposing a fix, demand concrete evidence. **"It should work like this" is not enough.**

Verification checklist:

- [ ] Can you reproduce the bug with concrete data (in console, a test, or a description of exact steps)?
- [ ] Does the root cause you identified explain 100% of the reported symptom?
- [ ] Are there other places in the code that produce the same symptom through a different route?
- [ ] Does the proposed fix not break adjacent flows (rollback, retries, domain edge cases)?
- [ ] Are there edge cases the fix doesn't cover?

If any item is left unanswered, flag it explicitly as an **open uncertainty** before moving on to propose a fix.

---

## Final output

When finishing the debug, produce a summary in this format (it serves as input for `/opsx:propose` or for the PR/issue description):

```
## Bug report

**Symptom:** <what the user sees>
**Root cause:** <file:line — explanation>
**Reproduction condition:** <exact combination of data/state>
**Downstream:** <where it breaks exactly>
**Flows that must NOT break:** <rollback, retries, integrations, etc.>
**Open uncertainties:** <what couldn't be verified>
```

This summary replaces free-form description and guarantees that the next step (proposal, fix, test) starts from a correct mental model.

---

## Skill closure — don't move to the fix without confirmation

`/debug` **ends with the bug report**. That block is the deliverable output, meant as input for `/opsx:propose` or the PR/issue description. **Don't move on to editing files in the same session**, even if the root cause is clear and the fix looks obvious or a few lines long.

User phrases like *"we need to fix X"*, *"all that's left is changing Y"*, *"the fix is trivial"* describe the **direction** of the fix — they are not authorization to apply it. Before touching code, ask explicitly:

> "Do you want me to apply the fix directly here, or do you pass the bug report to `/opsx:propose` so it enters the SDD flow?"

Why it matters: this repo uses the snappec SDD flow (`/opsx:propose` → `/opsx:apply` → `/opsx:archive` → `/documenter distill`). Skipping that flow leaves the change without a registered change in `openspec/`, without artifacts and without traceability. For low-friction fixes it may make sense to bypass it, but that decision is the user's, not the agent's.
