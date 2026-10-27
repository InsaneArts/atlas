## Give the app what it needs

The helper starts with none of the person's files or settings. When the app
needs some (its settings, a sample document, a tool), copy them yourself;
never ask the person to do it:

1. Write a manifest outside the project, such as in a temporary folder: one
   `[[files]]` table per file, with `source` (the absolute path on this Mac),
   `target` (a plain ASCII path inside the helper's home, like
   `.config/opie/rings.toml`) and `executable = true` for tools (put them in
   `.local/bin`, where `atlas run -- TOOL ARGS` finds them). Copy only what
   the app needs, never credentials, keychains or tokens.
2. Quit the app in the helper first (`atlas close --app NAME`), so it does not
   overwrite the files while they arrive.
3. `atlas environment MANIFEST` previews without changing anything. Read each
   file's `operation`: `conflict` means the helper already has a file Atlas
   did not put there, so stop and tell the person rather than working around
   it. A target inside, around or differing only in case from a file copied
   earlier is refused: roll that copy back first. Keep the `planId`.
4. `atlas environment apply PLAN_ID` copies exactly what was previewed, as a
   one-time snapshot. Keep every `planId` you apply. If it stops partway
   (`resumeRequired`, with `pendingTargets`), run the same apply again to
   finish, or roll it back. With `rollbackRequired`, a file changed in the
   helper meanwhile, so only rolling back finishes.

`atlas environment discover`, `profile` and `environment run` inspect or
freeze a selected set of tools and variables (see `atlas environment --help`);
a test seldom needs them.

Nothing is ever copied back to the person's account.

### Rolling back

`atlas environment rollback PLAN_ID --apply` undoes one copy. Roll back every
copy you applied, newest first, and check each reply says `rolledBack`. Files
you added are removed and replaced ones come back. A file changed in the
helper since the copy stays as it is (`keptTargets`): tell the person. A file
a stopped copy never put in place stays as the helper has it.

Without `--apply` it only previews: `pendingTargets` lists what would be
undone, and each file's `operation` still names what the copy did (`create`
is undone by removing the file, the others by putting the earlier file back).
Folders the copy made go too, unless something else now uses them
(`retainedDirectories`).
