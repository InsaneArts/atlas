# The frame pipe

Atlas.app's viewer window and the terminal viewer show the hidden helper through one shared stream of pictures, the frame pipe. The menu bar runs it beside its viewer service, so it lives as long as the menu bar does.

## What it does

- **One source.** Only the frame service reads the helper's live stream, one picture at a time, on demand. Every window and terminal shares it, so opening a second viewer never starts a second capture. With no viewer asking for two seconds, the helper's stream is given back.
- **Raw pictures.** Pictures travel as packed RGB over two private Unix sockets, never as PNG. A picture is read while the previous one travels, so a viewer receives about as many pictures as the helper can capture (about 27 a second at 1600 pixels wide on a 2560 × 1440 helper, against one picture every two seconds before).
- **Verified pictures only.** Every picture is bracketed by the helper's session proofs, as before, and is refused once it is older than half a second.
- **Committed pictures name input.** A viewer acknowledges each picture it shows. Only those can be named by Take Control and input, which go through the menu bar viewer's guarded routes with the same leases, frame keys and session checks as before.

When the frame service is unavailable, the app reads pictures over the viewer's address as before, and tries the pipe again after a pause. Agents' screenshots and the menu bar panel keep their own reads.

## Private by construction

The service listens only in a folder `/private/tmp/atlas-fp.<uid>.<key>/` that belongs to the controller account, with mode `0700` and no access control list. Each socket is mode `0600`. Atlas refuses an unsafe folder or socket rather than repairing it, and removes only the sockets it bound.

A client must hold the observer capability from the viewer's private address to pair. The kernel must name the same process on both sockets, and the client checks that the service's own process answers. Every packet has a fixed 32-byte prefix and strict JSON metadata: duplicate keys, unknown keys and lengths past their bounds end the connection before anything is reserved.

## Diagnostics

`atlas monitor --terminal` shares the frame service when the menu bar runs for the same helper. Otherwise it uses its own private viewer and the helper's stream directly, as it did before, and `--max-dimension` or `--capture-backend sck` select that private path explicitly.
