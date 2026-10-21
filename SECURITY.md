# Security policy

Atlas posts input, takes screenshots and runs commands inside a second macOS
login on your Mac, so we take security reports seriously.

## Reporting a problem

Email [ismael@insanearts.io](mailto:ismael@insanearts.io). Please don't open a
public issue.

Include what you found, the steps to reproduce it, the Atlas version
(`atlas --version`) and your macOS version. We'll reply within a few days and
keep you updated until it's fixed. Once a fix ships, we'll credit you in the
release notes unless you'd rather we didn't.

## What counts

We especially want to hear about anything that lets:

- input, screenshots or commands reach any login other than the configured
  helper, or reach the helper while it is on screen;
- another user, or an app running in the helper, use Atlas's sockets or the
  viewer's local address;
- Atlas change privacy settings, read or store passwords, or copy files back
  from the helper;
- an update or install run code that isn't Atlas's signed release.

## Supported versions

Fixes go into the latest release. Update with `atlas update`.
