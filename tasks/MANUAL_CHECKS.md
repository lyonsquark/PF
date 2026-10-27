# Manual checks for Adam (run `cargo run -p pf`)

Things the tests cannot judge: how things look and feel. Tick them off as you go.

## Account screen (T15b, T15c, T16)
- [ ] Click a payee, amount, memo, category, contexts and status: each edit is one Edit-menu entry, and ⌘Z reverses it.
- [ ] Esc cancels an in-progress payee or amount edit.
- [ ] Right-click anywhere on a row: Add to Budget, Duplicate, Delete appear. Duplicate selects the copies.
- [ ] Find button and ⌘F open and close the search bar; try each scope, a date range, and the "N results" count.
- [ ] Attach a PDF, Open it, remove it. Try a .exe to see the "Unsafe Attachment" alert.
- [ ] Hover a row: Attach, Open and the tag "+" appear. Off the row they hide.
- [ ] Sideways scroll stops at both ends (no rubber band at the left).
- [ ] "+ Add Transaction" (blue) and ⌘N: Save, Save & Another, transfers, Rec.
- [ ] Sidebar "+" adds an account. Edit Account: Calculate fills the starting balance; Cancel discards.

## Budget (T17)
- [ ] Top-aligned amounts in the table; income green.
- [ ] Add a Specific Months item (ticks Jan/Apr/Jul), an Expense (name fills from category), and an End Date.
- [ ] Double-click a row opens the editor (not tested automatically).
- [ ] Project into a new scenario: the count, the item list with previews, All/None per section, auto-close.
- [ ] Reproject appears in the editor only after a projection.
- [ ] Sideways scroll stops at both ends.

## Obligations (T18)
- [ ] Add one (context name fills from the name), move it to Spending from its detail, delete it and undo.
- [ ] Empty status sections are hidden.
- [ ] In a scenario, the card balance stays the Reality balance.
- [ ] Detail: progress bar, Set / remove due date, notes commit on focus loss.

## Scenarios (T19)
- [ ] Click a scenario to activate it: the toolbar menu and the dashboard follow.
- [ ] Edit a scenario; Create a new one (purple dot).
- [ ] Delete the active scenario, then ⌘Z.

## Categories and contexts (T20)
- [ ] Double-click a category to edit; a single click does nothing.
- [ ] Create a child category: it sits under its parent, indented.
- [ ] Type a bad pattern (e.g. "[bad") on Save: the alert lists it and it is removed.
- [ ] Apply in the editor: the count message appears and the editor stays open.
- [ ] Archive, Unarchive, and Delete a context that an obligation uses (refused with a message).
- [ ] Emoji category icons draw (they used to crash the app; now enabled).

## Known gaps to decide on
- Vertical scrolling may also rubber-band at the top and bottom (egui's own scroll). Tell me if it bothers you.
- Glyphs were replaced with plain text (+, •, ×, -, Open/Attach, "Active", "Edit", "Apply"). Emoji now draw. Say if you want the symbols back where the font has them.
