## Test your own app

After any change a person could see in a GUI, open the build in the helper and
look at it before saying it works:

1. Build it the way the project does.
2. If `atlas apps` shows an older copy of your app that this task opened,
   close it with `atlas close --app NAME`. If you did not open it, ask the
   person first: another agent may be testing it.
3. `atlas open /path/to/Build/Products/Debug/App.app`, adding
   `--env NAME=VALUE` for any launch variables, then `atlas wait --app NAME`.
   A build the helper cannot read is copied first into a folder the helper may
   read but not change.
4. Check that your build is the one running: `atlas apps` shows each app's
   `executable` path (`atlas apps --all` for a build that runs in the
   background only).
5. Exercise exactly what you changed. Check each state by text where you can,
   and with a crop of the window where you cannot; take a screenshot as
   evidence of each visible state you report. Check the edges too: empty
   states, long text, errors, a second launch.

The helper is its own account, with its own settings, documents, menu bar and
permissions, so an app launched there never changes the person's. If the app
asks for a permission or a sign-in there, tell the person: they allow it or
sign in inside the helper once, and it stays.

### Tools and apps the task needs

`atlas readiness --tool NAME --app NAME` checks the tools and installed apps
the task depends on (not the app you are building: open that by its path).
Copy a `missing` tool in yourself (see Give the app what it needs) and check
again; if something cannot be copied, stop and say so. Name tools by their
bare command name (`cat`, not `/bin/cat`); a found tool's `context` says where
it was found, `path` (the helper's PATH, `.local/bin` included) or
`environment` (a profile). `signIn: unknown` means look at the app before
assuming you are signed in.

### Installing an app

To test installing an app, or an app that must run from Applications,
`atlas install-app /path/App.app` copies it into the helper's
`~/Applications`, never over an app already there, and prints its `path`;
then `atlas open --app NAME`. `atlas cleanup --apply` moves it aside in
the helper when you are done (`held` gives its path; it is removed 7 days
later), unless it changed meanwhile (`kept`). Settings and data the app
wrote in the helper stay. An app you put there any other way is not moved
for you.

### Finder windows

Atlas never quits Finder: close Finder windows you opened with
`atlas key cmd+w`.
