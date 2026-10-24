# Live judge on the conflict samples (qwen3:8b)

| sample | ideal | model said | action after policy |
|---|---|---|---|
| 01 constraint vs a contradicting constraint, judged contradicts | Conflict | Conflict | Conflict |
| 02 same pair, model wrongly says duplicate | Conflict | Conflict | Conflict |
| 03 decision vs a decision that replaces it, judged supersedes | Supersede | Supersede | Pending |
| 04 decision vs a fact that restates it differently, wrongly judged duplicate | Conflict | Conflict | Conflict |
| 05 fact with another day, wrongly judged duplicate | Supersede | Create | Pending |
| 06 fact with another version, judged supersedes | Supersede | Supersede | Pending |
| 07 constraint vs a gotcha that licenses what it forbids, judged contradicts | Conflict | Conflict | Conflict |
| 08 procedure vs its opposite, judged contradicts | Conflict | Conflict | Conflict |
| 09 Chinese constraint vs its opposite, judged contradicts | Conflict | Conflict | Conflict |
| 10 constraint vs an unrelated-looking candidate, wrongly judged supersedes | Conflict | Conflict | Conflict |
| 11 gotcha with the opposite claim, wrongly judged duplicate (known gap) | Conflict | Conflict | Conflict |
| 12 decision vs a model verdict of unrelated | Conflict | Conflict | Conflict |

The model chose the ideal relation in 11 of 12 samples; the existing memory was untouched in all.

How to read this: the stubbed run (`cargo test -p memory-engine --test conflicts`) feeds 12 hand-designed pairs through the real proposal step and safety policy with a *given* verdict, including wrong ones, and checks that the existing Active memory (status, revision, text, updated time) never changes and no second Active memory appears: 12/12. This table is the same pairs with a live model deciding, then the same policy.

- Sample 05 (Friday vs Saturday): the live model said "unrelated" (a plain create), so the correction waits as an ordinary pending candidate. Nothing is overwritten, but accepting it as new would leave two Active memories that disagree. This is the dedup weakness found in B-08, seen from the other side.
- Sample 11 (opposite claim on the same kind): with the stub saying "duplicate" the candidate is merged as a source, since polarity is not checked for low-risk notes; the live model said conflict.
- Hand-designed and written by one author; one model, one run.
