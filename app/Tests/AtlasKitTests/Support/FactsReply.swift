import Foundation
import Testing

@testable import AtlasKit

/// Facts from a status reply as `atlas onboarding status` writes it.
///
/// - Parameter reply: The reply's JSON fields.
/// - Returns: The facts.
/// - Throws: When the reply is not a status.
func facts(_ reply: [String: Any]) throws -> OnboardingFacts {
  try #require(OnboardingFacts(json: JSONSerialization.data(withJSONObject: reply)))
}

/// Facts as the person's account reads them, with `helper` as its helper side.
func controller(_ helper: [String: Any]? = nil, finished: Bool = false) throws -> OnboardingFacts {
  var reply: [String: Any] = [
    "ok": true, "role": "controller",
    "me": ["name": "theiskaa", "realName": "Ismayil"],
    "app": ["installed": true, "signed": true, "linked": true],
    "configured": helper != nil, "finished": finished,
    "candidates": [["name": "qa", "realName": "QA Account"]],
  ]
  if let helper { reply["helper"] = helper }
  return try facts(reply)
}

/// The helper side of a status reply, by default set up and allowed.
func helper(
  signedIn: Bool = true,
  answering: Bool = true,
  hidden: Bool = true,
  allowed: Bool = true,
  admin: Bool = false,
  leftover: Bool = false,
  paired: Bool = true
) -> [String: Any] {
  [
    "exists": true, "name": "atlas", "realName": "Atlas",
    "administrator": admin, "leftoverSession": leftover, "paired": paired,
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
