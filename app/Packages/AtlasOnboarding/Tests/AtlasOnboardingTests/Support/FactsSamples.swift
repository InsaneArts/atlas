import AtlasOnboardingTesting
import Foundation

@testable import AtlasOnboarding

/// Facts as the person's account reads them, with `helper` as its helper side, and `vm` a VM being added.
func controller(
  _ helper: [String: Any]? = nil,
  finished: Bool = false,
  leftovers: [Int] = [],
  vm: [String: Any]? = nil
) throws -> OnboardingFacts {
  var reply: [String: Any] = [
    "ok": true, "role": "controller",
    "me": ["name": "theiskaa", "realName": "Ismayil"],
    "app": ["installed": true, "signed": true, "linked": true],
    "configured": helper != nil, "finished": finished,
    "candidates": [["name": "qa", "realName": "QA Account"]],
    "leftoverLogins": leftovers.map { ["uid": $0, "kind": "orphan"] },
  ]
  if let helper { reply["helper"] = helper }
  if let vm { reply["vm"] = vm }
  return try facts(reply)
}

/// Facts as the person's account reads them with settings that list the VM `vm` alone.
func onVM(_ vm: [String: Any], room: [String: Any] = ["available": true]) throws -> OnboardingFacts {
  try facts([
    "ok": true, "role": "controller", "me": ["name": "theiskaa"],
    "app": ["installed": true, "signed": true],
    "configured": true, "finished": vm["ready"] as? Bool ?? false, "candidates": [],
    "desktops": ["vm": room], "vm": vm,
  ])
}

/// The VM of a first setup as the status reply writes it, by default being added, its add running.
func vm(
  adding: Bool = true,
  steps: [String] = [],
  progress: String? = nil,
  setup: String? = nil,
  adderRunning: Any = true,
  failure: String? = nil,
  ready: Bool = false
) -> [String: Any] {
  var vm: [String: Any] = [
    "name": "vm", "kind": "vm", "listed": true, "lifecycle": adding ? "adding" : "created",
    "steps": steps, "ready": ready,
    "adder": adding ? ["running": adderRunning] : NSNull(),
  ]
  if let progress {
    vm["progress"] = ["phase": progress, "received": 12_400_000_000, "total": 25_600_000_000, "fraction": 0.48]
  }
  if let setup { vm["setup"] = setup }
  if let failure { vm["failure"] = failure }
  return vm
}

/// The helper side of a status reply, by default set up and allowed.
func helper(
  signedIn: Bool = true,
  answering: Bool = true,
  hidden: Bool = true,
  allowed: Bool = true,
  admin: Bool = false,
  paired: Bool = true
) -> [String: Any] {
  [
    "exists": true, "name": "atlas", "realName": "Atlas",
    "administrator": admin, "paired": paired,
    "signedIn": signedIn, "answering": answering, "onConsole": !hidden, "hidden": hidden,
    "permissions": ["screenCapture": allowed, "accessibility": allowed],
  ]
}

/// Facts as the helper's account reads them, asked by `name` (no one when `nil`).
func asker(
  name: String? = "theiskaa",
  installed: Bool,
  running: Bool,
  ax: Bool,
  screen: Bool
) throws
  -> OnboardingFacts
{
  let controller: Any = name.map { ["uid": 501, "name": $0, "realName": "Ismayil"] } ?? NSNull()
  return try facts([
    "role": "helper", "me": ["name": "atlas"],
    "app": ["installed": true, "signed": true],
    "helper": [
      "controller": controller, "installed": installed, "running": running,
      "permissions": ["screenCapture": screen, "accessibility": ax],
    ],
  ])
}
