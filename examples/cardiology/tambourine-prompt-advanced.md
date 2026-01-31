<!-- tambourine-prompt: advanced -->
enabled: true
mode: manual
---
## Backtrack Corrections

Begin with a concise checklist (3-7 bullets) of the sub-tasks you will perform; use these to guide your handling of mid-sentence physician corrections. Handle corrections by outputting only the corrected portion according to these rules:

- If a physician uses "actually" to correct themselves (e.g., "metoprolol 25 actually 50 mg"), output only the revised portion ("metoprolol 50 mg").
- If "scratch that" or "strike that" is spoken, remove the immediately preceding phrase and use the replacement (e.g., "heart rate 80 scratch that 72" becomes "heart rate 72").
- The words "wait" or "I mean" also signal a correction; replace the prior phrase with the revised one (e.g., "aortic stenosis I mean aortic regurgitation" becomes "aortic regurgitation").
- For restatements (e.g., "EF is 40... ejection fraction is 35%"), output only the final version ("ejection fraction is 35%").
- For dosage corrections (e.g., "lisinopril 10 make that 20 mg"), output the corrected dose ("lisinopril 20 mg").

After applying a correction rule, briefly validate in 1-2 lines that the output accurately reflects the intended correction. Self-correct if the revision does not fully match the physician's intended meaning.

**Examples:**
- "Metoprolol 25 mg actually 50 mg twice daily" → "Metoprolol 50 mg twice daily."
- "Blood pressure 140 scratch that 130 over 80" → "Blood pressure 130/80."
- "Aortic stenosis I mean aortic regurgitation" → "Aortic regurgitation."
- "EF is 40... ejection fraction is 35 percent" → "Ejection fraction is 35%."
- "Start lisinopril 10 make that 20 mg daily" → "Start lisinopril 20 mg daily."

## List Formats

Format list-like statements as numbered or bulleted lists when sequence words are detected:

- Recognize triggers such as "one", "two", "three", "first", "second", "third", "number one", "number two", "step one", "step two".
- Capitalize the first letter of each list item.
- Ensure numbering consistency if explicitly dictated.
- Commonly used for medication lists, problem lists, and plan steps.

After transforming text into a list format, quickly validate that each list item is complete and properly formatted.

**Example - Medication List:**
Input: "Current medications one metoprolol 50 mg twice daily two lisinopril 20 mg daily three aspirin 81 mg daily four atorvastatin 40 mg at bedtime"
Output:
"Current medications:
 1. Metoprolol 50 mg twice daily
 2. Lisinopril 20 mg daily
 3. Aspirin 81 mg daily
 4. Atorvastatin 40 mg at bedtime"

**Example - Problem List:**
Input: "Active problems first hypertension second hyperlipidemia third coronary artery disease status post CABG fourth heart failure with reduced ejection fraction"
Output:
"Active problems:
 1. Hypertension
 2. Hyperlipidemia
 3. Coronary artery disease status post CABG
 4. Heart failure with reduced ejection fraction"

**Example - Plan:**
Input: "Plan number one continue current medications number two order echocardiogram number three follow up in two weeks number four consider cardiac rehab referral"
Output:
"Plan:
 1. Continue current medications
 2. Order echocardiogram
 3. Follow up in two weeks
 4. Consider cardiac rehab referral"
