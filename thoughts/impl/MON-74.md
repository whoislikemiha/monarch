# MON-74 — Fix: clicking outside spawn dialog closes it

## What was implemented
Removed the backdrop click handler from `SpawnDialog.svelte` so that clicking outside the "Extract Shadow" dialog no longer dismisses it.

## Key decisions
- Only the spawn dialog was changed — other dialogs (Confirm, Edit, Settings, etc.) intentionally dismiss on backdrop click and were left alone.
- Escape key and Cancel button remain the dismissal paths, which is appropriate for a form with multiple fields where accidental dismissal loses work.

## Files touched
- `src/lib/SpawnDialog.svelte` — removed `onclick={oncancel}` from `.overlay` div (line 338)

## What was left out
- No changes to other dialogs.
