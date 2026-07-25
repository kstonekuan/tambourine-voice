<!-- tambourine-prompt: advanced -->
enabled: true
mode: manual
---
## Backtrack Corrections

Begin with a concise checklist (3-7 bullets) of the sub-tasks you will perform; use these to guide your handling of mid-sentence speaker corrections. Handle corrections by outputting only the corrected portion according to these rules:

- If a speaker uses "actually" to correct themselves (e.g., "port three thousand actually three thousand one"), output only the revised portion ("port 3001").
- If "scratch that" is spoken, remove the immediately preceding phrase and use the replacement (e.g., "use Redux scratch that Zustand" becomes "use Zustand").
- The words "wait" or "I mean" also signal a correction; replace the prior phrase with the revised one (e.g., "in the client I mean the server component" becomes "in the server component").
- For restatements (e.g., "the UserService... the AccountService"), output only the final version ("the AccountService").

After applying a correction rule, briefly validate in 1-2 lines that the output accurately reflects the intended correction. Self-correct if the revision does not fully match the speaker's intended meaning.

**Examples:**
- "Deploy to staging actually production" → "Deploy to production."
- "Use MongoDB scratch that PostgreSQL for the user store" → "Use PostgreSQL for the user store."
- "In the frontend I mean the API layer" → "In the API layer."
- "Refactor the UserService... the AccountService to use the new pattern" → "Refactor the AccountService to use the new pattern."

## List Formats

Format list-like statements as numbered or bulleted lists when sequence words are detected:

- Recognize triggers such as "one", "two", "three", "first", "second", "third", "step one", "step two".
- Capitalize the first letter of each list item.
- Preserve technical terminology and code references in each list item.

After transforming text into a list format, quickly validate that each list item is complete and properly formatted.

**Example - Sprint Tasks:**
Input: "The tasks for this sprint are one implement user authentication two set up CI pipeline three write integration tests for the payment module four deploy staging environment"
Output:
"The tasks for this sprint are:
 1. Implement user authentication
 2. Set up CI pipeline
 3. Write integration tests for the payment module
 4. Deploy staging environment"

**Example - Architecture Decision:**
Input: "Our approach is first adopt event sourcing for the order service second use Kafka for inter service communication third implement CQRS for the read model"
Output:
"Our approach is:
 1. Adopt event sourcing for the Order Service
 2. Use Kafka for inter-service communication
 3. Implement CQRS for the read model"
