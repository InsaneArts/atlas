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
3. `atlas environment MANIFEST` previews without changing anything, even while
   someone else is acting in the helper. Read each file's `operation`:
   `conflict` means the helper already has a file Atlas did not put there, so
   stop and tell the person rather than working around it. A target inside,
   around or differing only in case from a file copied earlier is refused:
   roll that copy back first. Keep the `planId`.
4. `atlas environment apply PLAN_ID` copies exactly what was previewed, as a
   one-time snapshot. Keep every `planId` you apply; `atlas activity` lists
   them again if you lose them. If it stops partway
   (`resumeRequired`, with `pendingTargets`), run the same apply again to
   finish, or roll it back. With `rollbackRequired`, a file changed in the
   helper meanwhile, so only rolling back finishes.

`atlas environment discover`, `profile` and `environment run` inspect or
freeze a selected set of tools and variables (see `atlas environment --help`);
a test seldom needs them. Discovery only reads, so it works while someone else
is acting in the helper.

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

If you lost a `planId`, `atlas environment list` shows each copy into the
helper, newest first, with its `state` and the files it copies, never their
contents; `thisTask` marks those this task made, and `session` names the
session that made one. `applying` and `rollingBack` stopped partway: finish
them with `atlas environment apply PLAN_ID` or
`atlas environment rollback PLAN_ID --apply`. `--all` also lists those rolled
back completely. To keep one part of a long session's copies apart, run that
part's commands with `ATLAS_TASK=NAME`.
