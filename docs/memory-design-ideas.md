# Memory System MVP (Dictation-Derived Ongoing Context)

This document defines a simple MVP for local user memory derived only from dictation history.

## MVP Principles

- Keep memory in a single local markdown file.
- Store only information we can infer from dictated content.
- Focus on ongoing context and likely continuation.
- Allow long-term interest and role signals only when evidence is repeated over time.
- Keep injection small and deterministic.
- Memory is disabled by default, and the same warning as the active app context toggle should appear when enabling it.

Formatting instructions belong in the model prompt/template, not in persistent memory.

---

## 1) Memory File Format (Locked)

Use strict sectioned markdown.

```md
# User Memory

## Long-Term Signals
- Likely professional domain: software development (confidence: high, evidence: repeated Python/code dictation across sessions).
- Likely recurring interests: tennis, music (confidence: medium, evidence: repeated mentions over recent weeks).
- Preferred writing domain: technical notes and implementation planning.

## Ongoing Context
- Active workstream: cardiology consult notes.
- Current artifact: discharge summary draft.
- Recent topics: NSTEMI follow-up, cath findings, medication reconciliation.

## Active Threads
- Pending completion: assessment and plan section for patient with chest pain.
- Pending completion: follow-up interval and return precautions.

## Recent Entities
- Clinical terms: NSTEMI, TTE, PCI.
- Medications: aspirin, metoprolol, atorvastatin.
- People/teams: cardiology, ED team.

## Observed Recurring Phrases
- "no acute distress"
- "risk and benefits discussed"
- "follow up with cardiology in one week"

## Do Not Store
- Passwords, API keys, one-time codes, personal identifiers, protected health information.

## Metadata
- Last updated: 2026-02-14T12:34:56Z
```

No YAML frontmatter for MVP.

---

## 2) When Memory Is Created

Create memory in one of two ways:

1. User clicks **Enable Memory**.
2. Automatic fallback: after **3 completed sessions**.

If neither condition is met, no memory file is created.

---

## 3) Background Flow (Minimal)

1. Client ends a session.
2. Client sends a compact summary of:
   - current dictation topic
   - unfinished or repeated threads
   - recurring entities and phrases from recent sessions
   - candidate long-term signals with evidence counts (for example, repeated topic clusters like Python, tennis, music)
3. Server LLM returns a full replacement markdown body for known sections.
4. Client writes the updated file locally.

MVP intentionally avoids patch/rebase logic.

---

## 4) Injection Back Into Runtime Calls

Keep injection deterministic and small:

- Always include:
  - top relevant `Ongoing Context`
  - top relevant `Active Threads`
- Optionally include up to **3** bullets from `Recent Entities` or `Observed Recurring Phrases` that match the current transcript.
- Optionally include up to **2** `Long-Term Signals` only when confidence is medium or high.

If user gives explicit instructions in the current turn, current-turn instruction wins.

---

## 5) Safety Guardrails (MVP)

- Never store anything listed under `Do Not Store`.
- Keep one local backup copy before each write: `user-memory.backup.md`.
- Only store items with repeated evidence across sessions.
- Never infer traits that are not directly grounded in dictated text.
- Long-term signal requirements:
  - At least 3 separate sessions with supporting evidence.
  - Include an explicit confidence label (`low`, `medium`, `high`).
  - Drop or downgrade signals when they have no evidence over time.

---

## 6) What We Deliberately Skip in MVP

To reduce mistakes, we skip these for now:

- patch-based merges
- embeddings-based retrieval pipelines
- confidence scoring systems
- multi-file memory packs
- user diff review UI
- per-token low-level ASR correction logic

These can be added only after real-world feedback.

---

This is intentionally simple and should be the first implementation.
